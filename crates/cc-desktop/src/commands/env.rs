use tauri::State;
use cc_store::db::Store;
use cc_core::config::EnvironmentCreate;

#[tauri::command(rename_all = "snake_case")]
pub fn list_envs(state: State<'_, Store>) -> Result<Vec<cc_core::config::Environment>, String> {
    let repo = cc_store::env_repo::EnvRepo::new(state.inner());
    repo.list().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_env(state: State<'_, Store>, id: String) -> Result<cc_core::config::Environment, String> {
    let repo = cc_store::env_repo::EnvRepo::new(state.inner());
    repo.get(&id).map_err(|e| e.to_string())?.ok_or("Environment not found".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_env(
    state: State<'_, Store>,
    name: String,
    parent_id: Option<String>,
) -> Result<cc_core::config::Environment, String> {
    let repo = cc_store::env_repo::EnvRepo::new(state.inner());
    repo.create(EnvironmentCreate { name, parent_id }).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_env(state: State<'_, Store>, id: String) -> Result<(), String> {
    let repo = cc_store::env_repo::EnvRepo::new(state.inner());
    repo.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_env_values(
    state: State<'_, Store>,
) -> Result<Vec<cc_core::config::ConfigEnvValue>, String> {
    let repo = cc_store::env_value_repo::EnvValueRepo::new(state.inner());
    repo.list().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_env_value(
    state: State<'_, Store>,
    config_id: String,
    env_id: String,
    value: String,
) -> Result<(), String> {
    let store = state.inner();
    let config = cc_store::config_repo::ConfigRepo::new(store)
        .get(&config_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "config not found".to_string())?;
    let stored = if config.secret {
        let key = cc_core::crypto::get_or_create_master_key().map_err(|e| e.to_string())?;
        cc_core::crypto::encrypt(&value, &key).map_err(|e| e.to_string())?
    } else {
        value
    };
    cc_store::env_value_repo::EnvValueRepo::new(store)
        .set(&config_id, &env_id, &stored)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_env_value(
    state: State<'_, Store>,
    config_id: String,
    env_id: String,
) -> Result<(), String> {
    cc_store::env_value_repo::EnvValueRepo::new(state.inner())
        .remove(&config_id, &env_id)
        .map_err(|e| e.to_string())
}
