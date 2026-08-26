use axum::http::HeaderMap;
use subtle::ConstantTimeEq;

use crate::{
    crypto::{decode_hex, sha256},
    network::{ApiRole, ApiTokenConfig},
};

use super::errors::{ApiError, ApiResult};

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub identity: String,
    pub role: ApiRole,
}

impl AuthContext {
    pub fn require(&self, minimum: ApiRole) -> ApiResult<()> {
        if role_level(self.role) < role_level(minimum) {
            return Err(ApiError::forbidden());
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct AuthService {
    tokens: Vec<TokenRecord>,
}

#[derive(Clone)]
struct TokenRecord {
    identity: String,
    role: ApiRole,
    digest: [u8; 32],
}

impl AuthService {
    pub fn new(tokens: &[ApiTokenConfig]) -> ApiResult<Self> {
        let tokens = tokens
            .iter()
            .map(|token| {
                let digest: [u8; 32] = decode_hex(&token.token_sha256)
                    .ok()
                    .and_then(|bytes| bytes.try_into().ok())
                    .ok_or_else(|| ApiError::validation("API token digest must be SHA-256"))?;
                Ok(TokenRecord {
                    identity: token.identity.clone(),
                    role: token.role,
                    digest,
                })
            })
            .collect::<ApiResult<Vec<_>>>()?;
        Ok(Self { tokens })
    }

    pub fn authenticate(&self, headers: &HeaderMap) -> ApiResult<AuthContext> {
        let header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(ApiError::unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .filter(|value| !value.is_empty())
            .ok_or_else(ApiError::unauthorized)?;
        self.authenticate_token(token)
    }

    pub fn authenticate_token(&self, token: &str) -> ApiResult<AuthContext> {
        if token.is_empty() {
            return Err(ApiError::unauthorized());
        }
        let candidate = sha256(token.as_bytes());
        self.tokens
            .iter()
            .find(|record| bool::from(record.digest.ct_eq(&candidate)))
            .map(|record| AuthContext {
                identity: record.identity.clone(),
                role: record.role,
            })
            .ok_or_else(ApiError::unauthorized)
    }
}

fn role_level(role: ApiRole) -> u8 {
    match role {
        ApiRole::Viewer => 1,
        ApiRole::Operator => 2,
        ApiRole::Admin => 3,
    }
}
