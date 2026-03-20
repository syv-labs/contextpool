mod cli;
mod credentials;
mod embedded_agent;
mod export;
mod init;
mod paths;
mod project;
mod summarize;
mod transcript;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ExportSource, InitSource};

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
        },
        Command::Export(args) => match args.source {
            ExportSource::Cursor(args) => export::cursor::export_cursor(args).await,
            ExportSource::ClaudeCode(args) => export::claude_code::export_claude_code(args).await,
            ExportSource::Vscdb(args) => export::vscdb::export_vscdb(args).await,
            ExportSource::Kiro(args) => export::kiro::export_kiro(args).await,
        },
    }
}
