use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::AppState;

/// Bearer token from `Authorization: Bearer <t>` header,
/// or `?token=<t>` query fallback (EventSource can't set headers).
pub fn extract_bearer(req: &Request) -> Option<String> {
    if let Some(h) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(s) = h.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return Some(t.trim().to_string());
            }
        }
    }
    req.uri().query().and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("token=").map(|v| v.to_string()))
    })
}

pub async fn require_token(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.auth_token.is_none() {
        // No token configured: only reachable on loopback (enforced at boot).
        return Ok(next.run(req).await);
    }
    match extract_bearer(&req) {
        Some(t) => {
            let check =
                cc_store::token_repo::TokenRepo::new(&state.store).check(&t);
            if !check.valid {
                tracing::warn!(path = %req.uri().path(), "rejected: bad or expired token");
                return Err(StatusCode::UNAUTHORIZED);
            }
            // Read tokens are consumption-only: any write requires admin.
            if !matches!(req.method(), &Method::GET | &Method::HEAD)
                && check.permissions != "admin"
            {
                tracing::warn!(
                    method = %req.method(),
                    path = %req.uri().path(),
                    "write blocked: token lacks admin permission"
                );
                return Err(StatusCode::FORBIDDEN);
            }
            if state.readonly && req.method() != Method::GET {
                tracing::warn!(method = %req.method(), path = %req.uri().path(), "write blocked: readonly mode");
                return Err(StatusCode::FORBIDDEN);
            }
            Ok(next.run(req).await)
        }
        None => {
            tracing::warn!(path = %req.uri().path(), "rejected: missing token");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
