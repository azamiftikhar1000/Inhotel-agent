use crate::server::AppState;
use axum::{body::Body, extract::State, middleware::Next, response::Response};
use http::Request;
use jsonwebtoken::{DecodingKey, Validation};
use osentities::{
    constant::{DEFAULT_AUDIENCE, DEFAULT_ISSUER, FALLBACK_AUDIENCE, FALLBACK_ISSUER},
    ApplicationError, Claims, PicaError, BEARER_PREFIX,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct JwtState {
    validation: Validation,
    jwt_secret: String,
    buildable_secret: String,
}

impl JwtState {
    pub fn from_state(state: &Arc<AppState>) -> Self {
        let mut validation = Validation::default();
        validation.set_audience(&[DEFAULT_AUDIENCE, FALLBACK_AUDIENCE]);
        validation.set_issuer(&[DEFAULT_ISSUER, FALLBACK_ISSUER]);

        // Get buildable_secret from environment if not in config
        let buildable_secret = std::env::var("BUILDABLE_SECRET")
            .unwrap_or_else(|_| "".to_string());

        Self {
            validation,
            jwt_secret: state.config.jwt_secret.clone(),
            buildable_secret,
        }
    }
}

pub async fn jwt_auth_middleware(
    State(state): State<Arc<JwtState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, PicaError> {
    let Some(auth_header) = req.headers().get(http::header::AUTHORIZATION) else {
        return Err(ApplicationError::unauthorized(
            "You are not authorized to access this resource",
            None,
        ));
    };

    let Ok(auth_header) = auth_header.to_str() else {
        return Err(ApplicationError::unauthorized(
            "You are not authorized to access this resource",
            None,
        ));
    };

    if !auth_header.starts_with(BEARER_PREFIX) {
        return Err(ApplicationError::unauthorized(
            "You are not authorized to access this resource",
            None,
        ));
    };

    let token = &auth_header[BEARER_PREFIX.len()..];

    // 1) First decode header to check algorithm
    let header = match jsonwebtoken::decode_header(token) {
        Ok(h) => h,
        Err(_) => {
            return Err(ApplicationError::unauthorized("Invalid token format", None));
        }
    };

    // 2) Try to decode claims with minimal validation to determine token type
    let mut minimal_validation = Validation::default();
    minimal_validation.insecure_disable_signature_validation();
    minimal_validation.validate_exp = false;
    minimal_validation.validate_nbf = false;
    
    let unverified_claims = match jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(&[]), // Empty key since we're not validating signature
        &minimal_validation,
    ) {
        Ok(token_data) => token_data.claims,
        Err(_) => {
            return Err(ApplicationError::unauthorized("Invalid token structure", None));
        }
    };

    // 3) Determine which secret to use for verification
    let composite_secret = if unverified_claims.is_buildable_core {
        format!("{}{}", state.buildable_secret, state.jwt_secret)
    } else if unverified_claims.buildable_id.is_empty() {
        state.jwt_secret.clone()
    } else {
        format!("{}{}", state.jwt_secret, unverified_claims.buildable_id)
    };

    // 4) Do final verification with correct secret and full validation
    let final_key = DecodingKey::from_secret(composite_secret.as_bytes());
    
    match jsonwebtoken::decode::<Claims>(token, &final_key, &state.validation) {
        Ok(decoded_token) => {
            req.extensions_mut().insert(Arc::new(decoded_token.claims));
            Ok(next.run(req).await)
        }
        Err(_) => {
            Err(ApplicationError::unauthorized("Invalid token signature", None))
        }
    }
}
