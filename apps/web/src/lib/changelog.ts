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
    id: `2026-07-webmcp-tools`,
    date: `2026-07-23`,
    title: `Browser AI agents can now work your boards`,
    summary: `The web app speaks WebMCP: in-browser AI agents can read your boards, file and update issues, comment, and navigate for you.`,
    body: `- **WebMCP support** — the web app now registers page tools via the emerging WebMCP browser standard, so AI agents running in your browser (Chrome's built-in agent, MCP browser extensions) can work with what's on screen.
- **Read and act as you** — agents can look up your boards, list and search issues, read full issue threads, check your inbox, create and update issues, comment, manage labels and subscriptions, and jump to any view — always as your signed-in user, only in teams you're a member of.
- **Nothing new is exposed** — tools reuse the exact same permissions and APIs as clicking the UI yourself.`,
  },
  {
    id: `2026-07-mobile-detail-live-support`,
    date: `2026-07-23`,
    title: `Redesigned issue view on mobile, live support chat`,
    summary: `A reworked issue screen on iOS and Android, start coding from your phone, and live support conversations.`,
    body: `- **Reworked issue detail on mobile** — iOS and Android get a cleaner issue screen: a bottom action bar, tidier property and label pickers, due-date and assignee sheets, and a collapsible activity timeline.
- **Start coding from iOS** — kick off a coding session on a connected desktop straight from the iOS app.
- **Live support chat** — when a reporter has their support thread open, replies appear live and we hold back the email notification while they're watching.
- **Resizable web terminal** — drag to resize the agent panel on the web, and it remembers your height.
- **Desktop IDE tabs** — closing, middle-click, and right-click on editor tabs now feel like a real IDE.
- **More reliable email** — hardened transactional email with automatic bounce and complaint handling.`,
  },
  {
    id: `2026-07-reliability-and-security`,
    date: `2026-07-22`,
    title: `Faster sync, tighter security, keyboard-driven search`,
    summary: `A reliability, performance, and security pass across every client.`,
    body: `- **Snappier real-time sync** — reworked how boards, issues, and notifications sync so large teams stay fast and connections recover cleanly under load.
- **Keyboard-driven search** — global issue search (⌘F on the web) now supports arrow keys to move and Enter to open a result.
- **Clearer billing errors** — checkout and billing-portal buttons now surface a message when a request fails instead of doing nothing.
- **Security hardening** — signing out now fully ends your session on the server, and we closed a case where an image link could carry your session token to another site.
- **Desktop IDE polish** — fixes to issue-title editing and description layout.`,
  },
  {
    id: `2026-07-feedback-widget-origin`,
    date: `2026-07-21`,
    title: `See what came from your feedback widget`,
    summary: `Issues filed through the embeddable feedback widget now carry a clear "Feedback widget" label.`,
    body: `- **Feedback widget label** — issues that arrive through your embeddable widget now show a "Feedback widget" origin pill on the issue, on web, iOS, Android, and the desktop IDE.
- **No more hidden bot users** — feedback used to be filed by a synthetic per-widget user that could show up in member lists; those are gone. Widget-filed issues simply have no sender, and everything (member lists, seat counts, account deletion) treats them cleanly.`,
  },
  {
    id: `2026-07-invites-from-the-web`,
    date: `2026-07-21`,
    title: `Invites now live on the web`,
    summary: `Team invites are created here on the web — invite links still open and join right inside the mobile apps.`,
    body: `- **Invite from the web** — the iOS and Android apps no longer create invites. Invite teammates under Settings → Members, and the link you share still opens and joins directly in the app.
- **Clearer storage errors on mobile** — when a team is out of attachment storage, image uploads now say so and offer a retry instead of retrying forever.
- **Privacy policy refresh** — it now covers Sign in with Apple, the legal bases and your rights under the GDPR, international transfers, and the 48-hour board trash.`,
  },
  {
    id: `2026-07-agents-need-you`,
    date: `2026-07-21`,
    title: `See when an agent needs you`,
    summary: `Coding sessions waiting on your input now show an attention badge on every client.`,
    body: `- **Needs-input badge** — when a coding agent parks on a plan approval or a question, the session is flagged everywhere: the Agents tab and nav badges on web, iOS, Android, and the IDE light up until you answer.
- **Start-coding polish** — reworked agent and model pickers in the Start-coding dialog on web, the IDE, and the mobile sheets.
- **Smoother steering on iOS** — answering agent questions and plan approvals in the session view got a cleaner flow.
- **Marketing + pricing refresh** — updated agents section and plan pages at exponential.at.`,
  },
  {
    id: `2026-07-widget-domains-mobile-agents`,
    date: `2026-07-20`,
    title: `Widget domain allowlists required, and a nicer mobile Start-coding sheet`,
    summary: `Feedback widgets now always require a domain allowlist, and the mobile agent picker got a visual refresh.`,
    body: `- **Widget keys are locked to your domains** — the "allow any website" mode is gone: every widget config needs at least one allowed domain, and keys without one stop serving until you add it in Settings → Feedback widget.
- **Mobile Start-coding refresh** — agent icons, per-agent options, and a cleaner repository picker on iOS and Android.
- **IDE fixes** — window sizing polish and coding-flow refinements.`,
  },
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
