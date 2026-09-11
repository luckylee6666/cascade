use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct EnvResponse {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

#[derive(Deserialize)]
pub struct EnvCreateRequest {
    pub name: String,
    pub parent_id: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<EnvResponse>>, StatusCode> {
    let repo = cc_store::env_repo::EnvRepo::new(&state.store);
    let envs = repo.list().map_err(super::db_err)?;

    let responses: Vec<EnvResponse> = envs.into_iter().map(|e| EnvResponse {
        id: e.id,
        name: e.name,
        parent_id: e.parent_id,
    }).collect();

    Ok(Json(responses))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<EnvResponse>, StatusCode> {
    let repo = cc_store::env_repo::EnvRepo::new(&state.store);
    let env = repo.get(&id).map_err(super::db_err)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(EnvResponse {
        id: env.id,
        name: env.name,
        parent_id: env.parent_id,
    }))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<EnvCreateRequest>,
) -> Result<Json<EnvResponse>, StatusCode> {
    let repo = cc_store::env_repo::EnvRepo::new(&state.store);
    let env = repo.create(cc_core::config::EnvironmentCreate {
        name: input.name,
        parent_id: input.parent_id,
    }).map_err(super::db_err)?;

    Ok(Json(EnvResponse {
        id: env.id,
        name: env.name,
        parent_id: env.parent_id,
    }))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let repo = cc_store::env_repo::EnvRepo::new(&state.store);
    repo.delete(&id).map_err(super::db_err)?;
    Ok(StatusCode::NO_CONTENT)
}
