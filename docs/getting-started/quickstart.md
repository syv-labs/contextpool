# Quickstart

## The Recommended Way — 30 Seconds

Run the install script. It handles everything:

```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

This installs the binary, registers the MCP server with both Claude Code and Cursor, and runs the backend setup wizard. **Restart your IDE and you're done.**

---

## Manual Setup — Claude Code

If you installed from source or want to update an existing install:

**1. Register the MCP server:**

```bash
cxp install --skip-cursor
```

This writes the `contextpool` entry to `~/.claude.json` (Claude Code's global config). Do not use `~/.claude/settings.json` — that file does not support MCP server definitions.

**2. Pick an LLM backend** (if not already done):

```bash
cxp install --setup
```

**3. Restart Claude Code.**

No API key is required if you choose the **Claude Code** backend — it uses your existing Claude Code subscription.

---

## Manual Setup — Cursor

**1. Register the MCP server:**

```bash
cxp install --skip-claude
```

This writes the `contextpool` entry to `~/.cursor/mcp.json`.

**2. Pick an LLM backend:**

```bash
cxp install --setup
```

The chosen key is saved to your system keychain — no need to paste it into `mcp.json`.

**3. Restart Cursor.**

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

Each `.summary.md` contains structured insights from that session. Sessions with no high-signal content are skipped — no empty files. The next time your agent opens this project, it loads these automatically.

---

→ [Your First Insights](first-insights.md) — understand what gets extracted and how to get more out of it.
