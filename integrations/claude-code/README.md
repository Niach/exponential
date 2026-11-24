# Exponential for Claude Code

[Exponential](https://exponential.at) is an open-source (Apache-2.0) realtime issue tracker with coding agents. This plugin brings the issue you are working on into Claude Code itself: in the terminal and in the desktop app's Code tab.

## What you get

- **Issue pane**: the issue your branch names (`exp/EXP-12`, `feature/exp-12-export`) with its status, its pull request and its live sessions. `/exp-issues` lists your open issues; a row opens its issue. `/exp-issue EXP-12` shows any issue.
- **Status line**: the issue and where its pull request stands, in the words Exponential's own header uses: `EXP-12 · PR #40 · Checks running`, `Queued · 2nd`, `Merged`.
- **Toasts**: a new comment, a mention, a reporter's reply or a session that waits for you. Each one shows once, even with several Claude Code sessions open.
- **`/exp-start EXP-12`**: puts the request to start a coding agent on that issue into the prompt box; Enter sends it.
- **The Exponential connector and its skill**: the same MCP server as the Claude plugin (OAuth sign-in), so Claude can read and change your team's work.

## Install

From a checkout of the [Exponential repository](https://github.com/Niach/exponential):

```
claude plugin marketplace add ./integrations/claude-code
claude plugin install exponential-code@exponential-code
```

If you already use the `exponential` plugin from Claude's directory, keep one of the two in Claude Code: both add the same connector.

## Sign in

The first call opens Exponential's OAuth sign-in in your browser. On the consent screen you choose which teams, or which boards of a team, Claude may reach. Revoke the grant at any time in Exponential under Settings.

**Without a browser** (a remote shell, CI): add the server yourself under the name `exponential` with a personal key from Settings → Security; the mods use it:

```
claude mcp add --transport http --scope user exponential https://app.exponential.at/api/mcp --header "Authorization: Bearer expu_..."
```

## Self-hosting

Set the plugin's **Exponential instance** option to your own origin (`/plugin` → exponential-code → Configure, or `claude plugin configure exponential-code`), for example `https://exponential.example.com`.

## What the plugin sends and runs

- Network: only the MCP connection to your Exponential instance (`<instance>/api/mcp`). The mods read through that same connection: the current issue every 30 seconds while a branch names one, your unread notifications every 60 seconds.
- On your machine: `git branch --show-current` in the session's folder, to learn which issue the branch names. Nothing else is run, read or written, except the id of the newest notification you were shown (Claude Code's own plugin store).
- A hook sees the session's tool calls to notice a pull request opened through Exponential; it changes none of them.
- No telemetry or analytics of any kind.

## Privacy, terms and support

- Privacy policy: https://exponential.at/privacy
- Terms of service: https://exponential.at/terms
- Support: support@exponential.at, or open an issue in the repository.

## Development

`hooks/kit/`, `.mcp.json` and `skills/exponential/SKILL.md` are generated from `packages/plugin-kit` (`bun run plugin-kit:generate`); edit the sources there.

```
claude plugin validate integrations/claude-code --strict
claude plugin test integrations/claude-code
claude --plugin-dir integrations/claude-code
```

## License

Apache-2.0, see [LICENSE](LICENSE).
