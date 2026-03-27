# Cursor

## Setup

Add `contextpool` to Cursor's MCP config. Go to **Settings → MCP → Add Server**, or edit `~/.cursor/mcp.json` directly:

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

Cursor doesn't expose its own auth to MCP subprocesses, so you need to provide an API key explicitly. See [all backend options](../reference/environment-variables.md).

---

## API Key Options

Provide any one of these in the `env` block:

```json
"env": {
  "ANTHROPIC_API_KEY": "sk-ant-..."
}
```

```json
"env": {
  "OPENAI_API_KEY": "sk-..."
}
```

```json
"env": {
  "NVIDIA_API_KEY": "nvapi-..."
}
```

`cxp` uses the first available backend in priority order: Anthropic → OpenAI → NVIDIA.

---

## Use Agent Mode

ContextPool works in **Agent mode** in Cursor. The agent calls `fetch_project_context` automatically before responding. In regular chat mode (non-agent), the tools aren't invoked automatically — you'd need to ask explicitly.

---

## Pre-populate Memory

```bash
cd your-project/
cxp init cursor --local
```

Processes all Cursor sessions for this project and writes summaries to `./ContextPool/`. Run once after setup; the MCP tool handles incremental indexing from there.

---

## Troubleshooting

**Tools not appearing in Cursor**
Restart Cursor after editing `mcp.json`. Check **Settings → MCP** to confirm the server status shows green.

**"No LLM backend available"**
Verify the `env` block in your MCP config has a valid API key. Test it by running `ANTHROPIC_API_KEY=sk-... cxp init cursor --local` from your terminal.

**Sessions not found**
Cursor stores transcripts in `~/.cursor/`. If you're using a non-standard Cursor installation, pass `--cursor-dir` to `cxp init cursor` with the correct path.
