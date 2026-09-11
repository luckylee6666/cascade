use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize)]
pub struct ImportQuery {
    pub group: Option<String>,
    /// true/1/yes to overwrite existing keys.
    pub overwrite: Option<String>,
}

/// `POST /api/import` — body is dotenv / JSON / YAML text (auto-detected).
/// AI-friendly: `curl -X POST --data-binary @file.env .../api/import`.
/// Requires an admin token when auth is enabled (writes are admin-only).
pub async fn import(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ImportQuery>,
    body: String,
) -> Result<Json<cc_store::import::ImportReport>, StatusCode> {
    let entries =
        cc_core::import::parse_import_text(&body, cc_core::import::ImportFormat::Auto)
            .map_err(|e| {
                tracing::warn!(error = %e, "import parse failed");
                StatusCode::BAD_REQUEST
            })?;
    if entries.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let overwrite = q
        .overwrite
        .as_deref()
        .map(|s| matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    let report = cc_store::import::import_entries(
        &state.store,
        &entries,
        &cc_store::import::ImportOptions {
            group: q.group,
            overwrite,
        },
    )
    .map_err(super::db_err)?;

    tracing::info!(
        added = report.added,
        updated = report.updated,
        skipped = report.skipped,
        secrets = report.secrets,
        "configs imported via HTTP"
    );
    Ok(Json(report))
}
