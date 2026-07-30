use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{DecodingKey, Validation};

use crate::config::Config;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub async fn jwt_auth(
    State(config): State<std::sync::Arc<Config>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Missing Authorization header".to_owned()))?;

    let token_str = auth_header
        .to_str()
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid header".to_owned()))?;

    let Some(token) = token_str.strip_prefix("Bearer ") else {
        return Err((StatusCode::UNAUTHORIZED, "Invalid auth scheme".to_owned()));
    };

    let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());
    let validation = Validation::default();

    jsonwebtoken::decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid token".to_owned()))?;

    Ok(next.run(req).await)
}
