# ContextPool (`cxp`)

A per-project memory store that extracts engineering insights from your local IDE/agent chat transcripts (Cursor, Claude Code, Kiro, VS Code forks) and exposes them via an MCP server. Works inside Claude Code and Cursor with no API key required.

## Install

No Rust required. One command installs the `cxp` binary:

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/syv-labs/cxp/main/install.ps1 | iex
```

To pin a version, set `CONTEXTPOOL_VERSION=0.1.0` (shell) or `$env:CONTEXTPOOL_VERSION="0.1.0"` (PowerShell) before running.

**Build from source:**
```bash
cargo build --release
# binary at target/release/cxp
```

---

## MCP Setup (Claude Code)

**1. Add to `~/.claude/settings.json`:**
```json
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp"]
    }
  }
}
```

**2. That's it.** No API key needed — `cxp` uses the `claude` CLI that ships with Claude Code.

Claude will now have access to four tools:

| Tool | Description |
|------|-------------|
| `fetch_project_context` | Scans Cursor and Claude Code transcripts for the current project, summarizes new ones, returns a compact index |
| `get_project_context` | Loads full markdown content of selected summaries into context |
| `search_context` | Full-text search across all summaries for a keyword or phrase |
| `list_context_projects` | Lists all projects that have stored context with summary counts |

**Typical workflow:** Claude calls `fetch_project_context` → picks relevant ids → calls `get_project_context` to load them.

Summaries are stored locally in `<project>/ContextPool/` alongside your code.

---

## MCP Setup (Cursor)

**1. Add to Cursor's MCP config (`~/.cursor/mcp.json` or via Settings → MCP):**
```json
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp"]
    }
  }
}
```

**2. API key:** Cursor does not expose its own auth to subprocesses. Provide one of:
- set `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, or `NVIDIA_API_KEY` in the MCP server env

---

## Authentication

`cxp` tries backends in this order and uses the first one available:

| Priority | Backend | How to enable |
|----------|---------|---------------|
| 1 | `claude` CLI | Install Claude Code or `npm i -g @anthropic-ai/claude-code`. No key needed. |
| 2 | Anthropic API | Set `ANTHROPIC_API_KEY` |
| 3 | OpenAI API | Set `OPENAI_API_KEY` |
| 4 | NVIDIA NIM | Set `NVIDIA_API_KEY`, or run any `cxp` command interactively to be prompted (key is saved to system keychain) |

To reset a saved NVIDIA key:
```bash
cxp --reset-nvidia-api-key
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | — | Anthropic API key (backend priority 2) |
| `OPENAI_API_KEY` | — | OpenAI API key (backend priority 3) |
| `NVIDIA_API_KEY` | — | NVIDIA NIM API key (backend priority 4) |
| `MODEL` | per-backend default¹ | Override the model used for summarization |
| `REPAIR_MODEL` | same as `MODEL` | Model used for the JSON repair pass if the first response is malformed |
| `TEMPERATURE` | `0.0` | Sampling temperature |
| `TOP_P` | `0.95` | Top-p sampling |
| `MAX_COMPLETION_TOKENS` | `4096` | Max tokens in the LLM response |
| `SANITIZE_CHAT` | `1` | Strip `<think>` blocks, tool calls, and markdown fences before sending to LLM (`0` to disable) |
| `EXTRACT_USER_QUERIES_ONLY` | `0` | Send only user messages to the LLM, not assistant replies (`1` to enable) |
| `DEBUG_LLM_OUTPUT` | `0` | Print raw LLM output to stderr before parsing (`1` to enable) |

¹ Default models by backend:
- Claude Code CLI / Anthropic: `claude-haiku-4-5-20251001`
- OpenAI: `gpt-4o-mini`
- NVIDIA: `qwen/qwen3.5-122b-a10b`

---

## CLI Reference

### `cxp init cursor [chat-ids...] [OPTIONS]`

Initialize memory for the current directory from Cursor transcripts.

```bash
cxp init cursor                          # summarize all Cursor chats for this project
cxp init cursor <id1> <id2>             # summarize specific chat ids
cxp init cursor --local                  # store summaries in ./ContextPool/ instead of app data dir
cxp init cursor --out ./store           # custom output directory
cxp init cursor --cursor-dir ~/.cursor2  # custom Cursor root
```

### `cxp init claude-code [session-ids...] [OPTIONS]`

Initialize memory for the current directory from Claude Code sessions.

```bash
cxp init claude-code                     # summarize all sessions for this project
cxp init claude-code <id1> <id2>        # summarize specific session ids
cxp init claude-code --local             # store summaries in ./ContextPool/
cxp init claude-code --out ./store      # custom output directory
cxp init claude-code --claude-dir ~/.claude2
```

### `cxp export cursor [OPTIONS]`

Bulk export all Cursor transcripts from `~/.cursor`.

```bash
cxp export cursor                        # scan and summarize all Cursor transcripts
cxp export cursor --offline              # store placeholder summaries (no LLM call)
cxp export cursor --transcript <path>    # export a single .jsonl file
cxp export cursor --out ./out            # custom output directory
cxp export cursor --cursor-dir <path>    # custom Cursor root
```

### `cxp export claude-code [OPTIONS]`

Bulk export all Claude Code sessions from `~/.claude/projects`.

```bash
cxp export claude-code
cxp export claude-code --offline
cxp export claude-code --session <path>  # export a single .jsonl session file
cxp export claude-code --out ./out
cxp export claude-code --claude-dir <path>
```

### `cxp export vscdb [OPTIONS]`

Export chat history from VS Code-style `state.vscdb` workspace storage (Cursor, Windsurf, and other forks).

```bash
cxp export vscdb --offline --product Cursor
cxp export vscdb --offline --product Windsurf --workspace-storage "<path>"
```

Default workspace storage paths:
- macOS (Cursor): `~/Library/Application Support/Cursor/User/workspaceStorage`
- Windows (Cursor): `%APPDATA%\Cursor\User\workspaceStorage`

### `cxp export kiro [OPTIONS]`

Export a Kiro session saved via `/chat save <path>`.

```bash
cxp export kiro --chat-json ./kiro-session.json
cxp export kiro --offline --chat-json ./kiro-session.json
```

### `cxp mcp [OPTIONS]`

Start the MCP server (used by Claude Code / Cursor config, not usually called directly).

```bash
cxp mcp
cxp mcp --data-dir ./ContextPool   # custom data directory
```

---

## Data Storage

Summaries are stored in `<project>/ContextPool/` by default (next to your code). The `--local` and `--out` flags on `init` commands, and `--out` on `export` commands, let you override this.

The MCP server's `fetch_project_context` tool always writes to `<project>/ContextPool/fetched/<timestamp>/`.
