# ContextPool (`cxp`)

> **Persistent memory for AI coding agents — across sessions, across your team.**

Your AI agent forgets everything between sessions. You re-explain the same architecture. You debug the same bug twice. You lose the fix that took an hour to find.

**ContextPool fixes that.** It reads your Cursor and Claude Code sessions, extracts what actually matters — bugs found, root causes, design decisions, gotchas — and feeds them back to your agent automatically. Every session starts smarter than the last.

```
$ cxp init claude-code --local

  Found 14 Claude Code session(s) for this project.
  Summarized 14 session(s) -> 47 insight(s) extracted.

  Top insights:
    bug      ESM import fails silently in test runner — add "type": "module" to package.json
    decision chose SQLite over Postgres for local-only storage
    gotcha   reqwest needs rustls-tls feature, not default openssl

  Your agent will now recall these automatically via MCP.
```

**Zero config in Claude Code.** No API key needed — it piggybacks on your existing auth.

---

## Why ContextPool?

| Without cxp | With cxp |
|---|---|
| Agent re-discovers bugs you already fixed | Agent recalls the fix instantly |
| You re-explain architecture every session | Agent loads decisions from memory |
| New teammates start from zero | Team shares a living knowledge base |
| Insights locked in one IDE | Works across Cursor, Claude Code, Windsurf, Kiro |

**Your chat history is a goldmine. cxp turns it into structured memory.**

---

## Install

No Rust required. One command:

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/syv-labs/cxp/main/install.ps1 | iex
```

To pin a version, set `CONTEXTPOOL_VERSION=0.1.0` before running.

**Build from source:**
```bash
cargo build --release
# binary at target/release/cxp
```

---

## Quickstart

```bash
# 1. Extract insights from your sessions
cxp init claude-code --local   # or: cxp init cursor --local

# 2. Add the MCP server to Claude Code
# ~/.claude/settings.json:
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp"]
    }
  }
}

# 3. Done. Your agent now has memory.
```

---

## MCP Setup

### Claude Code

Add to `~/.claude/settings.json`:
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

No API key needed — `cxp` uses the `claude` CLI that ships with Claude Code.

### Cursor

Add to `~/.cursor/mcp.json` (or **Settings → MCP**):
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

Cursor doesn't expose its own auth to subprocesses. Set one of: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, or `NVIDIA_API_KEY` in the MCP server env.

### What the agent can do

Once connected, your agent has four tools:

| Tool | What it does |
|------|-------------|
| `fetch_project_context` | Scans transcripts for this project, summarizes new ones, returns an index |
| `get_project_context` | Loads full insight content into context by id |
| `search_context` | Full-text search across all summaries |
| `list_context_projects` | Lists all projects with stored context |

The agent follows smart rules automatically:
- **First conversation** → auto-fetches context so it's never starting blind
- **"Remember when we..."** → searches first, only re-fetches if nothing found
- **Debugging** → searches by error message or component name before guessing

---

## Team Sync (Cloud)

Share insights across your whole team. Every engineer's discoveries become team knowledge.

**Get started at [contextpool.io](https://contextpool.io) → Sign in with GitHub → Copy your team key.**

```bash
# Authenticate
cxp auth <your-team-key>

# Push your local insights to the team pool
cxp push

# Pull your teammates' insights locally
cxp pull

# See team info and who's contributing
cxp team
cxp team projects
```

After `cxp pull`, the agent automatically searches team insights alongside your own. If a teammate already hit the same bug, your agent knows.

**Free tier:** 1,000 insights · 5 projects · 3 members
**Paid:** Unlimited — [contextpool.io](https://contextpool.io)

### Team env vars

| Variable | Default | Description |
|---|---|---|
| `CXP_API_KEY` | — | Team API key (alternative to keychain) |
| `CXP_API_URL` | `https://api.contextpool.dev` | Override for self-hosted or local dev |

---

## Supported IDEs

| IDE | Transcript source | Command |
|---|---|---|
| Claude Code | `~/.claude/projects/` JSONL | `cxp init claude-code` |
| Cursor | `~/.cursor/` JSONL | `cxp init cursor` |
| Cursor / Windsurf | VS Code `state.vscdb` | `cxp export vscdb` |
| Kiro | `/chat save` JSON export | `cxp export kiro` |

---

## Authentication (LLM Backend)

`cxp` uses an LLM to summarize transcripts. It detects what's available automatically:

| Priority | Backend | How to enable |
|---|---|---|
| 1 | `claude` CLI | Ships with Claude Code — **zero config** |
| 2 | Anthropic API | Set `ANTHROPIC_API_KEY` |
| 3 | OpenAI API | Set `OPENAI_API_KEY` |
| 4 | NVIDIA NIM | Set `NVIDIA_API_KEY` (saved to keychain on first use) |

To reset a saved NVIDIA key:
```bash
cxp --reset-nvidia-api-key
```

---

## CLI Reference

### `cxp init`

Extract and store insights from your IDE sessions.

```bash
cxp init claude-code                     # all sessions for this project
cxp init claude-code <id1> <id2>        # specific session ids
cxp init claude-code --local             # store in ./ContextPool/ (great for git)
cxp init claude-code --out ./store      # custom output directory
cxp init claude-code --claude-dir ~/.claude2

cxp init cursor                          # all Cursor chats for this project
cxp init cursor <id1> <id2>
cxp init cursor --local
cxp init cursor --cursor-dir ~/.cursor2
```

### `cxp export`

Bulk-export transcripts from any supported IDE.

```bash
cxp export claude-code                   # all Claude Code sessions
cxp export claude-code --session <path>  # single .jsonl file
cxp export claude-code --out ./out

cxp export cursor                        # all Cursor transcripts
cxp export cursor --transcript <path>    # single .jsonl file
cxp export cursor --out ./out

cxp export vscdb --product Cursor        # VS Code storage (Cursor, Windsurf, etc.)
cxp export vscdb --product Windsurf --workspace-storage "<path>"

cxp export kiro --chat-json ./session.json
```

### `cxp auth / push / pull / team`

Team sync commands.

```bash
cxp auth <team-key>      # authenticate and save key
cxp push                 # push local insights to the cloud
cxp pull                 # pull team insights locally
cxp team                 # show team info
cxp team projects        # list all team projects
```

### `cxp mcp`

Start the MCP server (called by your IDE config, not usually directly).

```bash
cxp mcp
cxp mcp --data-dir ./ContextPool
```

---

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `ANTHROPIC_API_KEY` | — | Anthropic API key (backend priority 2) |
| `OPENAI_API_KEY` | — | OpenAI API key (backend priority 3) |
| `NVIDIA_API_KEY` | — | NVIDIA NIM key (backend priority 4) |
| `CXP_API_KEY` | — | Team API key (alternative to `cxp auth`) |
| `CXP_API_URL` | `https://api.contextpool.dev` | Override API endpoint |
| `MODEL` | per-backend default | Override the summarization model |
| `REPAIR_MODEL` | same as `MODEL` | Model for JSON repair pass |
| `TEMPERATURE` | `0.0` | Sampling temperature |
| `TOP_P` | `0.95` | Top-p sampling |
| `MAX_COMPLETION_TOKENS` | `4096` | Max tokens per response |
| `SANITIZE_CHAT` | `1` | Strip tool calls, thinking blocks, file snapshots before LLM (`0` to disable) |
| `EXTRACT_USER_QUERIES_ONLY` | `0` | Send only user messages, not assistant replies (`1` to enable) |
| `DEBUG_LLM_OUTPUT` | `0` | Print raw LLM output to stderr (`1` to enable) |

Default models by backend:
- Claude Code CLI / Anthropic: `claude-haiku-4-5-20251001`
- OpenAI: `gpt-4o-mini`
- NVIDIA: `qwen/qwen3.5-122b-a10b`

---

## Data Storage

Summaries are stored in `<project>/ContextPool/` by default, next to your code. Use `--local` or `--out` on `init`/`export` commands to override.

The MCP `fetch_project_context` tool writes to `<project>/ContextPool/fetched/<timestamp>/`.

Team insights pulled via `cxp pull` are cached at `~/.cache/contextpool/team-cache/<project>/team-insights.md`.

**Privacy:** Secrets are automatically redacted before any insight leaves your machine — API keys, tokens, connection strings, and JWTs are stripped before summarization and before cloud sync.

---

## Self-Hosting

Want to run your own sync server? The server source is private but the API contract is simple — any server implementing the endpoints below works with the CLI.

Set `CXP_API_URL` to your server's base URL:
```bash
export CXP_API_URL=https://your-server.com
cxp auth <your-key>
```

---

## License

MIT — free for personal and commercial use.

Built with Rust. Contributions welcome.
