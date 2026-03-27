# Environment Variables

## LLM Backend

`cxp` tries these backends in order and uses the first available:

| Priority | Backend | Variable |
|---|---|---|
| 1 | `claude` CLI (zero-config) | Ships with Claude Code. Just needs `claude` in PATH. |
| 2 | Anthropic API | `ANTHROPIC_API_KEY` |
| 3 | OpenAI API | `OPENAI_API_KEY` |
| 4 | NVIDIA NIM | `NVIDIA_API_KEY` (saved to keychain on first use) |

Reset a saved NVIDIA key:
```bash
cxp --reset-nvidia-api-key
```

---

## Team Sync

| Variable | Default | Description |
|---|---|---|
| `CXP_API_KEY` | — | Team API key. Alternative to `cxp auth` — useful in CI or containers. |
| `CXP_API_URL` | `https://api.contextpool.dev` | Override the API endpoint. Use for self-hosted servers or local dev. |

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

Since Cursor doesn't expose its own auth to MCP subprocesses, pass env vars in the server config:

```json
{
  "mcpServers": {
    "contextpool": {
      "command": "cxp",
      "args": ["mcp"],
      "env": {
        "ANTHROPIC_API_KEY": "sk-ant-...",
        "CXP_API_KEY": "cxp_team_..."
      }
    }
  }
}
```
