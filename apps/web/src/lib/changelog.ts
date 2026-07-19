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
