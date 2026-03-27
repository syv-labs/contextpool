# Team Sync Setup

## 1. Create Your Team

Go to [contextpool.dev](https://contextpool.dev) and click **Get Started**. Sign in with GitHub — your team is created automatically and your API key is shown on the dashboard.

Copy your team key. It looks like: `cxp_team_4c4143d879ce471ab14afacb081a7b4f`

---

## 2. Authenticate

```bash
cxp auth cxp_team_<your-key>
```

Or set it as an environment variable instead of using the keychain:

```bash
export CXP_API_KEY=cxp_team_<your-key>
```

Verify it worked:

```bash
cxp team
# Team: your-team-name
# Plan: free
# Members: 1
```

---

## 3. Push Your Insights

```bash
cd your-project/
cxp push
```

This collects all insights from `./ContextPool/`, redacts any secrets, and uploads them to the team pool. Only new insights (by content hash) are sent — pushing twice is safe.

---

## 4. Invite Teammates

Share your team API key with teammates. They run the same `cxp auth` command and can immediately push and pull.

```bash
# Teammate runs:
cxp auth cxp_team_<shared-key>
cxp pull   # gets everyone's insights
cxp push   # contributes their own
```

---

## 5. Pull Team Insights

```bash
cxp pull
```

Downloads all team insights to `~/.cache/contextpool/team-cache/<project>/team-insights.md`. Your agent searches these alongside your local `ContextPool/` automatically.

---

## Self-Hosted

Want to run your own sync server? Set `CXP_API_URL` to point at your server:

```bash
export CXP_API_URL=https://your-server.com
cxp auth <your-key>
```

The server API contract is simple and documented — any server implementing the endpoints works with the CLI. Contact us for the API spec.
