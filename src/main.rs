mod cli;
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

    match cli.command {
        Command::Init(args) => match args.source {
            InitSource::Cursor(args) => init::init_cursor(args).await,
        },
        Command::Export(args) => match args.source {
            ExportSource::Cursor(args) => export::cursor::export_cursor(args).await,
            ExportSource::Vscdb(args) => export::vscdb::export_vscdb(args).await,
            ExportSource::Kiro(args) => export::kiro::export_kiro(args).await,
        },
    }
}
