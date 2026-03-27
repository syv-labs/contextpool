# Security & Privacy

## Your data stays on your machine

ContextPool is local-first by design. Your raw chat transcripts are never sent anywhere. The only thing that ever leaves your machine is:

1. **Cleaned, distilled summaries** — sent to your chosen LLM backend for summarization (Anthropic, OpenAI, NVIDIA, or the local `claude` CLI)
2. **Team insights you explicitly push** — only when you run `cxp push`

---

## Secret Redaction

Before any content is sent to an LLM or pushed to the cloud, ContextPool scans for secrets and replaces them with `[REDACTED]`.

Patterns detected and redacted:

| Pattern | Example |
|---|---|
| AWS access keys | `AKIA...` |
| AWS secret keys | 40-char alphanumeric after `aws_secret` |
| Generic API keys | `api_key = "..."`, `apiKey: "..."` |
| Bearer tokens | `Authorization: Bearer ...` |
| JWTs | `eyJ...` |
| Database connection strings | `postgres://user:pass@host/db` |
| Private key blocks | `-----BEGIN PRIVATE KEY-----` |
| GitHub tokens | `ghp_...`, `ghs_...` |
| Slack tokens | `xoxb-...`, `xoxp-...` |
| Generic high-entropy strings | Long random-looking values in common key patterns |

Redaction happens **twice** when pushing to the cloud — once during summarization, once before the push — to minimize any leakage path.

---

## LLM Backend Privacy

When `cxp` sends a cleaned transcript to an LLM for summarization, the data is subject to that provider's privacy policy:

| Backend | Data handling |
|---|---|
| `claude` CLI (local) | Processed by Anthropic API under your Claude account |
| Anthropic API | Anthropic's [privacy policy](https://www.anthropic.com/privacy) |
| OpenAI API | OpenAI's [privacy policy](https://openai.com/policies/privacy-policy) |
| NVIDIA NIM | NVIDIA's privacy policy |

If you want summaries to stay 100% local, host your own LLM and point `MODEL` and your API key at it, or use the Anthropic API with [zero data retention](https://www.anthropic.com/privacy) enabled on your account.

---

## Team Sync Privacy

When using Team Sync:

- **Secrets are stripped twice** before any insight leaves your machine
- **Raw transcripts are never uploaded** — only structured summaries
- **You control what's shared** — nothing syncs automatically; you must run `cxp push`
- **API keys are stored in your system keychain** — not in plain text config files
- **Transport is HTTPS** — all API calls use TLS

---

## Open Source

The `cxp` CLI is open source (MIT). You can read exactly what it does with your data:

```
github.com/syv-labs/cxp
```

The cloud sync server is private infrastructure. The API contract is public and documented — you can self-host a compatible server if you prefer full control.
