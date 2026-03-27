# Quickstart

## Claude Code — 30 seconds

**1. Install `cxp`** → [Installation](installation.md)

**2. Add to `~/.claude/settings.json`:**

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

**3. Restart Claude Code.**

That's it. No API key. `cxp` uses the `claude` CLI that ships with Claude Code automatically.

---

## Cursor — 1 minute

**1. Install `cxp`**

**2. Add to `~/.cursor/mcp.json`** (or **Settings → MCP → Add Server**):

```json
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp"],
      "env": {
        "ANTHROPIC_API_KEY": "sk-ant-..."
      }
    }
  }
}
```

Cursor doesn't expose its own auth to subprocesses, so you need to provide one API key. See [Authentication](../reference/environment-variables.md) for all options.

---

## Pre-populate Your Memory

Run this once in any project to index your existing sessions before the agent ever asks:

```bash
cd your-project/

# From Claude Code sessions
cxp init claude-code --local

# From Cursor
cxp init cursor --local
```

Summaries are written to `./ContextPool/`. The `--local` flag keeps them next to your code — great for committing alongside the repo so teammates get your memory too.

---

## What to Expect

After the first run, your project will have:

```
your-project/
└── ContextPool/
    └── fetched/
        └── 2026-03-27T08-00-00Z/
            ├── abc123.summary.md
            └── index.json
```

Each `.summary.md` contains up to 5 structured insights from that session. The next time your agent opens this project, it loads these automatically.

---

→ [Your First Insights](first-insights.md) — understand what gets extracted and how to get more out of it.
