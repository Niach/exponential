/**
 * #EXP-1732 (session-screen.json BODIES: the Chat tab = the ROOT's OWN
 * session view, for every kind): the recorded journals the stand-in desktop
 * (`screenshots:desktop`, lib/enet-feed-stub.ts) holds as STORED REPLAYS for
 * the three roots the captures open on their Chat tab, so none of them
 * photographs "Waiting for activity" / "No live transcript":
 *
 *   DEMO_CHAT_ID                  the cold-start chat (`chat-screen`)
 *   DEMO_AGENT_CONVERSATION_ID    the Triage conversation (`agent-channel`)
 *   DEMO_APP_EDITOR_SESSION_ID    the Working hours editor session (`app`)
 *
 * Every viewer (web, desktop, iOS, Android) reads a session's raw work over
 * eNet from the device running it, so these feed `bun run shots` on every
 * platform. The entries are the steered FEED's readable shape (`tool` rows by
 * name; screenshot-desktop.ts stamps the wire ids, `toolKind` and the
 * `tool_update` settle); `at` = epoch ms, aligned with the seeded messages
 * (the tool-posted cards, `thread_started` and `surface`, interleave by time).
 */
import {
  DEMO_AGENT_CHANNEL_TIMES,
  DEMO_AGENT_CONVERSATION_ID,
  DEMO_AGENT_REQUEST,
  DEMO_APP_CHAT_TIMES,
  DEMO_APP_EDITOR_SESSION_ID,
  DEMO_CHAT_ID,
} from "../screenshot-demo"

export type StoredEntry = Record<string, unknown> & { kind: string; at: number }

const MINUTE = 60_000

/** `at` a pinned instant plus `offsetMs`. */
const pinned = (at: Date, offsetMs = 0) => at.getTime() + offsetMs

/** The cold-start chat's root: the dispatcher's own work (its prompt, the
 *  issues it read, the session it started, the summary). The seed stamps its
 *  messages `minutesAgo(...)` and the reclock keeps those ages, so these are
 *  stamped off `now` the same way. */
function coldStartChat(now: number): StoredEntry[] {
  const ago = (minutes: number) => now - minutes * MINUTE
  return [
    {
      kind: `user_message`,
      text: `Cold start is at 1.4s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of the critical path.`,
      at: ago(60),
    },
    {
      kind: `narration`,
      text: `I'll read the linked issues first, then hand the profiling to a session on your MacBook so the launch path is measured before anything changes.`,
      at: ago(59),
    },
    { kind: `tool`, name: `mcp__exponential__exponential_issues_get`, detail: `APP-5`, at: ago(58) },
    { kind: `tool`, name: `mcp__exponential__exponential_issues_get`, detail: `APP-10`, at: ago(58) },
    { kind: `tool`, name: `Grep`, detail: `subscribeShapes( · 6 matches across 3 files`, at: ago(55) },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_sessions_send`,
      detail: `Profile the cold start and defer the sync bootstrap`,
      at: ago(52),
    },
    {
      kind: `narration`,
      text: `Deferred the shape subscribe until after the first frame and added a board snapshot cache that paints the last board before sync finishes: 740 ms on the CI device, under the 800 ms target. Filed FB-3 for the editor's highlighting bundle.`,
      at: ago(11),
    },
  ]
}

/** The demo user's conversation with the Triage agent (DEMO_AGENT_CHANNEL_TIMES). */
function triageConversation(): StoredEntry[] {
  const { report, chatStarted, request, reply } = DEMO_AGENT_CHANNEL_TIMES
  return [
    {
      kind: `user_message`,
      text: `Check Product Feedback for new widget reports.`,
      at: pinned(report, -2 * MINUTE),
    },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_issues_list`,
      detail: `Product Feedback · 2 new`,
      at: pinned(report, -100_000),
    },
    { kind: `tool`, name: `mcp__exponential__exponential_issues_get`, detail: `FB-8`, at: pinned(report, -80_000) },
    { kind: `tool`, name: `mcp__exponential__exponential_issues_get`, detail: `FB-9`, at: pinned(report, -70_000) },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_issues_update`,
      detail: `FB-8 · duplicate of FB-3`,
      at: pinned(report, -30_000),
    },
    {
      kind: `narration`,
      text: `Two new widget reports on Product Feedback. FB-8 is a duplicate of FB-3, merged it. FB-9 needs code, so I started a chat:`,
      at: pinned(report),
    },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_sessions_send`,
      detail: `Sort today's widget reports`,
      at: pinned(chatStarted, -5_000),
    },
    { kind: `user_message`, text: DEMO_AGENT_REQUEST, at: pinned(request) },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_issues_list`,
      detail: `search: android · 3 issues`,
      at: pinned(request, 10_000),
    },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_issues_labels_add`,
      detail: `FB-9 · android`,
      at: pinned(request, 20_000),
    },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_issues_labels_add`,
      detail: `APP-5 · android`,
      at: pinned(request, 24_000),
    },
    {
      kind: `tool`,
      name: `mcp__exponential__exponential_issues_labels_add`,
      detail: `APP-16 · android`,
      at: pinned(request, 28_000),
    },
    { kind: `narration`, text: `Done, 3 issues labelled android.`, at: pinned(reply) },
  ]
}

/** The Working hours editor session: one exchange per version (DEMO_APP_CHAT_TIMES). */
function appEditor(): StoredEntry[] {
  const t = DEMO_APP_CHAT_TIMES
  const exchange = (
    ask: string,
    asked: Date,
    tools: [string, string][],
    answer: string,
    answered: Date
  ): StoredEntry[] => [
    { kind: `user_message`, text: ask, at: pinned(asked) },
    ...tools.map(([name, detail], index) => ({
      kind: `tool`,
      name,
      detail,
      at: pinned(asked, (index + 1) * 12_000),
    })),
    { kind: `narration`, text: answer, at: pinned(answered) },
  ]
  return [
    ...exchange(
      `Replace my hours spreadsheet: every day of the month, how many hours I worked and what I did. Mine only, with the month's total.`,
      t.request,
      [
        [`mcp__exponential__exponential_apps_get`, `Working hours`],
        [`mcp__exponential__exponential_apps_upsert`, `Working hours · month screen`],
        [`mcp__exponential__exponential_apps_publish`, `Working hours · v1`],
      ],
      `Published v1: a month view bound to your own records (kind entry), a Log hours form that writes them, the total at the top.`,
      t.first
    ),
    ...exchange(
      `Add a monthly target and show what is left. I want to change the target in the app.`,
      t.change,
      [
        [`mcp__exponential__exponential_apps_upsert`, `Working hours · target tiles`],
        [`mcp__exponential__exponential_apps_publish`, `Working hours · v2`],
      ],
      `v2: a settings record keeps the target; the tiles show the target and what is left, Save target writes it.`,
      t.second
    ),
    ...exchange(
      `Let me switch to previous months, and show the days as a small trend.`,
      t.polish,
      [
        [`mcp__exponential__exponential_apps_upsert`, `Working hours · month switcher`],
        [`mcp__exponential__exponential_apps_publish`, `Working hours · v3`],
      ],
      `v3: previous and next re-bind the month through bind, the total tile carries the per-day sparkline, an entry opens for editing.`,
      t.third
    ),
    ...exchange(
      `Let me look at a teammate's month too, and suggest my open issues when I log a day.`,
      t.team,
      [
        [`mcp__exponential__exponential_apps_upsert`, `Working hours · member picker`],
        [`mcp__exponential__exponential_apps_publish`, `Working hours · v4`],
      ],
      `v4: a member picker from exp:members re-binds the month for that person; the issues field suggests your assigned issues from exp:issues.`,
      t.fourth
    ),
    ...exchange(
      `When I log a day on an issue, let me post it there as a comment.`,
      t.post,
      [
        [`mcp__exponential__exponential_apps_upsert`, `Working hours · Post to the issue`],
        [`mcp__exponential__exponential_apps_publish`, `Working hours · v5`],
      ],
      `v5: Post to the issue calls exponential.comments.create with the hours and the note; allow it once on the app's settings page and it posts without asking.`,
      t.fifth
    ),
  ]
}

/** Every recorded journal, by session id (the stub's `stored` entries). */
export function demoStoredJournals(now: number): Record<string, StoredEntry[]> {
  return {
    [DEMO_CHAT_ID]: coldStartChat(now),
    [DEMO_AGENT_CONVERSATION_ID]: triageConversation(),
    [DEMO_APP_EDITOR_SESSION_ID]: appEditor(),
  }
}
