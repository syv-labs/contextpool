mod cli;
mod cloud;
mod credentials;
mod embedded_agent;
mod export;
mod init;
mod install_cmd;
mod mcp;
mod paths;
mod project;
mod redact;
mod summarize;
mod team;
mod transcript;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ExportSource, InitSource, McpArgs};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.reset_nvidia_api_key {
        credentials::reset_nvidia_api_key()?;
    }

    match cli.command {
        Command::Init(args) => match args.source {
            InitSource::Cursor(args) => init::init_cursor(args).await,
            InitSource::ClaudeCode(args) => init::init_claude_code(args).await,
            InitSource::Kiro(args) => init::init_kiro(args).await,
            InitSource::Codex(args) => init::init_codex(args).await,
        },
        Command::Export(args) => match args.source {
            ExportSource::Cursor(args) => export::cursor::export_cursor(args).await,
            ExportSource::ClaudeCode(args) => export::claude_code::export_claude_code(args).await,
            ExportSource::Vscdb(args) => export::vscdb::export_vscdb(args).await,
            ExportSource::Kiro(args) => export::kiro::export_kiro(args).await,
            ExportSource::Codex(args) => export::codex::export_codex(args).await,
        },
        Command::Mcp(McpArgs { data_dir }) => mcp::run_server(data_dir).await,
        Command::Auth(args) => team::cmd_auth(args).await,
        Command::Push(args) => team::cmd_push(args).await,
        Command::Pull(args) => team::cmd_pull(args).await,
        Command::Team(args) => team::cmd_team(args).await,
        Command::Install(args) => install_cmd::cmd_install(args).map_err(Into::into),
    }
}
