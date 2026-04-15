# CLI Reference

## `cxp install [OPTIONS]`

Register the contextpool MCP server with Claude Code and Cursor, and configure an LLM backend.

```bash
cxp install                          # register MCP + run backend wizard
cxp install --setup                  # re-run just the backend wizard
cxp install --force                  # overwrite existing MCP entries
cxp install --skip-claude            # skip Claude Code registration
cxp install --skip-cursor            # skip Cursor registration
cxp install --skip-setup             # skip the backend wizard (non-interactive installs)
cxp install --binary-path <path>     # path to register (defaults to current executable)
cxp install --claude-json <path>     # override ~/.claude.json path
cxp install --cursor-mcp <path>      # override ~/.cursor/mcp.json path
```

The wizard presents four backends as distinct options:

| Choice | Description |
|---|---|
| Claude Code | Free, uses your existing Claude Code subscription. Spawns `claude -p`. |
| Anthropic API | Direct HTTP to the Messages API. Fastest, parallelizes well, works headless. |
| OpenAI API | OpenAI chat completions. Good if you already have a key. |
| NVIDIA NIM | OpenAI-compatible endpoint via NVIDIA. |

Keys are saved to the system keychain and a `0600` local file — no env vars required after setup.

---

## `cxp init`

Extract and store insights from your IDE sessions into the current project.

### `cxp init claude-code [session-ids...] [OPTIONS]`

```bash
cxp init claude-code                       # all sessions for this project
cxp init claude-code <id1> <id2>          # specific session ids
cxp init claude-code --local               # write to ./ContextPool/
cxp init claude-code --out ./store        # custom output directory
cxp init claude-code --claude-dir <path>  # custom ~/.claude root
```

### `cxp init cursor [chat-ids...] [OPTIONS]`

```bash
cxp init cursor                            # all Cursor chats for this project
cxp init cursor <id1> <id2>               # specific chat ids
cxp init cursor --local                    # write to ./ContextPool/
cxp init cursor --out ./store             # custom output directory
cxp init cursor --cursor-dir <path>       # custom ~/.cursor root
```

Sessions with no extractable insights are skipped — no empty `.summary.md` files are written.

---

## `cxp export`

Bulk-export and summarize transcripts from any supported IDE.

### `cxp export claude-code [OPTIONS]`

```bash
cxp export claude-code                     # all Claude Code sessions
cxp export claude-code --session <path>   # single .jsonl session file
cxp export claude-code --out ./out        # custom output directory
cxp export claude-code --claude-dir <path>
cxp export claude-code --offline          # skip LLM, write fallback summaries
```

### `cxp export cursor [OPTIONS]`

```bash
cxp export cursor                          # all Cursor transcripts
cxp export cursor --transcript <path>     # single .jsonl file
cxp export cursor --out ./out             # custom output directory
cxp export cursor --cursor-dir <path>
cxp export cursor --offline
```

### `cxp export vscdb [OPTIONS]`

Export from VS Code-style `state.vscdb` workspace storage (Cursor, Windsurf, other forks).

```bash
cxp export vscdb --product Cursor
cxp export vscdb --product Windsurf
cxp export vscdb --product Cursor --workspace-storage "<custom-path>"
```

Default paths:
- macOS (Cursor): `~/Library/Application Support/Cursor/User/workspaceStorage`
- Windows (Cursor): `%APPDATA%\Cursor\User\workspaceStorage`

### `cxp export kiro [OPTIONS]`

Export a Kiro session exported with `/chat save <path>`.

```bash
cxp export kiro --chat-json ./kiro-session.json
```

---

## `cxp mcp [OPTIONS]`

Start the MCP server. Used by your IDE config — not normally called directly.

```bash
cxp mcp
cxp mcp --data-dir ./ContextPool   # custom data directory
```

---

## `cxp auth [team-key] [OPTIONS]`

Authenticate with a ContextPool team API key.

```bash
cxp auth cxp_team_4c4143d879ce471ab14afacb081a7b4f   # save key to keychain
cxp auth --status                                      # show current auth status
cxp auth --logout                                      # remove stored API key
```

---

## `cxp push [OPTIONS]`

Push local insights to the team cloud pool.

```bash
cxp push
cxp push --all      # push all local projects, not just current directory
cxp push --dry-run  # show what would be pushed without pushing
```

---

## `cxp pull [OPTIONS]`

Pull team insights to local cache.

```bash
cxp pull
cxp pull --all      # pull all team projects, not just current directory
```

---

## `cxp team [ACTION]`

Show team info or list projects.

```bash
cxp team              # show team info
cxp team projects     # list team projects with insight counts
```

---

## Global Flags

```bash
cxp --reset-nvidia-api-key    # clear saved NVIDIA key from keychain
cxp --version                 # print version
cxp --help                    # list commands
```

To reset Anthropic or OpenAI keys, re-run `cxp install --setup` and choose a new backend.
