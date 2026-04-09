# Installation

No Rust toolchain required. Pick your platform:

---

## macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

The script downloads the pre-built binary, places it in `~/.local/bin`, and then runs `cxp install` automatically — which registers the MCP server with Claude Code and Cursor and walks you through picking an LLM backend.

## Windows

```powershell
irm https://raw.githubusercontent.com/syv-labs/cxp/main/install.ps1 | iex
```

## Pin a Specific Version

```bash
# macOS / Linux
CXP_VERSION=0.1.0 curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh

# Windows
$env:CXP_VERSION = "0.1.0"
irm https://raw.githubusercontent.com/syv-labs/cxp/main/install.ps1 | iex
```

## Build From Source

```bash
git clone https://github.com/syv-labs/cxp
cd cxp/contextpool
cargo build --release
# binary at target/release/cxp
```

---

## Verify

```bash
cxp --version
# cxp 0.1.0
```

---

## What `cxp install` Does

After the binary is placed, `cxp install` runs automatically and does two things:

1. **Registers the MCP server** — writes a `contextpool` entry to `~/.claude.json` (Claude Code) and `~/.cursor/mcp.json` (Cursor)
2. **Backend wizard** — asks which LLM you want to use for summarization and saves the key securely to your keychain

You can re-run this at any time:

```bash
cxp install              # re-register MCP + re-run wizard if not yet configured
cxp install --setup      # re-run just the backend wizard
cxp install --force      # overwrite existing entries
```

---

## Next Steps

→ [Quickstart](quickstart.md) — understand what just got set up.
