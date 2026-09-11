use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct ConfigResponse {
    pub id: String,
    pub key: String,
    pub value: Option<String>,
    pub secret: bool,
    pub group: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct ConfigCreateRequest {
    pub key: String,
    pub value: Option<String>,
    pub secret: bool,
    pub group: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct ConfigUpdateRequest {
    pub value: Option<String>,
    pub secret: Option<bool>,
    pub group: Option<String>,
    pub description: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ConfigResponse>>, StatusCode> {
    let repo = cc_store::config_repo::ConfigRepo::new(&state.store);
    let configs = repo.list(None).map_err(super::db_err)?;

    let responses: Vec<ConfigResponse> = configs.into_iter().map(|c| ConfigResponse {
        id: c.id,
        key: c.key,
        value: if c.secret { Some("***".to_string()) } else { c.value },
        secret: c.secret,
        group: c.group,
        description: c.description,
    }).collect();

    Ok(Json(responses))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ConfigResponse>, StatusCode> {
    let repo = cc_store::config_repo::ConfigRepo::new(&state.store);
    let config = repo.get(&id).map_err(super::db_err)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ConfigResponse {
        id: config.id,
        key: config.key,
        value: if config.secret { Some("***".to_string()) } else { config.value },
        secret: config.secret,
        group: config.group,
        description: config.description,
    }))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ConfigCreateRequest>,
) -> Result<Json<ConfigResponse>, StatusCode> {
    let repo = cc_store::config_repo::ConfigRepo::new(&state.store);
    let config = repo.create(cc_core::config::ConfigCreate {
        key: input.key,
        value: input.value,
        secret: input.secret,
        group: input.group,
        description: input.description,
    }).map_err(super::db_err)?;

    let _ = state.tx.send("config_created".to_string());

    Ok(Json(ConfigResponse {
        id: config.id,
        key: config.key,
        value: if config.secret { Some("***".to_string()) } else { config.value },
        secret: config.secret,
        group: config.group,
        description: config.description,
    }))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ConfigUpdateRequest>,
) -> Result<Json<ConfigResponse>, StatusCode> {
    let repo = cc_store::config_repo::ConfigRepo::new(&state.store);
    let config = repo.update(&id, cc_core::config::ConfigUpdate {
        value: input.value,
        secret: input.secret,
        group: input.group,
        description: input.description,
    }).map_err(super::db_err)?;

    let _ = state.tx.send(format!("config_updated:{}", id));

    Ok(Json(ConfigResponse {
        id: config.id,
        key: config.key,
        value: if config.secret { Some("***".to_string()) } else { config.value },
        secret: config.secret,
        group: config.group,
        description: config.description,
    }))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let repo = cc_store::config_repo::ConfigRepo::new(&state.store);
    repo.delete(&id).map_err(super::db_err)?;

    let _ = state.tx.send(format!("config_deleted:{}", id));

    Ok(StatusCode::NO_CONTENT)
}

pub async fn history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<cc_core::config::ConfigHistory>>, StatusCode> {
    let repo = cc_store::config_repo::ConfigRepo::new(&state.store);
    let history = repo.history(&id, 50).map_err(super::db_err)?;
    Ok(Json(history))
}
