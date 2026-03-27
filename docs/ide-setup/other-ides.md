# Other IDEs

## Windsurf

Windsurf stores chat history in a VS Code-style workspace database. Export it with:

```bash
cxp export vscdb --product Windsurf
```

To specify a custom workspace storage path:

```bash
cxp export vscdb --product Windsurf --workspace-storage "/path/to/workspaceStorage"
```

Then add `cxp mcp` to Windsurf's MCP config (same format as Cursor).

---

## VS Code Forks (General)

Any VS Code fork that stores chat in `state.vscdb` files works with `cxp export vscdb`.

Default workspace storage paths:

| Platform | Path |
|---|---|
| macOS (Cursor) | `~/Library/Application Support/Cursor/User/workspaceStorage` |
| Windows (Cursor) | `%APPDATA%\Cursor\User\workspaceStorage` |
| macOS (Windsurf) | `~/Library/Application Support/Windsurf/User/workspaceStorage` |

---

## Kiro

Kiro lets you export sessions with `/chat save <path>`. Once saved:

```bash
cxp export kiro --chat-json ./kiro-session.json
```

---

## Any MCP-Compatible IDE

If your IDE supports MCP servers, add this config:

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

The four MCP tools — `fetch_project_context`, `get_project_context`, `search_context`, `list_context_projects` — work in any compliant MCP client.

---

## Coming Soon

Native support planned for:
- JetBrains AI Assistant (IntelliJ, WebStorm, etc.)
- GitHub Copilot Chat
- Zed AI

See the [Roadmap](../roadmap.md) for details.
