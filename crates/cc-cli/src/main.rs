mod commands;

use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "cascade", about = "Cascade — config center CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    List {
        #[arg(short, long)]
        group: Option<String>,
    },
    Get {
        key: String,
        /// Decrypt and print the real value of a secret.
        #[arg(long)]
        reveal: bool,
    },
    Set {
        key: String,
        value: String,
        #[arg(short, long)]
        secret: bool,
        /// Set the value for one environment only (config × env layer).
        #[arg(long)]
        env: Option<String>,
    },
    Unset {
        key: String,
        /// Remove only the value for this environment.
        #[arg(long)]
        env: Option<String>,
    },
    Explain {
        key: String,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(short, long)]
        env: Option<String>,
        #[arg(long)]
        reveal: bool,
    },
    Project {
        #[command(subcommand)]
        command: commands::ProjectCommands,
    },
    Env {
        #[command(subcommand)]
        command: commands::EnvCommands,
    },
    Export {
        project: String,
        #[arg(short, long)]
        output: Option<String>,
        #[arg(short, long, default_value = "yaml")]
        format: String,
        #[arg(short, long)]
        env: Option<String>,
        #[arg(short, long)]
        watch: bool,
        #[arg(long)]
        reveal: bool,
        /// Poll interval in seconds for --watch.
        #[arg(long, default_value = "2")]
        interval: u64,
    },
    Run {
        project: String,
        #[arg(trailing_var_arg = true)]
        cmd: Vec<String>,
        #[arg(short, long)]
        env: Option<String>,
    },
    History {
        key: String,
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },
    Revert {
        history_id: i64,
    },
    Import {
        files: Vec<String>,
        #[arg(short, long)]
        group: Option<String>,
        #[arg(long)]
        dry_run: bool,
        /// Overwrite existing keys instead of skipping them.
        #[arg(long)]
        overwrite: bool,
    },
    Render {
        #[arg(short, long)]
        target: Option<String>,
        #[arg(short, long)]
        project: String,
        #[arg(short, long)]
        env: Option<String>,
        /// Overwrite conflicting user-owned keys instead of skipping them.
        #[arg(long)]
        force: bool,
    },
    Models {
        #[command(subcommand)]
        command: commands::ModelsCommands,
    },
    Secrets {
        #[command(subcommand)]
        command: commands::SecretsCommands,
    },
    Serve {
        #[arg(short, long, default_value = "7070")]
        port: u16,
        #[arg(long)]
        open: bool,
        #[arg(long, default_value = "127.0.0.1")]
        listen: String,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        readonly: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::execute(),
        Commands::List { group } => commands::list::execute(group),
        Commands::Get { key, reveal } => commands::get::execute(&key, reveal),
        Commands::Set { key, value, secret, env } => commands::set::execute(&key, &value, secret, env.as_deref()),
        Commands::Unset { key, env } => commands::unset::execute(&key, env.as_deref()),
        Commands::Explain { key, project, env, reveal } => {
            commands::explain::execute(&key, project.as_deref(), env.as_deref(), reveal)
        }
        Commands::Project { command } => commands::project::execute(command),
        Commands::Env { command } => commands::env::execute(command),
        Commands::Export { project, output, format, env, watch, reveal, interval } => {
            commands::export::execute(&project, output.as_deref(), &format, env.as_deref(), watch, reveal, interval).await
        }
        Commands::Run { project, cmd, env } => commands::run::execute(&project, &cmd, env.as_deref()).await,
        Commands::History { key, limit } => commands::history::execute(&key, limit),
        Commands::Revert { history_id } => commands::revert::execute(history_id),
        Commands::Import { files, group, dry_run, overwrite } => {
            commands::import::execute(&files, group.as_deref(), dry_run, overwrite)
        }
        Commands::Render { target, project, env, force } => {
            commands::render::execute(target.as_deref(), &project, env.as_deref(), force)
        }
        Commands::Models { command } => commands::models::execute(command),
        Commands::Secrets { command } => commands::secrets::execute(command),
        Commands::Serve { port, open, listen, token, readonly } => commands::serve::execute(port, open, &listen, token.as_deref(), readonly).await,
    }
}
