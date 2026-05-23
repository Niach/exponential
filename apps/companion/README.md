# @exp/companion

Long-lived agent companion daemon for Exponential. Watches issues assigned to
its agent user, runs a local coding agent (Claude Agent SDK or Codex SDK) in a
git worktree, and opens a GitHub PR when the agent's patch lands. Owner-side
notifications (plan ready, questions asked, PR opened, agent errors) arrive on
mobile via the existing FCM push pipeline — every event the daemon cares about
is also a comment on the issue, and comment creation already fans out pushes.

## Quick start

```bash
# 1. In Exponential workspace settings, add an agent member and copy the
#    generated Linux install command.

# 2. Or manually claim a setup token from a checkout:
bun apps/companion/src/cli.ts setup --server https://app.exponential.at --setup-token expc_...

# 3. Run it in the foreground:
bun --filter @exp/companion start

# 4. Or install as a user-level systemd service on Linux:
bun --filter @exp/companion start -- install-service
```

## Operational notes

- GitHub PR creation uses your local `gh auth` token. No PAT required.
- Worktrees live under `~/.exponential-companion/worktrees/`.
- State (in-flight issues, ShapeStream offsets, poll cursors) lives in
  `~/.exponential-companion/state.db` (SQLite, WAL).
