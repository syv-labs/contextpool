# Data Storage

## Local Storage

By default, summaries are stored in `ContextPool/` inside your project directory:

```
your-project/
└── ContextPool/
    └── fetched/
        └── 2026-03-27T08-00-00Z/
            ├── abc123.summary.md    ← full insights for one session
            └── index.json           ← metadata index for fast lookup
```

Each `.summary.md` contains structured insights in this format:

```markdown
# Session abc123

- **bug** ESM import fails silently in test runner — add "type": "module" to package.json
- **gotcha** reqwest needs rustls-tls feature on Alpine/ARM — default OpenSSL fails to link
- **decision** chose SQLite over Postgres — latency requirements don't justify the ops overhead
```

`index.json` stores metadata (ids, titles, types, tags, timestamps) for fast retrieval without reading every summary file.

---

## Storage Locations

| Flag / Command | Where summaries are written |
|---|---|
| `cxp init --local` | `./ContextPool/` (next to your code) |
| `cxp init --out <dir>` | `<dir>/` |
| `cxp init` (no flag) | OS app data directory |
| `cxp mcp --data-dir <dir>` | `<dir>/` (MCP server reads from here) |
| `cxp export --out <dir>` | `<dir>/` |

The `--local` flag is recommended for most projects — it keeps memory next to the code and makes it easy to commit to git.

---

## Team Cache

After `cxp pull`, team insights are cached at:

```
~/.cache/contextpool/team-cache/<project-name>/team-insights.md
```

The MCP server's `search_context` and `get_project_context` tools automatically include these alongside local summaries.

---

## Committing to Git

Committing your `ContextPool/` directory is a great way to share memory with teammates without setting up Team Sync:

```bash
# Add to git
git add ContextPool/
git commit -m "chore: add contextpool memory"

# Or add to .gitignore if you prefer local-only
echo "ContextPool/" >> .gitignore
```

When a teammate clones the repo, they immediately have your insights available in their MCP sessions.

---

## Disk Usage

Each `.summary.md` file is typically 500–2,000 bytes. A project with 100 sessions produces roughly 50–200 KB of summary files. The overhead is negligible.
