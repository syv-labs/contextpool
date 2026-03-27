# Roadmap

ContextPool is actively developed. Here's what's coming.

---

## Near-Term

### Semantic Search
`search_context` currently does keyword matching. The next release adds vector embeddings so you can search by meaning — "how did we handle rate limiting?" — and get relevant results even when the exact words don't match.

### Auto-Indexing Hooks
Configure `cxp` to automatically index new sessions when your IDE closes or a git commit is made. Today you rely on the MCP tool being called by your agent, or you run `cxp init` manually.

### VS Code Extension
A native sidebar panel for browsing and searching your `ContextPool/` summaries without needing an AI agent. Useful for reviewing past decisions and bugs directly in the editor.

---

## Medium-Term

### Web Dashboard
A local browser UI (`cxp serve`) for full-text and semantic search across all your projects' summaries. Think of it as a searchable engineering notebook built from your actual coding sessions — with no third-party service involved.

### More IDE Sources
Planned support for:
- **JetBrains AI Assistant** — IntelliJ, WebStorm, PyCharm
- **GitHub Copilot Chat** — VS Code Copilot session storage
- **Zed AI** — Zed editor agent transcripts

### Custom Extraction Prompts
Configure the summarization prompt per-project or per-team — focus on security insights, infrastructure decisions, a specific tech stack, or your team's conventions.

---

## Longer-Term

### Summary Deduplication
Detect near-duplicate insights across sessions (the same bug fixed twice, the same decision revisited) and merge or flag them, keeping the index clean over time.

### GitHub Actions Integration
A CI action that runs `cxp export` on push, automatically keeping team summaries current without anyone needing to remember to `cxp push`.

### Insight Aging
Surface recently-added insights more prominently, and flag insights that might be stale based on related code changes in git history.

---

## Suggest a Feature

Open an issue at [github.com/syv-labs/cxp](https://github.com/syv-labs/cxp). The roadmap is driven by what engineers actually need — if something's blocking you, say so.
