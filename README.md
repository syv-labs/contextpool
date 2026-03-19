# ContextPool (CLI)

Create a centralized, per-project “memory” store from locally stored IDE/agent chats (starting with Cursor).

## Build

```bash
cargo build --release
```

Binary will be at `target/release/cxp`.

## Install (cross-platform binaries)

This repo ships prebuilt binaries via GitHub Releases. End users do **not** need Rust installed.

- **macOS / Linux** (shell):

```bash
export CONTEXTPOOL_REPO="OWNER/REPO"
curl -fsSL "https://raw.githubusercontent.com/$CONTEXTPOOL_REPO/main/install.sh" | sh
```

- **Windows** (PowerShell):

```powershell
$env:CONTEXTPOOL_REPO="OWNER/REPO"
irm "https://raw.githubusercontent.com/$env:CONTEXTPOOL_REPO/main/install.ps1" | iex
```

To install a specific version:
- `CONTEXTPOOL_VERSION=0.1.0` (shell) / `$env:CONTEXTPOOL_VERSION="0.1.0"` (PowerShell)

Release automation:
- Pushing a git tag like `v0.1.0` builds and uploads `tar.gz`/`zip` assets plus `checksums.txt`.

## Initialize memory for current directory (Cursor-only flow)

Run this from inside your project directory.

- If you provide **Cursor chat ids** (typically the transcript filename without `.jsonl`), it summarizes only those.
- If you provide **no chat ids**, it summarizes **all** chats for the current project.

```bash
./target/release/cxp init cursor <chat-id> <chat-id> ...
```

Summarize everything for the project:

```bash
./target/release/cxp init cursor
```

This creates a centralized store under your OS local app data dir:
- macOS: `~/Library/Application Support/ContextPool/projects/<project-id>/`

Then it summarizes only those Cursor transcripts for that project id into:
- `.../imports/cursor/<timestamp>/`

You can override the centralized store location:

```bash
./target/release/cxp init cursor <chat-id> --out ./contextpool-store
```

## Export Cursor chats (debug / bulk export)

Scans common Cursor transcript locations under `~/.cursor`:
- `~/.cursor/agent-transcripts/**/*.jsonl`
- `~/.cursor/projects/**/agent-transcripts/**/*.jsonl`

```bash
./target/release/cxp export cursor --offline
```

Note: In offline mode, the CLI stores only a short placeholder summary (it does **not** persist raw transcript contents).

Export a **single Cursor transcript file** (useful when you already know the path, on macOS or Windows):

```bash
./target/release/cxp export cursor --offline --transcript "/path/to/<session>.jsonl"
```

By default, exports are written under your OS local app data dir:
- macOS: `~/Library/Application Support/ContextPool/exports/<timestamp>/`

Override output directory:

```bash
./target/release/cxp export cursor --offline --out ./out
```

## Export Cursor chats (using your API)

This CLI will call your running `context-generator-agent`:
- `POST <API_BASE>/generate-context`
- JSON body: `{ "chat": "<extracted transcript text>", "files": [], "repo_type": "" }`
- Expected JSON response: a JSON array of 0–5 context items

```bash
export CONTEXT_POOL_API_BASE="https://your-service.example"
./target/release/cxp export cursor
```

If the API call fails, the CLI falls back to an offline summary.

## Export VS Code-style chat history (`state.vscdb`)

Many VS Code-based editors (Cursor and similar forks) store chat state in SQLite DBs named `state.vscdb` under a `workspaceStorage` directory.

This command scans `workspaceStorage/**/state.vscdb`, extracts a few known AI-chat keys, and summarizes what it finds:

```bash
./target/release/cxp export vscdb --offline --product Cursor
```

Override the storage location explicitly (recommended if your editor uses a different product name):

- **Windows (Cursor)**: `%APPDATA%\Cursor\User\workspaceStorage`
- **macOS (Cursor)**: `~/Library/Application Support/Cursor/User/workspaceStorage`

```bash
./target/release/cxp export vscdb --offline --product Cursor --workspace-storage "C:\\Users\\<you>\\AppData\\Roaming\\Cursor\\User\\workspaceStorage"
```

For **Windsurf** or other VS Code forks, try:

```bash
./target/release/cxp export vscdb --offline --product Windsurf --workspace-storage "<their workspaceStorage path>"
```

## Export Kiro chats

Kiro can export the current session to a JSON file via `/chat save <path>` (or the equivalent in their CLI). Once you have that JSON file:

```bash
./target/release/cxp export kiro --offline --chat-json ./kiro-session.json
```

