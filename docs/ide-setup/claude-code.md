# Claude Code

## Setup

Add `contextpool` to your MCP servers in `~/.claude/settings.json`:

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

Restart Claude Code. No API key needed — `cxp` uses the `claude` CLI that ships with Claude Code.

---

## How the Agent Behaves

Once connected, Claude follows smart rules automatically:

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

This processes all Claude Code sessions for the current project and writes summaries to `./ContextPool/`. Run it once after installing, then let the MCP tool handle incremental indexing going forward.

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

## Troubleshooting

**"No LLM backend available"**
Make sure `claude` is in your PATH. Run `which claude` — if it's not found, reinstall Claude Code or run `npm i -g @anthropic-ai/claude-code`.

**No insights appearing after init**
Check that sessions exist for the current project: `ls ~/.claude/projects/`. The project is matched by the absolute path of the current directory.

**Slow first fetch**
The first `fetch_project_context` call processes all unindexed sessions. For projects with many sessions this can take 10–30 seconds. Subsequent calls only process new sessions and are near-instant.
