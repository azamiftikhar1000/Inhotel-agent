use crate::server::AppState;
use axum::{body::Body, extract::State, middleware::Next, response::Response};
use http::Request;
use jsonwebtoken::{DecodingKey, Validation};
use osentities::{
    constant::{DEFAULT_AUDIENCE, DEFAULT_ISSUER, FALLBACK_AUDIENCE, FALLBACK_ISSUER},
    ApplicationError, Claims, PicaError, BEARER_PREFIX,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

/// Minimal claims struct for peeking at isBuildableCore without full validation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialClaims {
    #[serde(default)]
    is_buildable_core: bool,
    #[serde(default)]
    buildable_id: Option<String>,
}

#[derive(Clone)]
pub struct JwtState {
    validation: Validation,
    /// Decoding key for core tokens (BUILDABLE_SECRET + JWT_SECRET)
    core_decoding_key: DecodingKey,
    /// Base JWT secret for user tokens (JWT_SECRET + buildableId)
    base_jwt_secret: String,
}

impl JwtState {
    pub fn from_state(state: &Arc<AppState>) -> Self {
        let mut validation = Validation::default();
        validation.set_audience(&[DEFAULT_AUDIENCE, FALLBACK_AUDIENCE]);
        validation.set_issuer(&[DEFAULT_ISSUER, FALLBACK_ISSUER]);

        // Core secret: BUILDABLE_SECRET + JWT_SECRET (for isBuildableCore: true from internal services)
        let core_secret = format!(
            "{}{}",
            state.config.buildable_secret, state.config.jwt_secret
        );

        Self {
            validation,
            core_decoding_key: DecodingKey::from_secret(core_secret.as_bytes()),
            base_jwt_secret: state.config.jwt_secret.clone(),
        }
    }

    /// Get the appropriate decoding key based on token claims (direct matching, no fallback)
    fn get_decoding_key(&self, token: &str) -> Result<DecodingKey, PicaError> {
        // Decode without verification to peek at claims
        let mut peek_validation = Validation::default();
        peek_validation.insecure_disable_signature_validation();
        peek_validation.set_audience(&[DEFAULT_AUDIENCE, FALLBACK_AUDIENCE]);
        peek_validation.set_issuer(&[DEFAULT_ISSUER, FALLBACK_ISSUER]);

        // Use a dummy key for peeking (signature validation is disabled)
        let dummy_key = DecodingKey::from_secret(b"dummy");

        let token_data = jsonwebtoken::decode::<PartialClaims>(token, &dummy_key, &peek_validation)
            .map_err(|e| {
                warn!("Failed to decode token claims: {:?}", e);
                ApplicationError::unauthorized("Invalid token format", None)
            })?;

        if token_data.claims.is_buildable_core {
            // isBuildableCore: true → Core token (service-to-service)
            // Uses: BUILDABLE_SECRET + JWT_SECRET
            info!("Token type: core (isBuildableCore: true)");
            Ok(self.core_decoding_key.clone())
        } else {
            // isBuildableCore: false → User token
            // Uses: JWT_SECRET + buildableId
            let buildable_id = token_data.claims.buildable_id.ok_or_else(|| {
                warn!("User token missing buildableId");
                ApplicationError::unauthorized("Invalid token: missing buildableId", None)
            })?;
            info!("Token type: user (buildableId: {})", buildable_id);
            let secret = format!("{}{}", self.base_jwt_secret, buildable_id);
            Ok(DecodingKey::from_secret(secret.as_bytes()))
        }
    }
}

pub async fn jwt_auth_middleware(
    State(state): State<Arc<JwtState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, PicaError> {
    let Some(auth_header) = req.headers().get(http::header::AUTHORIZATION) else {
        info!("missing authorization header");
        return Err(ApplicationError::unauthorized(
            "You are not authorized to access this resource",
            None,
        ));
    };

    let Ok(auth_header) = auth_header.to_str() else {
        info!("invalid authorization header");
        return Err(ApplicationError::unauthorized(
            "You are not authorized to access this resource",
            None,
        ));
    };

    if !auth_header.starts_with(BEARER_PREFIX) {
        info!("invalid authorization header");
        return Err(ApplicationError::unauthorized(
            "You are not authorized to access this resource",
            None,
        ));
    }

    let token = &auth_header[BEARER_PREFIX.len()..];

    // Get the appropriate decoding key based on token type (direct matching)
    let decoding_key = state.get_decoding_key(token)?;

    // Validate the token with the selected key
    match jsonwebtoken::decode::<Claims>(token, &decoding_key, &state.validation) {
        Ok(decoded_token) => {
            info!("JWT token validated successfully");
            req.extensions_mut().insert(Arc::new(decoded_token.claims));
            Ok(next.run(req).await)
        }
        Err(e) => {
            warn!("JWT validation failed: {:?}", e);
            Err(ApplicationError::forbidden(
                "You are not authorized to access this resource",
                None,
            ))
        }
    }
}
