# Introduction

## Your AI agent forgets everything. ContextPool fixes that.

Every time you open a new chat with an AI coding agent, it starts completely cold. It doesn't know about the bug you spent three hours tracing last week. It doesn't remember the architectural trade-off you documented on Tuesday. It has no idea about the non-obvious gotcha you hit last month — so it walks right into it again.

You end up doing one of two things: re-explaining your entire project every session, or watching your agent repeat your past mistakes.

**ContextPool is persistent memory for AI coding agents.** It reads your Cursor and Claude Code sessions, distils them into structured engineering insights — bugs found, root causes, design decisions, gotchas — and feeds them back to your agent automatically via MCP. Every session starts smarter than the last.

---

## See It In Action

```
$ cxp init claude-code --local

  Found 14 Claude Code session(s) for this project.
  Summarized 14 session(s) → 47 insight(s) extracted.

  Top insights:
    bug      ESM import fails silently in test runner
             → add "type": "module" to package.json
    decision chose SQLite over Postgres for local-only storage
             → latency requirements don't justify the ops overhead
    gotcha   reqwest needs rustls-tls feature, not default openssl
             → linking fails on Alpine and macOS ARM without it

  Your agent will now recall these automatically via MCP.
```

The next time you open this project in Claude Code or Cursor, your agent already knows all of this. It won't re-discover. It won't re-explain. It will just work.

---

## How It Works

```
IDE chat transcript (.jsonl)
        │
        ▼
   cxp extracts clean conversation turns
   (drops tool calls, thinking blocks, file noise)
        │
        ▼
   LLM distils into structured insights
   (type, title, summary, tags, related file)
        │
        ▼
   Stored as markdown in ContextPool/
        │
        ▼
   MCP server surfaces them on demand
        │
        ▼
   Agent loads relevant context before every response
```

Indexing is incremental — only new, unprocessed sessions are summarized on each run. Everything lives on your machine, next to your code.

---

## One Command to Set Up Everything

```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

This installs the binary, registers the MCP server with both Claude Code and Cursor, and walks you through picking an LLM backend. Claude Code users can choose the **free** Claude Code backend — no API key needed. Anthropic, OpenAI, and NVIDIA are also available; keys are saved to your keychain.

---

## Works Across Every AI IDE

| IDE | How transcripts are found |
|---|---|
| Claude Code | `~/.claude/projects/` JSONL sessions |
| Cursor | `~/.cursor/` agent transcripts |
| Windsurf / VS Code forks | `workspaceStorage/*/state.vscdb` |
| Kiro | Exported via `/chat save` |

---

## What Gets Stored

Each session produces up to 5 structured insights, each with:

| Field | Description |
|---|---|
| `type` | `bug`, `fix`, `decision`, `pattern`, or `gotcha` |
| `title` | Short headline used in the index |
| `summary` | Actionable insight, ≤200 characters |
| `tags` | Keywords for search |
| `file` | Related source file, if applicable |

High signal only. The extraction is tuned to drop generic explanations and exploratory chatter — only actionable insights tied to real code or real decisions are kept.

---

## Ready to start?

→ [Install cxp](getting-started/installation.md) — takes 30 seconds, no Rust required.
