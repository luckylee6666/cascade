use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub id: String,
    pub key: String,
    pub value: Option<String>,
    pub secret: bool,
    pub encrypted: Option<String>,
    pub group: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigWithValue {
    pub config: Config,
    pub effective_value: String,
    pub source: ConfigSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigSource {
    Base,
    Environment(String),
    ProjectOverride(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigCreate {
    pub key: String,
    pub value: Option<String>,
    pub secret: bool,
    pub group: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdate {
    pub value: Option<String>,
    pub secret: Option<bool>,
    pub group: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCreate {
    pub name: String,
    pub parent_id: Option<String>,
}

/// Per-environment value for one config (config × env layer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEnvValue {
    pub config_id: String,
    pub env_id: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCreate {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_id: String,
    pub config_id: String,
    pub env_id: String,
    pub override_value: Option<String>,
    pub override_encrypted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfigCreate {
    pub config_id: String,
    pub env_id: String,
    pub override_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigHistory {
    pub id: i64,
    pub config_id: String,
    pub key: String,
    pub value: Option<String>,
    pub encrypted: Option<String>,
    pub action: HistoryAction,
    pub operator: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HistoryAction {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub project: Project,
    pub env: Environment,
    pub configs: Vec<ConfigWithValue>,
}
