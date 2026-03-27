# Your First Insights

## What Gets Extracted

ContextPool reads your chat transcripts and asks an LLM to extract the moments that are actually worth remembering. Not summaries of conversations — specific, actionable engineering insights.

Each insight has a **type**:

| Type | What it captures |
|---|---|
| `bug` | A specific bug found — what it was, what caused it |
| `fix` | The solution applied — the exact change that resolved it |
| `decision` | An architectural or design choice made and why |
| `pattern` | A reusable approach or convention established |
| `gotcha` | A non-obvious pitfall that took time to discover |

---

## Example Output

Given a session where you debugged a Rust build failure, `cxp` might extract:

```markdown
- **gotcha** reqwest needs `rustls-tls` feature on Alpine/ARM — default OpenSSL fails to link
- **decision** switched from `tokio::spawn` to `rayon` for CPU-bound extraction — avoids blocking the async runtime
- **fix** added `CARGO_NET_GIT_FETCH_WITH_CLI=true` to fix private crate auth in CI
```

These are stored in a `.summary.md` file and indexed by the MCP server. Your agent can retrieve them with `get_project_context` or find them by keyword with `search_context`.

---

## Getting Higher Quality Insights

**Have longer, more specific conversations.**
Short back-and-forth chats produce fewer insights. Sessions where you work through a real problem — debugging, designing, refactoring — tend to produce the most valuable memory.

**Name things explicitly in chat.**
If you say "let's use a queue here instead of polling", that's a decision worth remembering. The more explicit the reasoning in your session, the better the extraction.

**Run `cxp init` after sessions, not just before.**
Index new sessions regularly so your memory stays current. The MCP `fetch_project_context` tool does this automatically — but running `cxp init` manually from the CLI is faster for bulk historical imports.

---

## What Gets Filtered Out

The extraction is aggressive about dropping noise:

- Tool call results and file contents injected by the IDE
- Generic explanations ("here's how async/await works...")
- Exploratory chat with no concrete outcome
- Thinking blocks and internal reasoning traces
- Lines longer than 500 characters (usually raw data)

Only what's worth telling a future engineer — or a future agent — makes it through.

---

## Sharing With Your Team

Commit your `ContextPool/` directory to git. Your teammates will automatically have your insights the next time they clone or pull:

```bash
git add ContextPool/
git commit -m "chore: add contextpool memory"
```

Or use [Team Sync](../team-sync/overview.md) for automatic cloud sharing across your whole engineering team.
