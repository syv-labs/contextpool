use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cxp", version, about = "ContextPool: shared memory pool for local IDE/agent chats")]
pub struct Cli {
    /// Delete the stored NVIDIA API key from the system keychain (forces re-prompt).
    #[arg(long)]
    pub reset_nvidia_api_key: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize memory for the current directory (project)
    Init(InitArgs),

    /// Export chats from Cursor (scans ~/.cursor)
    Export(ExportArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {
    #[command(subcommand)]
    pub source: InitSource,
}

#[derive(Subcommand, Debug, Clone)]
pub enum InitSource {
    /// Initialize memory from Cursor for the current directory's project id
    Cursor(InitCursorArgs),

    /// Initialize memory from Claude Code for the current directory's project id
    ClaudeCode(InitClaudeCodeArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct InitClaudeCodeArgs {
    /// Optional centralized storage directory (defaults to OS local app data dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Store initialized summaries inside the current directory (`./ContextPool/...`)
    #[arg(long, conflicts_with = "out")]
    pub local: bool,

    /// Claude Code root directory (defaults to ~/.claude)
    #[arg(long)]
    pub claude_dir: Option<PathBuf>,

    /// Space-separated Claude Code session ids (typically session file names without `.jsonl`)
    ///
    /// Example: `cxp init claude-code 7b1e... 1a2b...`
    #[arg(required = false)]
    pub session_ids: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct InitCursorArgs {
    /// Optional centralized storage directory (defaults to OS local app data dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Store initialized summaries inside the current directory (`./ContextPool/...`)
    #[arg(long, conflicts_with = "out")]
    pub local: bool,

    /// Cursor root directory (defaults to ~/.cursor)
    #[arg(long)]
    pub cursor_dir: Option<PathBuf>,

    /// Space-separated Cursor chat ids (typically transcript file names without `.jsonl`)
    ///
    /// Example: `cxp init cursor 7b1e... 1a2b...`
    #[arg(required = false)]
    pub chat_ids: Vec<String>,
}

#[derive(Parser, Debug)]
pub struct ExportArgs {
    #[command(subcommand)]
    pub source: ExportSource,
}

#[derive(Subcommand, Debug)]
pub enum ExportSource {
    /// Export Cursor agent transcripts (*.jsonl) and store summaries locally
    Cursor(ExportCursorArgs),

    /// Export Claude Code session files (*.jsonl) from ~/.claude/projects
    ClaudeCode(ExportClaudeCodeArgs),

    /// Export chat history from VS Code-style workspace storage (state.vscdb)
    Vscdb(ExportVscdbArgs),

    /// Export chats from a Kiro `/chat save` JSON file
    Kiro(ExportKiroArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ExportClaudeCodeArgs {
    /// Optional output directory (defaults to app data dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Claude Code root directory (defaults to ~/.claude)
    #[arg(long)]
    pub claude_dir: Option<PathBuf>,

    /// Export a single Claude Code session file (.jsonl) instead of scanning all projects
    #[arg(long)]
    pub session: Option<PathBuf>,

    /// Do not call remote API; store a local fallback summary
    #[arg(long)]
    pub offline: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct ExportCursorArgs {
    /// Optional output directory (defaults to app data dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Cursor root directory (defaults to ~/.cursor)
    #[arg(long)]
    pub cursor_dir: Option<PathBuf>,

    /// Export a single Cursor transcript file (.jsonl) instead of scanning directories
    #[arg(long)]
    pub transcript: Option<PathBuf>,

    /// Do not call remote API; store a local fallback summary
    #[arg(long)]
    pub offline: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct ExportVscdbArgs {
    /// Optional output directory (defaults to app data dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Which editor product directory name to use for defaults (Cursor, Code, Windsurf, etc.)
    ///
    /// If you set `--workspace-storage`, this is only used for labeling.
    #[arg(long, default_value = "Cursor")]
    pub product: String,

    /// Path to a VS Code-style workspaceStorage directory.
    ///
    /// Examples:
    /// - Windows: %APPDATA%\\Cursor\\User\\workspaceStorage
    /// - macOS:   ~/Library/Application Support/Cursor/User/workspaceStorage
    #[arg(long)]
    pub workspace_storage: Option<PathBuf>,

    /// Do not call remote API; store a local fallback summary
    #[arg(long)]
    pub offline: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct ExportKiroArgs {
    /// Optional output directory (defaults to app data dir)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Path to a Kiro exported chat JSON file (from `/chat save <path>`).
    #[arg(long)]
    pub chat_json: PathBuf,

    /// Do not call remote API; store a local fallback summary
    #[arg(long)]
    pub offline: bool,
}

