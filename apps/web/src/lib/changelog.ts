// In-app changelog (EXP-164). Entries power the dismissable "What's new" card
// in the sidebar footer and the detailed changelog sheet it opens.
//
// Authoring convention: every user-facing release prepends ONE entry at the
// HEAD of `CHANGELOG` with a fresh `id` — the card re-surfaces for a user
// whenever the head entry's id differs from the one they last dismissed
// (per-device, see `changelog-seen.ts`). Keep `summary` to a single short
// line (it renders truncated in the card) and `body` to a few GFM bullets.

export interface ChangelogEntry {
  // Stable slug, e.g. `2026-07-whats-new-card`. Never reuse an id — it is
  // the dismissal key.
  id: string
  // ISO date (display only).
  date: string
  title: string
  // One-line card preview.
  summary: string
  // GFM markdown, rendered read-only in the changelog sheet.
  body: string
}

// Newest first.
export const CHANGELOG: ChangelogEntry[] = [
  {
    id: `2026-07-multi-agent-resume`,
    date: `2026-07-20`,
    title: `Pick your coding agent, and resume where you left off`,
    summary: `Start coding sessions with Claude, Codex, or pi — and resume an earlier session instead of starting over.`,
    body: `- **Three agents** — the Start-coding dialog now offers Claude, Codex, and pi, each with its own model and effort picks; your defaults are saved per agent. Remote starts from your phone only offer the agents actually installed on the chosen desktop.
- **Resume coding** — reopen a finished or interrupted session and the agent picks up with its previous context instead of starting from scratch.
- **Guarded by default** — Claude runs now default to the guarded auto permission mode instead of skipping permissions; the IDE doctor needs claude ≥ 2.1.215 for it (run \`claude update\` if coding shows as blocked).
- **IDE polish** — long issue descriptions no longer get cut off in the IDE editor.
- **Smoother marketing film** — the exponential.at intro video now autoplays reliably.`,
  },
  {
    id: `2026-07-mobile-steering-review`,
    date: `2026-07-20`,
    title: `Answer your agent from anywhere, and sessions that wait for review`,
    summary: `Tap to answer agent questions from your phone, and coding sessions now show a "ready for review" state once the PR is open.`,
    body: `- **Steer from your phone** — when an agent asks a question, iOS, Android, and web now show it as a tappable card: pick an option (multi-select included) instead of typing keystrokes.
- **Ready for review** — a coding session no longer just vanishes when the PR opens; it moves to an in-review state across all clients so you can see what's waiting on you.
- **Better tabs on mobile** — My Work and Support switched to a cleaner segmented control on iOS and Android, and agent sessions open full-screen on the mobile web app.
- **Editing on phones** — the issue description editor keeps your cursor visible above the keyboard while you type.
- **Fresh marketing pages** — new agents, teamwork, and helpdesk sections plus clearer pricing at exponential.at.`,
  },
  {
    id: `2026-07-create-or-join-mobile-web`,
    date: `2026-07-19`,
    title: `Create or join a team, a merged inbox, and a mobile-friendly web app`,
    summary: `Pick your own first team, My Issues lives in the Inbox now, and the web app works properly on phones.`,
    body: `- **Create or join** — new accounts no longer get an auto-created personal team. On first launch you create a team (you own it) or join one by pasting an invite link; invites can now also be emailed directly.
- **One inbox** — My Issues merged into the Inbox as a tab on web and in the IDE, so notifications and your assigned work live on one page. Support gets an unread badge on every client.
- **Web on your phone** — a bottom tab bar, mobile-sized layouts, and detail pages that use the full screen.
- **Fixes** — desktop steering activity works again, Android keeps the helpdesk entry after partial syncs and refreshes repositories after board creation, and the IDE got a round of polish.`,
  },
  {
    id: `2026-07-teams-boards-helpdesk`,
    date: `2026-07-19`,
    title: `Boards, a simpler helpdesk, and a private-by-default product`,
    summary: `Projects are now boards, the helpdesk moved to one team-level inbox, and public boards are gone.`,
    body: `- **Projects are now boards** — same power, clearer name, everywhere: web, mobile, and the IDE.
- **Public boards are gone** — nothing in a team is readable from outside anymore. The feedback widget is the one way outsiders reach you, and it's now included on every plan (1 widget on Free, 3 on Pro).
- **One helpdesk per team** — flip a single switch under Settings → Feedback widget (Pro+) and every member shares the Support inbox. Tickets are standalone conversations with an email reply loop, and any ticket can be escalated into an issue on a board with one click.
- **Simpler board creation** — no more board types. A board is a board; connect a repository when you want to code on it.`,
  },
  {
    id: `2026-07-whats-new-card`,
    date: `2026-07-17`,
    title: `A changelog, mobile coding, and support inboxes`,
    summary: `Start coding from your phone, helpdesk widget mode, and this changelog.`,
    body: `- **What's new lives here now** — each release drops a note in this changelog. Dismiss the card and it stays quiet until the next release; reopen it anytime from the user menu.
- **Start coding from mobile** — the iOS and Android apps can now remotely start coding sessions on your desktop, including batch runs.
- **Support inboxes** — the feedback widget gained a helpdesk mode: support tickets can file into a separate private inbox, away from your feedback board.
- **Steering polish** — stale steering sessions are cleaned up reliably, and the web agent dock and review views got a refresh.`,
  },
]

export function latestChangelogEntry(): ChangelogEntry | null {
  return CHANGELOG[0] ?? null
}
