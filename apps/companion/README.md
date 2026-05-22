# @exp/companion

Long-lived agent companion daemon for Exponential. Watches issues assigned to
its agent user, runs a local coding agent (Claude Agent SDK or Codex SDK) in a
git worktree, opens a GitHub PR when the agent's patch passes tests, and can
send WhatsApp updates through a Baileys linked-device session.

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

- WhatsApp linking uses [`@whiskeysockets/baileys`](https://github.com/WhiskeySockets/Baileys).
  Request pairing from the Exponential workspace settings UI and scan the QR
  there.
- Baileys-based WhatsApp messaging is outside Meta's official Business API and
  is technically a ToS gray-zone. Acceptable for a single-user personal
  companion; do not bulk-message anyone but the configured `notifyJid`.
- GitHub PR creation uses your local `gh auth` token. No PAT required.
- Worktrees live under `~/.exponential-companion/worktrees/`.
- State (in-flight issues, ShapeStream offsets) lives in
  `~/.exponential-companion/state.db` (SQLite, WAL).
