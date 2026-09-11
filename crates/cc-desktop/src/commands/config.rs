use tauri::State;
use cc_store::db::Store;
use cc_core::config::{ConfigCreate, ConfigUpdate};

#[tauri::command(rename_all = "snake_case")]
pub fn list_configs(state: State<'_, Store>) -> Result<Vec<cc_core::config::Config>, String> {
    let repo = cc_store::config_repo::ConfigRepo::new(state.inner());
    repo.list(None).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_config(state: State<'_, Store>, id: String) -> Result<cc_core::config::Config, String> {
    let repo = cc_store::config_repo::ConfigRepo::new(state.inner());
    repo.get(&id).map_err(|e| e.to_string())?.ok_or("Config not found".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_config(
    state: State<'_, Store>,
    key: String,
    value: Option<String>,
    secret: bool,
    group: Option<String>,
    description: Option<String>,
) -> Result<cc_core::config::Config, String> {
    let repo = cc_store::config_repo::ConfigRepo::new(state.inner());
    repo.create(ConfigCreate { key, value, secret, group, description })
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_config(
    state: State<'_, Store>,
    id: String,
    value: Option<String>,
    secret: Option<bool>,
    group: Option<String>,
    description: Option<String>,
) -> Result<cc_core::config::Config, String> {
    let repo = cc_store::config_repo::ConfigRepo::new(state.inner());
    repo.update(&id, ConfigUpdate { value, secret, group, description })
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_config(state: State<'_, Store>, id: String) -> Result<(), String> {
    let repo = cc_store::config_repo::ConfigRepo::new(state.inner());
    repo.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_config_history(
    state: State<'_, Store>,
    config_id: String,
) -> Result<Vec<cc_core::config::ConfigHistory>, String> {
    let repo = cc_store::history_repo::HistoryRepo::new(state.inner());
    repo.list(&config_id, 100).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn revert_config(state: State<'_, Store>, history_id: i64) -> Result<(), String> {
    let repo = cc_store::history_repo::HistoryRepo::new(state.inner());
    repo.revert(history_id).map_err(|e| e.to_string())
}
