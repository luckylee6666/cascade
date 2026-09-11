use anyhow::Result;
use std::collections::HashSet;

use crate::config_repo::ConfigRepo;
use crate::db::Store;
use crate::env_repo::EnvRepo;
use crate::env_value_repo::EnvValueRepo;
use crate::project_repo::ProjectRepo;
use cc_core::config::inherit::resolve_inheritance;
use cc_core::config::{ConfigWithValue, Environment, Project};

pub struct ResolvedScope {
    pub project: Project,
    pub env: Environment,
    pub configs: Vec<ConfigWithValue>,
}

/// Resolve what a project actually consumes for one environment.
///
/// Inclusion rule: a config is part of the scope **only if it is attached to
/// the project** (a row in `project_configs`). Unattached pool entries never
/// appear in exports, process injection or SDK output. Attachments are also
/// where per-project overrides live.
///
/// Value layer order: base value → env-chain values → project override
/// (leaf env only).
pub fn resolve_project_scope(
    store: &Store,
    project: &str,     // name or id
    env: Option<&str>, // name or id; defaults to "base", then the first env
) -> Result<ResolvedScope> {
    let project_repo = ProjectRepo::new(store);
    let config_repo = ConfigRepo::new(store);
    let env_repo = EnvRepo::new(store);
    let env_value_repo = EnvValueRepo::new(store);

    let proj = project_repo
        .list()?
        .into_iter()
        .find(|p| p.name == project || p.id == project)
        .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", project))?;

    let envs = env_repo.list()?;
    let env = match env {
        Some(name) => envs
            .iter()
            .find(|e| e.name == name || e.id == name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Environment '{}' not found", name))?,
        None => envs
            .iter()
            .find(|e| e.name == "base")
            .or_else(|| envs.first())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No environments found"))?,
    };

    let attachments = project_repo.list_configs(&proj.id)?;
    let attached: HashSet<&str> = attachments.iter().map(|a| a.config_id.as_str()).collect();
    let configs: Vec<_> = config_repo
        .list(None)?
        .into_iter()
        .filter(|c| attached.contains(c.id.as_str()))
        .collect();

    let chain = env_repo.get_chain(&env.id)?;
    let resolved = resolve_inheritance(&configs, &chain, &env_value_repo.list()?, &attachments);

    Ok(ResolvedScope {
        project: proj,
        env,
        configs: resolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_repo::ConfigRepo;
    use crate::env_repo::EnvRepo;
    use cc_core::config::{ConfigCreate, EnvironmentCreate, ProjectCreate, ProjectConfigCreate};

    #[test]
    fn test_scope_is_attachments_only() {
        let store = Store::open_memory().unwrap();
        let configs = ConfigRepo::new(&store);
        let envs = EnvRepo::new(&store);
        let projects = ProjectRepo::new(&store);

        let attached = configs
            .create(ConfigCreate {
                key: "db.host".into(),
                value: Some("localhost".into()),
                secret: false,
                group: None,
                description: None,
            })
            .unwrap();
        let unattached = configs
            .create(ConfigCreate {
                key: "redis.host".into(),
                value: Some("localhost".into()),
                secret: false,
                group: None,
                description: None,
            })
            .unwrap();
        let env = envs
            .create(EnvironmentCreate {
                name: "base".into(),
                parent_id: None,
            })
            .unwrap();
        let proj = projects
            .create(ProjectCreate {
                name: "app".into(),
                description: None,
            })
            .unwrap();
        projects
            .add_config(
                &proj.id,
                ProjectConfigCreate {
                    config_id: attached.id.clone(),
                    env_id: env.id.clone(),
                    override_value: Some("attached.host".into()),
                },
            )
            .unwrap();

        // Unattached config gets an env value — it still must not leak in.
        EnvValueRepo::new(&store)
            .set(&unattached.id, &env.id, "redis.internal")
            .unwrap();

        let scope = resolve_project_scope(&store, "app", None).unwrap();
        assert_eq!(scope.configs.len(), 1);
        assert_eq!(scope.configs[0].config.key, "db.host");
        assert_eq!(scope.configs[0].effective_value, "attached.host");
        assert_eq!(scope.env.name, "base");
    }

    #[test]
    fn test_scope_env_chain_applies_to_attached() {
        let store = Store::open_memory().unwrap();
        let configs = ConfigRepo::new(&store);
        let envs = EnvRepo::new(&store);
        let projects = ProjectRepo::new(&store);

        let c = configs
            .create(ConfigCreate {
                key: "db.host".into(),
                value: Some("localhost".into()),
                secret: false,
                group: None,
                description: None,
            })
            .unwrap();
        let base = envs
            .create(EnvironmentCreate {
                name: "base".into(),
                parent_id: None,
            })
            .unwrap();
        let prod = envs
            .create(EnvironmentCreate {
                name: "prod".into(),
                parent_id: Some(base.id.clone()),
            })
            .unwrap();
        let proj = projects
            .create(ProjectCreate {
                name: "app".into(),
                description: None,
            })
            .unwrap();
        projects
            .add_config(
                &proj.id,
                ProjectConfigCreate {
                    config_id: c.id.clone(),
                    env_id: base.id.clone(),
                    override_value: None,
                },
            )
            .unwrap();
        EnvValueRepo::new(&store)
            .set(&c.id, &prod.id, "prod.internal")
            .unwrap();

        let scope = resolve_project_scope(&store, "app", Some("prod")).unwrap();
        assert_eq!(scope.configs[0].effective_value, "prod.internal");
    }
}
