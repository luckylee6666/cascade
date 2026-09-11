use anyhow::Result;
use std::process::Command;

pub async fn execute(project: &str, cmd: &[String], env_name: Option<&str>) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    // Include only configs attached to the project; env chain + overrides applied.
    let scope = cc_store::resolve::resolve_project_scope(&store, project, env_name)?;

    // Flatten to env vars (decrypt secrets: run is a trusted channel)
    let master_key = cc_core::crypto::get_or_create_master_key()?;
    let pairs: Vec<(String, String)> = scope.configs.iter()
        .map(|c| {
            let value = if cc_core::crypto::is_encrypted(&c.effective_value) {
                cc_core::crypto::decrypt(&c.effective_value, &master_key)?
            } else {
                c.effective_value.clone()
            };
            Ok((c.config.key.clone(), value))
        })
        .collect::<Result<_>>()?;
    let env_vars = cc_core::config::flatten::flatten_to_env(&pairs);

    // Build command
    if cmd.is_empty() {
        anyhow::bail!("No command specified");
    }

    let mut command = Command::new(&cmd[0]);
    if cmd.len() > 1 {
        command.args(&cmd[1..]);
    }

    // Inject env vars
    for (key, value) in &env_vars {
        command.env(key, value);
    }

    // Run and propagate exit code
    let status = command.status()?;
    std::process::exit(status.code().unwrap_or(1));
}


