use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::Value;

use crate::network::NodeConfig;

use super::error::{CliError, CliResult};

#[derive(Clone)]
pub struct ApiClient {
    client: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl ApiClient {
    pub fn from_config(config: &NodeConfig) -> CliResult<Self> {
        let api = config
            .api
            .as_ref()
            .ok_or_else(|| CliError::unavailable("The REST API is not configured"))?;
        let token = std::env::var("MINICHAIN_API_TOKEN").ok();
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|error| {
                    CliError::unavailable("Could not initialize HTTP client")
                        .reason(error.to_string())
                })?,
            base: format!("http://{}/api/v1", api.listen_address),
            token,
        })
    }

    pub async fn public_get(&self, path: &str) -> CliResult<Value> {
        self.request(Method::GET, path, Option::<&Value>::None, false)
            .await
    }

    pub async fn get(&self, path: &str) -> CliResult<Value> {
        self.request(Method::GET, path, Option::<&Value>::None, true)
            .await
    }

    pub async fn post_empty(&self, path: &str) -> CliResult<Value> {
        self.request(Method::POST, path, Option::<&Value>::None, true)
            .await
    }

    pub async fn post<T: Serialize + ?Sized>(&self, path: &str, body: &T) -> CliResult<Value> {
        self.request(Method::POST, path, Some(body), true).await
    }

    async fn request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&T>,
        protected: bool,
    ) -> CliResult<Value> {
        let mut request = self
            .client
            .request(method, format!("{}{}", self.base, path));
        if protected {
            let token = self.token.as_deref().ok_or_else(|| {
                CliError::unavailable("MINICHAIN_API_TOKEN is required for this live-node command")
            })?;
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|error| {
            CliError::unavailable("The node API is unavailable").reason(error.to_string())
        })?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(|error| {
            CliError::unavailable("The node API returned invalid JSON").reason(error.to_string())
        })?;
        if status.is_success() {
            return Ok(value);
        }
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("The node rejected the request");
        let code = match status {
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => 3,
            StatusCode::SERVICE_UNAVAILABLE => 4,
            StatusCode::CONFLICT => 5,
            _ => 1,
        };
        Err(CliError::new(code, message))
    }
}
