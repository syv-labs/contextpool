# Installation

No Rust toolchain required. Pick your platform:

---

## macOS / Linux

```bash
curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh
```

The script downloads the pre-built binary for your platform and places it in `~/.local/bin` (or `/usr/local/bin` if writable).

## Windows

```powershell
irm https://raw.githubusercontent.com/syv-labs/cxp/main/install.ps1 | iex
```

## Pin a Specific Version

```bash
# macOS / Linux
CONTEXTPOOL_VERSION=0.1.0 curl -fsSL https://raw.githubusercontent.com/syv-labs/cxp/main/install.sh | sh

# Windows
$env:CONTEXTPOOL_VERSION = "0.1.0"
irm https://raw.githubusercontent.com/syv-labs/cxp/main/install.ps1 | iex
```

## Build From Source

```bash
git clone https://github.com/syv-labs/cxp
cd cxp
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

## Next Steps

→ [Quickstart](quickstart.md) — add `cxp` to Claude Code or Cursor in under a minute.
