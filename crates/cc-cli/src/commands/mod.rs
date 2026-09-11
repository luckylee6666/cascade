pub mod init;
pub mod list;
pub mod get;
pub mod set;
pub mod unset;
pub mod explain;
pub mod project;
pub mod env;
pub mod export;
pub mod run;
pub mod history;
pub mod revert;
pub mod import;
pub mod render;
pub mod models;
pub mod secrets;
pub mod serve;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ProjectCommands {
    Create {
        name: String,
        #[arg(short, long)]
        description: Option<String>,
    },
    List,
    Delete {
        id: String,
    },
    AddConfig {
        project_id: String,
        config_id: String,
        env_id: String,
        #[arg(short, long)]
        value: Option<String>,
    },
    RemoveConfig {
        project_id: String,
        config_id: String,
        env_id: String,
    },
    Configs {
        project_id: String,
    },
}

#[derive(Subcommand)]
pub enum EnvCommands {
    Create {
        name: String,
        #[arg(short, long)]
        parent: Option<String>,
    },
    List,
    Delete {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum ModelsCommands {
    Sync {
        /// Base URL of an OpenAI-compatible API, e.g. https://api.openai.com/v1
        #[arg(long)]
        url: Option<String>,
        /// Bearer key. Falls back to $CC_MODELS_KEY.
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum SecretsCommands {
    Init,
    Encrypt,
    Ls,
    Squash {
        #[arg(long)]
        yes: bool,
    },
}
