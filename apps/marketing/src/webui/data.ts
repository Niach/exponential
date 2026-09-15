/* ─── Web-app demo fixtures — ONLY what the web chrome adds ───
   Issues, inbox items, reviews and issue bodies come from the single fixture
   universe in ../ide/data (dogfood: Exponential building Exponential). */
import { PROJECT, type Issue, type IssueStatus } from "../ide/data"

/* The issue the collab scene's widget report files onto the board
   (EXP-602) — mirrors Mara's support thread below: a fresh widget report,
   unassigned, labelled feedback. Injected into WebDemo, never in ISSUES. */
export const WIDGET_FILED_ISSUE: Issue = {
  id: `EXP-14`,
  title: `Screenshot upload never finishes`,
  status: `backlog`,
  priority: `none`,
  labels: [{ name: `feedback`, color: `#22c55e` }],
}

/* ─── Sidebar boards — colored icons ─── */

export type DemoProjectIcon = `code` | `kanban` | `megaphone`

export type DemoProject = {
  name: string
  slug: string
  color: string
  icon: DemoProjectIcon
}

/* The dogfood board carries a repository, so it draws the `ui-repository`
   glyph like every repo-backed board in the app. */
export const WEB_PROJECTS: DemoProject[] = [
  { name: PROJECT.name, slug: `exponential`, color: `#6366f1`, icon: `code` },
  { name: `Mobile Apps`, slug: `mobile-apps`, color: `#f97316`, icon: `kanban` },
  { name: `Feedback`, slug: `feedback`, color: `#22c55e`, icon: `megaphone` },
]

export const WEB_BOARD = WEB_PROJECTS[0]

/* Group order — the app lists groups by the status rows' contract
   displayOrder (backlog → started → completed), NOT by the IDE's
   activity-first order. */
export const WEB_GROUP_ORDER: IssueStatus[] = [
  `backlog`,
  `in_progress`,
  `in_review`,
  `done`,
]

/* The demo user (matches the assignee fixture DS in ide/data). The sidebar
   footer shows the FIRST name only (EXP-311). */
export const WEB_USER = {
  name: `Danny Strähhuber`,
  firstName: `Danny`,
  initials: `DS`,
  email: `danny@exponential.at`,
}

export const WEB_DEVICE = `Danny's MacBook Pro`

/* ─── Coding runs (EXP-870/874/877) ───
   Every live run of mine owns a WORK TAB, grouped by agent in the strip
   above the card; the Agent nav entry counts them. A run is the Run face of
   its issue's tab. */
export type DemoAgent = `claude` | `codex`

export type RunState = `working` | `review`

export type AgentSession = {
  id: string
  issueId: string
  agent: DemoAgent
  device: string
  state: RunState
  /* The Run face's `+N -M` diff label. */
  additions: number
  deletions: number
  /* The composer footer: model word + the context ring's percent. */
  model: string
  contextPercent: number
  started: string
}

export const AGENT_LABEL: Record<DemoAgent, string> = {
  claude: `Claude Code`,
  codex: `Codex`,
}

export const AGENT_SESSIONS: AgentSession[] = [
  {
    id: `run-8`,
    issueId: `EXP-8`,
    agent: `claude`,
    device: WEB_DEVICE,
    state: `working`,
    additions: 24,
    deletions: 6,
    model: `Fable`,
    contextPercent: 38,
    started: `started 12 min ago`,
  },
  {
    id: `run-11`,
    issueId: `EXP-11`,
    agent: `codex`,
    device: WEB_DEVICE,
    state: `review`,
    additions: 82,
    deletions: 14,
    model: `GPT-5.5`,
    contextPercent: 61,
    started: `started 2 hours ago`,
  },
]

export const AGENTS_RUNNING = AGENT_SESSIONS.length

export const sessionFor = (issueId: string): AgentSession | undefined =>
  AGENT_SESSIONS.find((s) => s.issueId === issueId)

/* The Agent page's folded "Recent" list (EXP-862: folded by default; EXP-886: was "Past"). */
export const PAST_RUNS: { identifier: string | null; title: string; byline: string }[] = [
  { identifier: `EXP-5`, title: `Side-by-side diff view`, byline: `${WEB_DEVICE} · yesterday` },
  { identifier: null, title: `Weekly standup digest`, byline: `${WEB_DEVICE} · 3 days ago` },
]

/* Launch suggestions above the Agent page composer (action-suggestions.ts). */
export const AGENT_SUGGESTIONS = [`Fix #`, `Review #`, `Find duplicate issues and link them`]

/* ─── The Run face transcript (agent-session.tsx rows) ───
   Narration = the agent's prose; `group` = a collapsed run of tool calls
   wearing the contract toolGroupSummary caption; `edit` = one settled edit
   row; `question` = the answerable AskUserQuestion card. Owned here rather
   than borrowed from ide/data so the web recreation stands on its own. */
export type RunRow =
  | { kind: `narration`; text: string }
  | { kind: `group`; caption: string; detail?: string }
  | { kind: `edit`; path: string; detail: string }
  | { kind: `user`; text: string }
  | { kind: `question`; text: string; options: { title: string; sub: string }[] }

export const RUN_FEED: Record<string, RunRow[]> = {
  [`EXP-8`]: [
    {
      kind: `narration`,
      text: `On it. I'll find where the viewer loses the stream first: a stale feed after a relay drop is usually a socket nobody re-opens.`,
    },
    { kind: `group`, caption: `Read 3 files · searched 2 times` },
    {
      kind: `narration`,
      text: `Confirmed: the viewer opens its socket once and never re-dials. On close it should back off, reconnect and replay from the last acked seq.`,
    },
    {
      kind: `edit`,
      path: `apps/web/src/lib/steer-session-store.ts`,
      detail: `reconnect with backoff`,
    },
    { kind: `group`, caption: `Ran 2 commands · edited 1 file` },
    {
      kind: `narration`,
      text: `Killing the relay mid-run now recovers in under 2s and the feed resumes where it stopped. Typecheck and the steering tests are clean.`,
    },
    {
      kind: `question`,
      text: `The backoff caps at 15s. Keep that, or make the cap a team setting?`,
      options: [
        { title: `Keep the 15s cap`, sub: `One less setting, and it matches the relay's own backoff` },
        { title: `Make it a team setting`, sub: `A settings row and a migration on top of this PR` },
      ],
    },
  ],
  [`EXP-11`]: [
    {
      kind: `narration`,
      text: `Adding j/k and arrow-key navigation to the board list, with Enter opening the focused issue.`,
    },
    { kind: `group`, caption: `Read 4 files · edited 3 files` },
    { kind: `edit`, path: `apps/web/src/components/issue-list.tsx`, detail: `roving focus` },
    { kind: `group`, caption: `Ran 3 commands` },
    {
      kind: `narration`,
      text: `Tests pass. Pushed exp/EXP-11 and opened PR #214, ready for review.`,
    },
  ],
}

/* ─── The diff face (session-diff-face.tsx) ─── */
export type DiffLine = { old: number | null; new: number | null; kind: `ctx` | `add` | `del`; text: string }

export type DiffFile = { path: string; additions: number; deletions: number; hunk?: string; lines?: DiffLine[] }

export const RUN_DIFF: DiffFile[] = [
  {
    path: `apps/web/src/lib/steer-session-store.ts`,
    additions: 18,
    deletions: 4,
    hunk: `@@ -88,9 +88,23 @@ export function createSteerSessionStore(`,
    lines: [
      { old: 88, new: 88, kind: `ctx`, text: `  const connect = () => {` },
      { old: 89, new: 89, kind: `ctx`, text: `    const ws = new WebSocket(ticketUrl(sessionId))` },
      { old: 90, new: null, kind: `del`, text: `    ws.onclose = () => setPhase({ kind: \`ended\` })` },
      { old: null, new: 90, kind: `add`, text: `    ws.onclose = () => {` },
      { old: null, new: 91, kind: `add`, text: `      if (closed) return` },
      { old: null, new: 92, kind: `add`, text: `      const delay = Math.min(15_000, 500 * 2 ** attempt++)` },
      { old: null, new: 93, kind: `add`, text: `      retry = setTimeout(connect, delay)` },
      { old: null, new: 94, kind: `add`, text: `    }` },
      { old: 91, new: 95, kind: `ctx`, text: `    ws.onopen = () => {` },
      { old: 92, new: null, kind: `del`, text: `      ws.send(hello())` },
      { old: null, new: 96, kind: `add`, text: `      attempt = 0` },
      { old: null, new: 97, kind: `add`, text: `      ws.send(hello({ resumeFrom: lastAckedSeq }))` },
      { old: 93, new: 98, kind: `ctx`, text: `    }` },
      { old: 94, new: 99, kind: `ctx`, text: `    socket = ws` },
    ],
  },
  { path: `apps/web/src/lib/steer-session-store.test.ts`, additions: 5, deletions: 0 },
  { path: `apps/steer-relay/src/room.ts`, additions: 1, deletions: 2 },
]

/* ─── Support (helpdesk) threads — server-only tables in the real app,
   so the demo carries its own conversation fixtures. Mirrors the real
   support-inbox.tsx shape (EXP-388): a thread carries its own title and the
   widget submission context (page URL / user agent / viewport), and an issue
   exists only once a member ESCALATES the ticket — un-escalated threads show
   the Escalate board picker in the details rail instead. ─── */

export type SupportMessage = {
  direction: `inbound` | `outbound`
  /* Outbound only: internal notes are never emailed to the reporter. */
  internal?: boolean
  author: string
  body: string
  time: string
}

export type SupportThread = {
  id: string
  reporterName: string
  reporterEmail: string
  title: string
  /* Set once a member escalated the ticket into an issue. */
  issueId?: string
  /* Widget submission context shown in the details rail. */
  context?: { pageUrl: string; userAgent: string; viewport: string }
  lastSeen: string
  resolved?: boolean
  unread?: boolean
  time: string
  messages: SupportMessage[]
}

export const SUPPORT_THREADS: SupportThread[] = [
  {
    id: `t-mara`,
    reporterName: `Mara Winkler`,
    reporterEmail: `mara@heliolabs.io`,
    title: `Screenshot upload never finishes`,
    context: {
      pageUrl: `https://app.heliolabs.io/reports`,
      userAgent: `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Safari/17.6`,
      viewport: `1728×1024`,
    },
    lastSeen: `12m ago`,
    unread: true,
    time: `12m`,
    messages: [
      {
        direction: `inbound`,
        author: `Mara Winkler`,
        body: `Hi, when I attach a screenshot to a bug report the upload spinner never finishes. Safari 17 on macOS.`,
        time: `1h`,
      },
      {
        direction: `outbound`,
        author: `Danny Strähhuber`,
        body: `Thanks Mara, I've reproduced it on Safari. The annotation layer is blocking the upload callback; fix is underway.`,
        time: `48m`,
      },
      {
        direction: `outbound`,
        internal: true,
        author: `Danny Strähhuber`,
        body: `Same root cause as EXP-13. The annotation flatten re-encode stalls on Safari WebP. Fix rides the next widget release.`,
        time: `45m`,
      },
      {
        direction: `inbound`,
        author: `Mara Winkler`,
        body: `Great, thanks for the quick response! Happy to test a build.`,
        time: `12m`,
      },
    ],
  },
  {
    id: `t-jonas`,
    reporterName: `Jonas Petersen`,
    reporterEmail: `jonas@fjordworks.no`,
    title: `Paste images from the clipboard?`,
    issueId: `EXP-12`,
    context: {
      pageUrl: `https://fjordworks.no/support`,
      userAgent: `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36`,
      viewport: `1920×1080`,
    },
    lastSeen: `3h ago`,
    time: `3h`,
    messages: [
      {
        direction: `inbound`,
        author: `Jonas Petersen`,
        body: `Is there a way to paste images straight from the clipboard into a report?`,
        time: `4h`,
      },
      {
        direction: `outbound`,
        author: `Danny Strähhuber`,
        body: `Not yet. Paste uploads are tracked as EXP-12, and I'll follow up here the moment it ships.`,
        time: `3h`,
      },
    ],
  },
  {
    id: `t-sofia`,
    reporterName: `Sofia Marino`,
    reporterEmail: `sofia@brightapps.co`,
    title: `Diff view clips on ultrawide`,
    issueId: `EXP-5`,
    lastSeen: `2d ago`,
    resolved: true,
    time: `2d`,
    messages: [
      {
        direction: `inbound`,
        author: `Sofia Marino`,
        body: `The side-by-side diff view clips the right pane on ultrawide monitors.`,
        time: `3d`,
      },
      {
        direction: `outbound`,
        author: `Danny Strähhuber`,
        body: `Fixed in last week's release. Thanks for the report!`,
        time: `2d`,
      },
    ],
  },
]

export const getThread = (id: string): SupportThread =>
  SUPPORT_THREADS.find((t) => t.id === id) ?? SUPPORT_THREADS[0]
