use axum::{
    Json,
    extract::{State, Path, Query},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct ProjectResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct ProjectCreateRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddConfigRequest {
    pub config_id: String,
    pub env_id: String,
    pub override_value: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProjectResponse>>, StatusCode> {
    let repo = cc_store::project_repo::ProjectRepo::new(&state.store);
    let projects = repo.list().map_err(super::db_err)?;

    let responses: Vec<ProjectResponse> = projects.into_iter().map(|p| ProjectResponse {
        id: p.id,
        name: p.name,
        description: p.description,
    }).collect();

    Ok(Json(responses))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProjectResponse>, StatusCode> {
    let repo = cc_store::project_repo::ProjectRepo::new(&state.store);
    let project = repo.get(&id).map_err(super::db_err)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ProjectResponse {
        id: project.id,
        name: project.name,
        description: project.description,
    }))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ProjectCreateRequest>,
) -> Result<Json<ProjectResponse>, StatusCode> {
    let repo = cc_store::project_repo::ProjectRepo::new(&state.store);
    let project = repo.create(cc_core::config::ProjectCreate {
        name: input.name,
        description: input.description,
    }).map_err(super::db_err)?;

    Ok(Json(ProjectResponse {
        id: project.id,
        name: project.name,
        description: project.description,
    }))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let repo = cc_store::project_repo::ProjectRepo::new(&state.store);
    repo.delete(&id).map_err(super::db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_configs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<cc_core::config::ProjectConfig>>, StatusCode> {
    let repo = cc_store::project_repo::ProjectRepo::new(&state.store);
    let configs = repo.list_configs(&id).map_err(super::db_err)?;
    Ok(Json(configs))
}

pub async fn add_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<AddConfigRequest>,
) -> Result<Json<cc_core::config::ProjectConfig>, StatusCode> {
    let repo = cc_store::project_repo::ProjectRepo::new(&state.store);
    let config = repo.add_config(&id, cc_core::config::ProjectConfigCreate {
        config_id: input.config_id,
        env_id: input.env_id,
        override_value: input.override_value,
    }).map_err(super::db_err)?;

    Ok(Json(config))
}

/* ── resolved configs (SDK / consumers) ────────────────────────────── */

#[derive(Deserialize)]
pub struct ResolvedQuery {
    /// Environment id or name. Defaults to `base`, else the first env.
    pub env: Option<String>,
    /// Reveal secret values (requires an admin token when auth is enabled).
    /// Accepts true/1/yes.
    pub reveal: Option<String>,
    /// Token fallback for clients that cannot set headers.
    pub token: Option<String>,
}

#[derive(Serialize)]
pub struct ResolvedConfig {
    pub id: String,
    pub key: String,
    /// null for secrets that were not revealed.
    pub value: Option<String>,
    pub secret: bool,
    pub source: String,
    pub group: Option<String>,
    pub description: Option<String>,
}

fn bearer_from(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .or_else(|| query_token.map(|s| s.to_string()))
}

fn can_reveal(state: &AppState, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    if state.auth_token.is_none() {
        // No auth configured: loopback trusted deployment.
        return true;
    }
    bearer_from(headers, query_token)
        .map(|t| cc_store::token_repo::TokenRepo::new(&state.store).check(&t).permissions == "admin")
        .unwrap_or(false)
}

pub async fn resolved(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<ResolvedQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<ResolvedConfig>>, StatusCode> {
    // Attachments define inclusion; env chain + project overrides applied.
    let scope = cc_store::resolve::resolve_project_scope(&state.store, &id, q.env.as_deref())
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("No environments") {
                tracing::warn!(error = %msg, "resolved: not found");
                StatusCode::NOT_FOUND
            } else {
                tracing::warn!(error = %msg, "resolved failed");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    let want_reveal = q
        .reveal
        .as_deref()
        .map(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let reveal_ok = want_reveal && can_reveal(&state, &headers, q.token.as_deref());

    let out: Vec<ResolvedConfig> = scope
        .configs
        .iter()
        .map(|c| {
            let source = match &c.source {
                cc_core::config::ConfigSource::Base => "base".to_string(),
                cc_core::config::ConfigSource::Environment(n) => format!("env:{}", n),
                cc_core::config::ConfigSource::ProjectOverride(n) => format!("project:{}", n),
            };
            let value = if c.config.secret {
                if reveal_ok {
                    if cc_core::crypto::is_encrypted(&c.effective_value) {
                        match cc_core::crypto::get_or_create_master_key()
                            .and_then(|k| cc_core::crypto::decrypt(&c.effective_value, &k))
                        {
                            Ok(v) => Some(v),
                            Err(e) => {
                                tracing::warn!(key = %c.config.key, error = %e, "reveal failed; returning null");
                                None
                            }
                        }
                    } else {
                        Some(c.effective_value.clone())
                    }
                } else {
                    None
                }
            } else {
                Some(c.effective_value.clone())
            };
            ResolvedConfig {
                id: c.config.id.clone(),
                key: c.config.key.clone(),
                value,
                secret: c.config.secret,
                source,
                group: c.config.group.clone(),
                description: c.config.description.clone(),
            }
        })
        .collect();

    Ok(Json(out))
}
