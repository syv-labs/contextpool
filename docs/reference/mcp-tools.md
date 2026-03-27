# MCP Tools

When running as an MCP server, `cxp` exposes four tools to your AI agent.

---

## `fetch_project_context`

Discovers and summarizes new transcripts for the current project. Returns a compact index of all available summaries — ids, types, titles, and tags. Call this first to get the list, then use `get_project_context` to load specific ones.

**When to call:** At the start of a new conversation, or when the user references past work.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `project_path` | string | No | Absolute path to the project root. Defaults to cwd. |

**Returns:** A markdown index listing all available summaries with ids, titles, types, and tags.

---

## `get_project_context`

Loads the full markdown content of selected summaries into the agent's context window.

**When to call:** After `fetch_project_context`, when specific summaries look relevant.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `project_path` | string | No | Absolute path to the project root. Defaults to cwd. |
| `ids` | string[] | No | Summary ids from `fetch_project_context`. Omit to load all. |

**Returns:** Full markdown content of the selected summaries.

---

## `search_context`

Full-text search across all stored summaries. Searches both local `ContextPool/` and any cached team insights.

**When to call:** When the user mentions a specific bug, error message, component, or decision. Before suggesting solutions to a problem — check if it was already solved.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `query` | string | Yes | Keyword or phrase to search for. |
| `project_path` | string | No | Limit search to one project. Omit to search all projects. |

**Returns:** Matching insight excerpts with source file references.

---

## `list_context_projects`

Lists all projects that have stored summaries, with insight counts.

**When to call:** When working across multiple repos, or when the user asks what's in memory.

*(No parameters)*

**Returns:** A list of project paths and their summary counts.

---

## Agent Behavior Rules

The MCP server ships with built-in instructions that guide your agent to use these tools intelligently:

- **First message in a new project** → auto-call `fetch_project_context`
- **User references a past conversation** → `search_context` first, only `fetch` if nothing found
- **Debugging an error** → `search_context` with the error message or component name
- **Making an architectural decision** → `search_context` to check prior decisions

These instructions are embedded in the MCP server and don't require any configuration.
