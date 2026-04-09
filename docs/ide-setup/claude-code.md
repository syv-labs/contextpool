# Claude Code

## Setup

The easiest way is to run the install script, which handles registration automatically:

```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

Or, if you already have the binary:

```bash
cxp install
```

This writes the `contextpool` MCP entry to `~/.claude.json` — Claude Code's global config file. Do **not** add it to `~/.claude/settings.json`; that file does not support MCP server definitions.

The resulting entry looks like:

```json
{
  "mcpServers": {
    "contextpool": {
      "type": "stdio",
      "command": "/path/to/cxp",
      "args": ["mcp"]
    }
  }
}
```

Restart Claude Code to activate.

---

## LLM Backend

`cxp install` runs a wizard that lets you pick your backend. Claude Code and the Anthropic API are separate options with different trade-offs:

| Option | Cost | Speed | Notes |
|---|---|---|---|
| **Claude Code** | Free (uses your subscription) | Slower | Uses `claude -p` subprocess. Best for occasional use. |
| **Anthropic API** | Billed per token | Fastest | Direct HTTP. Parallelizes well. Required for headless contexts. |
| **OpenAI API** | Billed per token | Fast | Good alternative if you already have a key. |
| **NVIDIA NIM** | Billed per token | Fast | OpenAI-compatible endpoint. |

Your choice is saved to the system keychain. To change it later:

```bash
cxp install --setup
```

---

## How the Agent Behaves

Once connected, Claude follows these rules automatically:

**First conversation in a new project**
Claude calls `fetch_project_context` before responding. It discovers and indexes any new sessions, then loads relevant summaries. You don't need to ask — it happens on the first message.

**"Remember when we fixed that auth bug?"**
Claude searches your memory first with `search_context`. It only re-fetches and re-indexes if the search returns nothing.

**Debugging or hitting an error**
Claude searches for the error message or component name before suggesting solutions. If you already solved this, it finds the fix.

**Working across projects**
Claude can call `list_context_projects` to see all projects with stored memory — useful when you work across multiple repos.

---

## Project-Level Setup

To pre-populate memory from existing sessions before the agent even starts:

```bash
cd your-project/
cxp init claude-code --local
```

This processes all Claude Code sessions for the current project and writes summaries to `./ContextPool/`. Run it once after installing, then let the MCP tool handle incremental indexing going forward. Sessions with no extractable insights are skipped — no empty files.

---

## Per-Project MCP Config

If you want different settings per project, add a `.mcp.json` file in the project root:

```json
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp", "--data-dir", "./ContextPool"]
    }
  }
}
```

---

## Performance Tuning

The MCP server runs up to 8 summarization calls concurrently by default. For large projects you can tune this:

```bash
# In your shell profile, or in the MCP server env block:
CXP_CONCURRENCY=12    # more parallel LLM calls (watch rate limits)
CXP_MAX_CHARS=8000    # shorter transcript slice → faster per-call (default: 20000)
```

---

## Troubleshooting

**"No LLM backend available"**
Run `cxp install --setup` to configure a backend. If you chose Claude Code, make sure `claude` is in PATH: `which claude`. If not found, reinstall Claude Code or run `npm i -g @anthropic-ai/claude-code`.

**No insights appearing after init**
Check that sessions exist for the current project: `ls ~/.claude/projects/`. The project is matched by the absolute path of the current directory.

**Slow first fetch**
The first `fetch_project_context` call processes all unindexed sessions. For large projects, run `cxp init claude-code` from the CLI first — it's faster and gives you progress output. Subsequent MCP calls only process new sessions.
