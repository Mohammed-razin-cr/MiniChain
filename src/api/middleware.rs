use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, Method},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

use super::{errors::ApiError, router::ApiState};

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum RateClass {
    Read,
    Write,
    Admin,
}

#[derive(Clone)]
pub(crate) struct RateLimiter {
    window: Duration,
    read_limit: u32,
    write_limit: u32,
    admin_limit: u32,
    buckets: Arc<Mutex<HashMap<(String, RateClass), Bucket>>>,
}

#[derive(Clone, Copy)]
struct Bucket {
    started: Instant,
    count: u32,
}

impl RateLimiter {
    pub fn new(config: &crate::network::ApiConfig) -> Self {
        Self {
            window: Duration::from_millis(config.rate_window_ms),
            read_limit: config.read_requests_per_window,
            write_limit: config.write_requests_per_window,
            admin_limit: config.admin_requests_per_window,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn check(&self, identity: &str, class: RateClass) -> Result<(), ApiError> {
        let limit = match class {
            RateClass::Read => self.read_limit,
            RateClass::Write => self.write_limit,
            RateClass::Admin => self.admin_limit,
        };
        let now = Instant::now();
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets
            .entry((identity.to_owned(), class))
            .or_insert(Bucket {
                started: now,
                count: 0,
            });
        if now.duration_since(bucket.started) >= self.window {
            bucket.started = now;
            bucket.count = 0;
        }
        if bucket.count >= limit {
            return Err(ApiError::new(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "The request rate limit has been exceeded",
            ));
        }
        bucket.count += 1;
        Ok(())
    }
}

pub(crate) async fn request_id(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(&REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
        .to_string();
    let value = HeaderValue::from_str(&id).expect("a UUID is a valid header value");
    request
        .headers_mut()
        .insert(REQUEST_ID.clone(), value.clone());
    let mut response = next.run(request).await;
    response.headers_mut().insert(REQUEST_ID, value);
    response
}

pub(crate) async fn authenticate_and_limit(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let request_id = request_id_value(&request);
    let started = Instant::now();
    let context = match state.auth.authenticate(request.headers()) {
        Ok(context) => context,
        Err(error) => {
            let class = rate_class(&method, &path);
            let response = match state.limiter.check("unauthenticated", class).await {
                Ok(()) => error.into_response(),
                Err(rate_error) => rate_error.into_response(),
            };
            log_request(
                &method,
                &path,
                response.status().as_u16(),
                started,
                "unauthenticated",
                &request_id,
            );
            return response;
        }
    };
    let class = rate_class(&method, &path);
    if let Err(error) = state.limiter.check(&context.identity, class).await {
        let response = error.into_response();
        log_request(
            &method,
            &path,
            response.status().as_u16(),
            started,
            &format!("{:?}", context.role),
            &request_id,
        );
        return response;
    }
    let role = format!("{:?}", context.role);
    request.extensions_mut().insert(context);
    let response = next.run(request).await;
    log_request(
        &method,
        &path,
        response.status().as_u16(),
        started,
        &role,
        &request_id,
    );
    response
}

pub(crate) async fn limit_public(
    State(state): State<ApiState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let request_id = request_id_value(&request);
    let started = Instant::now();
    if let Err(error) = state.limiter.check("public-health", RateClass::Read).await {
        let response = error.into_response();
        log_request(
            &method,
            &path,
            response.status().as_u16(),
            started,
            "public",
            &request_id,
        );
        return response;
    }
    let response = next.run(request).await;
    log_request(
        &method,
        &path,
        response.status().as_u16(),
        started,
        "public",
        &request_id,
    );
    response
}

fn rate_class(method: &Method, path: &str) -> RateClass {
    if path.contains("/snapshots") {
        RateClass::Admin
    } else if method == Method::GET {
        RateClass::Read
    } else {
        RateClass::Write
    }
}

fn request_id_value(request: &Request) -> String {
    request
        .headers()
        .get(&REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("pending")
        .to_owned()
}

fn log_request(
    method: &Method,
    path: &str,
    status: u16,
    started: Instant,
    role: &str,
    request_id: &str,
) {
    info!(
        %method,
        path,
        status,
        latency_ms = started.elapsed().as_millis() as u64,
        role,
        request_id,
        "HTTP request"
    );
}
