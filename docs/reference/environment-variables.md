# Environment Variables

## LLM Backend

The active backend is resolved in this order:

1. **Explicit preference** saved by `cxp install --setup` (stored in keychain + local file)
2. **Environment variables** (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `NVIDIA_API_KEY`)
3. **Keychain / file cache** from a previous `cxp install --setup` run
4. **Claude Code CLI fallback** — if none of the above, and `claude` is in PATH

The recommended way to configure a backend is `cxp install --setup`, which presents the options as distinct choices with clear trade-offs. See [Claude Code setup](../ide-setup/claude-code.md) and [Cursor setup](../ide-setup/cursor.md).

**Claude Code** (subscription, via `claude -p`) and **Anthropic API** (direct HTTP, billed per token) are separate options — the former is free but slower; the latter is faster and parallelizes better.

### Backend environment variables

| Variable | Backend |
|---|---|
| `ANTHROPIC_API_KEY` | Anthropic Messages API |
| `OPENAI_API_KEY` | OpenAI chat completions |
| `NVIDIA_API_KEY` | NVIDIA NIM (OpenAI-compatible) |

If set, env vars override the saved preference for that process.

Reset a saved NVIDIA key:
```bash
cxp --reset-nvidia-api-key
```

Reset Anthropic or OpenAI keys:
```bash
cxp install --setup    # re-run the wizard and pick a new backend
```

---

## Team Sync

| Variable | Default | Description |
|---|---|---|
| `CXP_API_KEY` | — | Team API key. Alternative to `cxp auth` — useful in CI or containers. |
| `CXP_API_URL` | `https://api.contextpool.dev` | Override the API endpoint. Use for self-hosted servers or local dev. |

---

## Performance Tuning

| Variable | Default | Description |
|---|---|---|
| `CXP_CONCURRENCY` | `8` | Max concurrent LLM calls during `fetch_project_context` and `cxp init`. Increase for faster bulk processing; decrease if you hit rate limits. |
| `CXP_MAX_CHARS` | `20000` | Transcript characters sent to the LLM per session. Lowering this (e.g. `8000`) speeds up each call significantly with minimal quality loss. |

Example — tune for speed on a large project:

```bash
CXP_CONCURRENCY=12 CXP_MAX_CHARS=8000 cxp init claude-code --local
```

---

## LLM Tuning

| Variable | Default | Description |
|---|---|---|
| `MODEL` | per-backend default¹ | Override the model used for summarization. |
| `REPAIR_MODEL` | same as `MODEL` | Model used for JSON repair if the first response is malformed. |
| `TEMPERATURE` | `0.0` | Sampling temperature. |
| `TOP_P` | `0.95` | Top-p nucleus sampling. |
| `MAX_COMPLETION_TOKENS` | `4096` | Max tokens per LLM response. |

¹ Default models:
- Claude Code CLI / Anthropic: `claude-haiku-4-5-20251001`
- OpenAI: `gpt-4o-mini`
- NVIDIA NIM: `qwen/qwen3.5-122b-a10b`

---

## Extraction Behavior

| Variable | Default | Description |
|---|---|---|
| `SANITIZE_CHAT` | `1` | Strip tool calls, thinking blocks, file snapshots, and injected context before sending to the LLM. Set to `0` to disable. |
| `EXTRACT_USER_QUERIES_ONLY` | `0` | Send only user messages to the LLM, not assistant replies. Set to `1` to enable. |

---

## Debugging

| Variable | Default | Description |
|---|---|---|
| `DEBUG_LLM_OUTPUT` | `0` | Print the raw LLM response to stderr before parsing. Set to `1` to enable. Useful for diagnosing extraction failures. |

---

## In MCP Config (Cursor)

If you prefer not to use the keychain, pass env vars directly in `~/.cursor/mcp.json`. This is also useful in headless or container environments:

```json
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp"],
      "env": {
        "ANTHROPIC_API_KEY": "sk-ant-...",
        "CXP_API_KEY": "cxp_team_...",
        "CXP_CONCURRENCY": "10"
      }
    }
  }
}
```
