mod commands;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let store_path = cc_store::db::Store::vault_path()?;
            if let Some(parent) = store_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let store = cc_store::db::Store::open(&store_path)?;

            app.manage(store);
            app.manage(commands::system::ShareState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::list_configs,
            commands::config::get_config,
            commands::config::create_config,
            commands::config::update_config,
            commands::config::delete_config,
            commands::config::list_config_history,
            commands::config::revert_config,
            commands::project::list_projects,
            commands::project::create_project,
            commands::project::get_project,
            commands::project::delete_project,
            commands::project::list_project_configs,
            commands::project::add_project_config,
            commands::project::remove_project_config,
            commands::project::export_project,
            commands::env::list_envs,
            commands::env::create_env,
            commands::env::get_env,
            commands::env::delete_env,
            commands::env::list_env_values,
            commands::env::set_env_value,
            commands::env::delete_env_value,
            commands::crypto::encrypt_value,
            commands::crypto::decrypt_value,
            commands::system::vault_path,
            commands::system::toolchain_paths,
            commands::system::share_status,
            commands::system::share_start,
            commands::system::share_stop,
            commands::import::pick_import_files,
            commands::import::preview_import,
            commands::import::run_import,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let share = app_handle.try_state::<commands::system::ShareState>();
                let store = app_handle.try_state::<cc_store::db::Store>();
                if let Some(share) = share {
                    commands::system::kill_share_with_store(
                        &share,
                        store.as_deref(),
                    );
                }
            }
        });
}
