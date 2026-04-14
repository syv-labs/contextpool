# Team Sync Overview

## Your team's knowledge should be shared knowledge.

When you debug a tricky issue and find the fix, that insight lives in your `ContextPool/`. Your teammate — working on the same codebase the next week — hits the same issue. Their agent has no idea you already solved it.

**Team Sync closes that gap.** Push your local insights to a shared cloud pool. Pull your teammates' insights locally. When your agent searches for a bug or decision, it searches the entire team's memory — not just yours.

---

## How It Works

```
Engineer A                     ContextPool Cloud              Engineer B
─────────────────              ──────────────────             ─────────────────
cxp push              →        team insight pool      →       cxp pull
(local insights)               (deduplicated,                 (team insights at
                                quota-enforced)                ~/.cache/contextpool/)
                                                               ↓
                                                     agent searches both local
                                                     + team insights automatically
```

Insights are deduplicated by content hash — pushing the same insight twice doesn't create duplicates. Secrets are redacted before anything leaves your machine.

---

## Privacy

- **Secrets are stripped before push.** API keys, tokens, JWTs, connection strings, and other secrets are redacted automatically before any insight is sent to the cloud.
- **You control what leaves your machine.** Only insights you explicitly `cxp push` are shared. Nothing is synced automatically.
- **Your raw chat transcripts never leave your machine.** Only the distilled, structured summaries are pushed — not the full conversation.

---

## Plans

| | Free | Paid |
|---|---|---|
| Insights | 1,000 | Unlimited |
| Projects | 5 | Unlimited |
| Members | 3 | Unlimited |
| Price | Free | [contextpool.io](https://contextpool.io) |

---

## Get Started

→ [Setup](setup.md) — create your team and get your API key in 2 minutes.
