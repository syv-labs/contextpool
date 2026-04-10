# Team Commands

## `cxp auth`

Authenticate with your team API key. The key is saved to the system keychain, or you can use `CXP_API_KEY` env var instead.

```bash
cxp auth cxp_team_4c4143d879ce471ab14afacb081a7b4f
# Authenticated as: your-team-name (free plan)
# Key saved.

cxp auth --status    # show current auth status
cxp auth --logout    # remove stored API key from keychain
```

---

## `cxp push [OPTIONS]`

Push local insights to the team pool.

```bash
cxp push             # push from current project's ContextPool/
cxp push --all       # push all local projects, not just current directory
cxp push --dry-run   # show what would be pushed without pushing
```

What happens:
1. Reads all `.summary.md` files in `ContextPool/`
2. Strips any secrets (API keys, tokens, connection strings)
3. Sends only new insights (deduplication by content hash)
4. Reports how many were added vs. skipped

```
Pushed 12 insight(s). 3 already existed (skipped).
```

---

## `cxp pull`

Download all team insights to the local cache.

```bash
cxp pull
# Pulled 47 team insight(s) for project: your-project
# Cached at: ~/.cache/contextpool/team-cache/your-project/team-insights.md
```

Your agent's `search_context` and `get_project_context` calls automatically include these cached insights.

---

## `cxp team`

Show team info: name, plan, member count.

```bash
cxp team
# Team:    your-team-name
# Plan:    free
# Members: 2
# Limit:   3 members, 1000 insights, 5 projects
```

---

## `cxp team projects`

List all projects the team has contributed insights for, with insight counts.

```bash
cxp team projects
# my-api          34 insights
# frontend        12 insights
# infra-scripts    8 insights
```

---

## Environment Variables

| Variable | Description |
|---|---|
| `CXP_API_KEY` | Team API key (alternative to keychain) |
| `CXP_API_URL` | Override the API endpoint (default: `https://contextpool-server-nj1f.onrender.com`) |
