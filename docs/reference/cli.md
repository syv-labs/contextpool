# CLI Reference

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

---

## `cxp export`

Bulk-export and summarize transcripts from any supported IDE.

### `cxp export claude-code [OPTIONS]`

```bash
cxp export claude-code                     # all Claude Code sessions
cxp export claude-code --session <path>   # single .jsonl session file
cxp export claude-code --out ./out        # custom output directory
cxp export claude-code --claude-dir <path>
```

### `cxp export cursor [OPTIONS]`

```bash
cxp export cursor                          # all Cursor transcripts
cxp export cursor --transcript <path>     # single .jsonl file
cxp export cursor --out ./out             # custom output directory
cxp export cursor --cursor-dir <path>
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

## `cxp auth <team-key>`

Authenticate with a ContextPool team API key.

```bash
cxp auth cxp_team_4c4143d879ce471ab14afacb081a7b4f
```

---

## `cxp push [OPTIONS]`

Push local insights to the team cloud pool.

```bash
cxp push
cxp push --dir ./ContextPool
```

---

## `cxp pull`

Pull team insights to local cache.

```bash
cxp pull
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
