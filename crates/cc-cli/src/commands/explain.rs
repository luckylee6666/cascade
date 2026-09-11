use anyhow::Result;
use colored::Colorize;

/// Show every layer that contributes to a key and the effective value.
pub fn execute(key: &str, project: Option<&str>, env_name: Option<&str>, reveal: bool) -> Result<()> {
    let store = cc_store::db::Store::open_default()?;

    use cc_store::config_repo::ConfigRepo;
    use cc_store::env_repo::EnvRepo;
    use cc_store::env_value_repo::EnvValueRepo;
    use cc_store::project_repo::ProjectRepo;

    let config_repo = ConfigRepo::new(&store);
    let env_repo = EnvRepo::new(&store);
    let env_value_repo = EnvValueRepo::new(&store);
    let project_repo = ProjectRepo::new(&store);

    let config = config_repo
        .get_by_key(key)?
        .ok_or_else(|| anyhow::anyhow!("Key '{}' not found", key))?;

    let display = |v: &str| -> Result<String> {
        if config.secret && cc_core::crypto::is_encrypted(v) && reveal {
            let master_key = cc_core::crypto::get_or_create_master_key()?;
            cc_core::crypto::decrypt(v, &master_key)
        } else if config.secret {
            Ok("••••••••".to_string())
        } else {
            Ok(v.to_string())
        }
    };

    println!("{}", config.key.bold());

    let base = config.value.clone().unwrap_or_default();
    println!("  {:<10} {}", "base".dimmed(), display(&base)?);

    let envs = env_repo.list()?;
    let env_values = env_value_repo.list()?;
    for ev in env_values.iter().filter(|v| v.config_id == config.id) {
        let env_label = envs
            .iter()
            .find(|e| e.id == ev.env_id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| ev.env_id.clone());
        let v = ev.value.clone().unwrap_or_default();
        println!("  {:<10} {}", env_label, display(&v)?);
    }

    let overrides = if let Some(p) = project {
        let proj = project_repo
            .list()?
            .into_iter()
            .find(|x| x.name == p || x.id == p)
            .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", p))?;
        let items = project_repo.list_configs(&proj.id)?;
        let is_attached = items.iter().any(|o| o.config_id == config.id);
        if !is_attached {
            println!(
                "  {} not attached to project '{}' — its exports won't include this key",
                "!".yellow(),
                p
            );
        }
        items
            .into_iter()
            .filter(|o| o.config_id == config.id && o.override_value.is_some())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    for o in &overrides {
        let env_label = envs
            .iter()
            .find(|e| e.id == o.env_id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| o.env_id.clone());
        let v = o.override_value.clone().unwrap_or_default();
        println!(
            "  {:<10} {} {}",
            "project".dimmed(),
            format!("({})", env_label).dimmed(),
            display(&v)?
        );
    }

    // Effective value for the requested scope.
    let env_chain = if let Some(name) = env_name {
        let env = envs
            .iter()
            .find(|e| e.name == name || e.id == name)
            .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", name))?;
        env_repo.get_chain(&env.id)?
    } else {
        vec![]
    };
    let resolved = cc_core::config::inherit::resolve_inheritance(
        std::slice::from_ref(&config),
        &env_chain,
        &env_values,
        &overrides,
    );
    let r = &resolved[0];
    let source_label = match &r.source {
        cc_core::config::ConfigSource::Base => "base".to_string(),
        cc_core::config::ConfigSource::Environment(name) => format!("env {}", name),
        cc_core::config::ConfigSource::ProjectOverride(name) => format!("project override @ {}", name),
    };
    println!(
        "  {} {} {}",
        "=".green(),
        display(&r.effective_value)?,
        format!("[{}]", source_label).dimmed()
    );

    Ok(())
}
