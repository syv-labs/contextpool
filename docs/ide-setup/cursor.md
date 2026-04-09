# Cursor

## Setup

The easiest way is to run the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

Or, if you already have the binary:

```bash
cxp install
```

This writes the `contextpool` MCP entry to `~/.cursor/mcp.json` and runs the backend setup wizard. Your API key is saved to the system keychain — no need to hardcode it in the config file.

Restart Cursor to activate.

---

## LLM Backend

`cxp install` runs a wizard that lets you choose and save your backend. Run it interactively:

```bash
cxp install --setup
```

```
Which backend should ContextPool use for summarization?

  1) Claude Code  — free, uses your Claude Code subscription
                    (slower, ~1 subprocess at a time)
  2) Anthropic API — direct API, billed per token, fastest
                    (parallelizes well, works headless)
  3) OpenAI API
  4) NVIDIA NIM
  5) Skip — I'll configure this later
```

Your choice is saved to the system keychain. The MCP subprocess picks it up automatically — no env vars in the config file needed.

---

## Manual API Key (Alternative)

If you prefer to manage keys explicitly — or if you're on a machine where the keychain isn't available — you can pass the key directly in `~/.cursor/mcp.json`:

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

Supported env vars: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `NVIDIA_API_KEY`. If an env var is set, it takes precedence over the keychain.

---

## Use Agent Mode

ContextPool works in **Agent mode** in Cursor. The agent calls `fetch_project_context` automatically before responding. In regular chat mode (non-agent), the tools aren't invoked automatically — you'd need to ask explicitly.

---

## Pre-populate Memory

```bash
cd your-project/
cxp init cursor --local
```

Processes all Cursor sessions for this project and writes summaries to `./ContextPool/`. Sessions with no extractable insights are skipped — no empty files. Run once after setup; the MCP tool handles incremental indexing from there.

---

## Troubleshooting

**Tools not appearing in Cursor**
Restart Cursor after running `cxp install`. Check **Settings → MCP** to confirm the server status shows green.

**"No LLM backend available"**
Run `cxp install --setup` to configure a backend. If you set a key via the wizard, verify it was saved: run `cxp install --setup` again — it will show the existing key and ask if you want to reuse it.

**Sessions not found**
Cursor stores transcripts in `~/.cursor/`. If you're using a non-standard Cursor installation, pass `--cursor-dir` to `cxp init cursor` with the correct path.
