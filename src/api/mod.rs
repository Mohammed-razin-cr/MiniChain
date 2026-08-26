mod auth;
mod dto;
mod errors;
mod events;
mod handlers;
mod middleware;
mod router;

pub use auth::AuthContext;
pub use dto::*;
pub use errors::{ApiError, ErrorBody, ErrorEnvelope};
pub use router::{ApiState, router, serve};
