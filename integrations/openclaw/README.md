# Exponential for OpenClaw

[Exponential](https://exponential.at) is an open-source realtime issue tracker
with local coding agents. This plugin connects OpenClaw to Exponential's MCP
server: issues, boards, labels, statuses, comments, pull requests and coding
runs, plus a skill that tells the agent how to use them.

With OpenClaw's MCP Apps bridge on, two tools render Exponential's own views
in the dashboard instead of JSON:

- `exponential_issues_show`: the issue list, grouped by status like the board
  in Exponential. A row opens the issue (pills, description, comments). It is
  also an app entrypoint, so it opens without asking the model.
- `exponential_sessions_get`: a coding run's report, the same report its pull
  request carries.

## Install

```bash
openclaw plugins install clawhub:@exponential/openclaw-plugin
```

## Sign in

**API key (headless gateways).** Create a personal key in Exponential under
Settings → Security (it starts with `expu_`) and give it to the gateway:

```bash
export EXPONENTIAL_API_KEY=expu_...
```

**OAuth (interactive).** Replace the plugin's server entry with an OAuth one
and log in. The consent screen lets you grant everything or specific teams or
boards:

```bash
openclaw mcp add exponential --url https://app.exponential.at/api/mcp \
  --transport streamable-http --auth oauth
openclaw mcp login exponential
```

**Self-hosted Exponential.** Point the same entry at your instance:
`--url https://<your-instance>/api/mcp`.

## Turn on the views

```bash
openclaw config set mcp.apps.enabled true --strict-json
```

Restart the gateway. Behind a reverse proxy, give the apps sandbox its own
origin (`mcp.apps.sandboxOrigin`); see OpenClaw's
[MCP Apps docs](https://docs.openclaw.ai/cli/mcp/apps).

## Source

This folder lives in the Exponential monorepo
([`integrations/openclaw`](https://github.com/Niach/exponential/tree/master/integrations/openclaw)).
The views are built from `packages/mcp-apps` and served by every Exponential
instance at `ui://exponential/*`, so they work in any MCP Apps host. Licensed
under Apache-2.0.
