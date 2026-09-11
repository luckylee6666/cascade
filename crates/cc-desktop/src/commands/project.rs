use tauri::State;
use cc_store::db::Store;
use cc_core::config::ProjectCreate;

#[tauri::command(rename_all = "snake_case")]
pub fn list_projects(state: State<'_, Store>) -> Result<Vec<cc_core::config::Project>, String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.list().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_project(state: State<'_, Store>, id: String) -> Result<cc_core::config::Project, String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.get(&id).map_err(|e| e.to_string())?.ok_or("Project not found".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_project(
    state: State<'_, Store>,
    name: String,
    description: Option<String>,
) -> Result<cc_core::config::Project, String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.create(ProjectCreate { name, description }).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_project(state: State<'_, Store>, id: String) -> Result<(), String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_project_configs(
    state: State<'_, Store>,
    project_id: String,
) -> Result<Vec<cc_core::config::ProjectConfig>, String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.list_configs(&project_id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn add_project_config(
    state: State<'_, Store>,
    project_id: String,
    config_id: String,
    env_id: String,
    override_value: Option<String>,
) -> Result<cc_core::config::ProjectConfig, String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.add_config(
        &project_id,
        cc_core::config::ProjectConfigCreate {
            config_id,
            env_id,
            override_value,
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_project_config(
    state: State<'_, Store>,
    project_id: String,
    config_id: String,
    env_id: String,
) -> Result<(), String> {
    let repo = cc_store::project_repo::ProjectRepo::new(state.inner());
    repo.remove_config(&project_id, &config_id, &env_id)
        .map_err(|e| e.to_string())
}

/// Render the resolved config of a project/env into yaml / json / dotenv.
/// Attachments define inclusion; secrets are always masked (no reveal path).
#[tauri::command(rename_all = "snake_case")]
pub fn export_project(
    state: State<'_, Store>,
    project_id: String,
    env_id: String,
    format: String,
) -> Result<String, String> {
    let store = state.inner();
    let scope = cc_store::resolve::resolve_project_scope(store, &project_id, Some(&env_id))
        .map_err(|e| e.to_string())?;

    let mut pairs: Vec<(String, String)> = Vec::new();
    for c in &scope.configs {
        let value = if c.config.secret {
            format!(
                "${{{}}}",
                c.config.key.replace('.', "_").to_uppercase()
            )
        } else {
            c.effective_value.clone()
        };
        pairs.push((c.config.key.clone(), value));
    }

    let revision: u64 = store
        .conn()
        .query_row("SELECT COALESCE(MAX(id), 0) FROM config_history", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;

    match format.as_str() {
        "yaml" => cc_core::export::yaml::export_yaml(&pairs, revision).map_err(|e| e.to_string()),
        "json" => cc_core::export::json::export_json(&pairs, revision).map_err(|e| e.to_string()),
        "dotenv" => cc_core::export::dotenv::export_dotenv(&pairs, false).map_err(|e| e.to_string()),
        _ => Err(format!("unknown format: {}", format)),
    }
}
