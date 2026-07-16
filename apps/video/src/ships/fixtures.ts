// The ONE shared fixture world every scene draws from (storyboard §2, corrected
// against the real-app reference screenshots in apps/video/ref/).

export type IssueStatus = "backlog" | "todo" | "in_progress" | "done"
export type Priority = "none" | "urgent" | "high" | "medium" | "low"

export type BoardRow = {
  id: string // "EXP-142"
  title: string
  status: IssueStatus
  priority: Priority
  label?: { name: string; dot: string }
  assignee?: string // avatar initials; undefined = no avatar
  due?: string // "Jul 15"
}

export const IDENTITY = {
  workspace: "Exponential",
  project: "Exponential",
  projectColor: "#6366f1",
  prefix: "EXP",
  repo: "niach/exponential",
  defaultBranch: "main",
  user: "Alex Carter",
  initials: "AC",
  device: "MacBook Pro",
  host: "Alexs-MacBook-Pro.local", // steer header: "Live · <host>"
  runConfig: "Dev Server",
} as const

export const LABELS = {
  bug: { name: "bug", dot: "#ef4444" },
  desktop: { name: "desktop", dot: "#3b82f6" },
  web: { name: "web", dot: "#6366f1" },
  widget: { name: "widget", dot: "#22c55e" },
} as const

// Board at story start (S3). Status changes over the film are applied by scenes
// via overrides — this array is the base truth.
export const BOARD: BoardRow[] = [
  { id: "EXP-139", title: "Release progress bar drifts when issues are cancelled", status: "in_progress", priority: "medium", label: LABELS.bug, assignee: "AC", due: "Jul 14" },
  { id: "EXP-142", title: "Live-steer terminal reconnect", status: "todo", priority: "high", label: LABELS.bug, assignee: "AC", due: "Jul 15" },
  { id: "EXP-143", title: "Confirm-merge double-fires on slow connections", status: "todo", priority: "medium", label: LABELS.desktop },
  { id: "EXP-144", title: "Branch flow graph clips long release names", status: "todo", priority: "low", label: LABELS.desktop, assignee: "AC" },
  { id: "EXP-146", title: "Offline board cache for the web app", status: "backlog", priority: "none", label: LABELS.web },
  { id: "EXP-147", title: "Widget screenshot annotations on Safari", status: "backlog", priority: "none", label: LABELS.widget },
  { id: "EXP-138", title: "Exit-code badges on orchestrator tabs", status: "done", priority: "medium", assignee: "AC" },
  { id: "EXP-140", title: "Ship desktop auto-update banner", status: "done", priority: "low", assignee: "AC" },
]

export const HERO = {
  id: "EXP-142",
  title: "Live-steer terminal reconnect",
  descriptionParas: [
    "When the steer relay drops a WebSocket mid-session, the terminal view goes stale and never recovers. Reconnect with exponential backoff and resume the scrollback buffer.",
    "Repro: restart the relay while a session is streaming — the viewer freezes until a full reload.",
  ],
  switcher: "3 / 8", // "N / M" pager next to the chevrons
  activity: [
    { actor: "Alex Carter", text: "added label bug" },
    { actor: "Alex Carter", text: "added this to release v0.12" },
  ],
  branch: "exp/EXP-142",
  worktree: ".worktrees/EXP-142",
  pr: 214,
  sessionTab: "Fix live-steer terminal reconnect", // real app titles session tabs by task
} as const

// ── Claude session events — REAL Claude Code CLI grammar (see ref/desktop-claude-session-dock.png):
//    tool:   "● Bash(bun run typecheck)" then "  ⎿ <result summary>" (muted)
//    prose:  "● <assistant text>" (white dot, wraps)
//    spinner:"✳ Vibing… (2m 41s · ↓ 12.3k tokens)" — ✳ + verb yellow, parens muted; ticks live
//    The input box at the bottom: "❯ " + blinking block cursor between hairline rules, and the
//    status row "▶▶ bypass permissions on (shift+tab to cycle) · esc to interrupt · ⏎ for agents".
export type SessionEvent =
  | { kind: "tool"; tool: string; args?: string; result?: string; extra?: string[] }
  | { kind: "prose"; text: string }
  | { kind: "spinner"; verb: string } // rendered with live elapsed/tokens counters
  | { kind: "flash"; text: string } // green ✓-style landing line (PR opened etc.)

// Hero session (dock tab "Fix live-steer terminal reconnect"). S1/S2 flash-forward shows
// the tail (from index FLASH_FORWARD_FROM); S6 plays it whole.
export const HERO_SESSION: SessionEvent[] = [
  { kind: "tool", tool: "Read", args: "apps/web/src/components/agent-session.tsx", result: "Read 212 lines" },
  { kind: "prose", text: "The steer socket never retries after a relay drop. Adding reconnect with exponential backoff and scrollback resume:" },
  { kind: "tool", tool: "Update", args: "apps/web/src/components/agent-session.tsx", result: "Added 29 lines, removed 11 lines" },
  { kind: "tool", tool: "Write", args: "apps/web/src/lib/steer-backoff.ts", result: "Created file with 46 lines" },
  { kind: "tool", tool: "Bash", args: "bun run typecheck", result: "0 errors" },
  { kind: "tool", tool: "Bash", args: "bun test steer-backoff", result: "5 pass · 0 fail" },
  { kind: "spinner", verb: "Vibing" },
  { kind: "tool", tool: "Bash", args: "git push -u origin exp/EXP-142", result: "To github.com:niach/exponential.git" },
  { kind: "tool", tool: "mcp__exponential__exponential_pr_open" },
  { kind: "flash", text: "Opened PR #214 — Live-steer terminal reconnect" },
]
export const FLASH_FORWARD_FROM = 2 // S1 crop starts at the Update event

// Phone steer feed (iOS activity view — see ref/ios-steer-activity.png).
// Tool rows: crossed-tools glyph + bold name + muted mono summary. Narration: sparkle + bubble.
export type SteerItem =
  | { kind: "tool"; name: string; summary?: string }
  | { kind: "narration"; text: string }
export const PHONE_FEED: SteerItem[] = [
  { kind: "tool", name: "Read", summary: "agent-session.tsx" },
  { kind: "narration", text: "The steer socket never retries after a relay drop. Adding reconnect with exponential backoff and scrollback resume:" },
  { kind: "tool", name: "Update", summary: "apps/web/src/components/agent-session…" },
  { kind: "tool", name: "Write", summary: "apps/web/src/lib/steer-backoff.ts" },
  { kind: "tool", name: "Bash", summary: "Typecheck the web app" },
  { kind: "tool", name: "Bash", summary: "Run the backoff unit tests" },
  { kind: "tool", name: "Bash", summary: "Push the branch" },
  { kind: "tool", name: "mcp__exponential__exponential_pr_open" },
  { kind: "narration", text: "Done — reconnect with exponential backoff, scrollback resume, tests green. Opened PR #214." },
]
export const PHONE_DIFFSTAT = { add: 51, del: 16 } // pinned "Latest changes" strip

// ── EXP-142 diff (Changes tab) ────────────────────────────────────────────────
export const DIFF_HEADER = {
  branch: "exp/EXP-142",
  pr: "PR #214",
  stats: { files: 5, add: 120, del: 34 },
}
export const DIFF_FILES = [
  { status: "M", path: "apps/web/src/components/agent-session.tsx", selected: true },
  { status: "M", path: "apps/web/src/lib/steer.ts" },
  { status: "A", path: "apps/web/src/lib/steer-backoff.ts" },
  { status: "M", path: "apps/web/src/hooks/use-agent-stream.ts" },
  { status: "A", path: "apps/web/src/lib/steer-backoff.test.ts" },
] as const

// Side-by-side hunk. old/new line numbers; type: ctx | del | add.
export type DiffRow = { t: "ctx" | "del" | "add" | "hunk"; text: string; old?: number; new?: number }
export const DIFF_ROWS: DiffRow[] = [
  { t: "hunk", text: "@@ -48,11 +48,29 @@ export function AgentSessionView({ sessionId }: AgentSessionProps)" },
  { t: "ctx", text: "  const terminal = useTerminal()", old: 48, new: 48 },
  { t: "ctx", text: "  const relayUrl = useRelayUrl(sessionId)", old: 49, new: 49 },
  { t: "ctx", text: "", old: 50, new: 50 },
  { t: "del", text: "  const socket = connect(relayUrl)", old: 51 },
  { t: "add", text: "  const socket = connectWithBackoff(relayUrl, {", new: 51 },
  { t: "add", text: "    base: 500,", new: 52 },
  { t: "add", text: "    factor: 2,", new: 53 },
  { t: "add", text: "    maxDelay: 15_000,", new: 54 },
  { t: "add", text: "    onResume: (buffered) => terminal.write(buffered),", new: 55 },
  { t: "add", text: "  })", new: 56 },
  { t: "ctx", text: "", old: 52, new: 57 },
  { t: "ctx", text: "  useEffect(() => {", old: 53, new: 58 },
  { t: "ctx", text: "    socket.on(`chunk`, (data) => terminal.write(data))", old: 54, new: 59 },
  { t: "add", text: "    socket.on(`resume`, () => setStale(false))", new: 60 },
  { t: "add", text: "    socket.on(`drop`, () => setStale(true))", new: 61 },
  { t: "ctx", text: "    return () => socket.close()", old: 55, new: 62 },
  { t: "ctx", text: "  }, [socket, terminal])", old: 56, new: 63 },
  { t: "hunk", text: "@@ -88,9 +95,21 @@ export function AgentSessionView({ sessionId }: AgentSessionProps)" },
  { t: "ctx", text: "  const composer = useSteerComposer(sessionId)", old: 88, new: 95 },
  { t: "ctx", text: "", old: 89, new: 96 },
  { t: "del", text: "  if (!socket.connected) {", old: 90 },
  { t: "del", text: "    return <SessionStale />", old: 91 },
  { t: "add", text: "  if (stale) {", new: 97 },
  { t: "add", text: "    return (", new: 98 },
  { t: "add", text: "      <SessionStale", new: 99 },
  { t: "add", text: "        retryIn={socket.nextRetryMs}", new: 100 },
  { t: "add", text: "        onRetryNow={() => socket.reconnect()}", new: 101 },
  { t: "add", text: "      />", new: 102 },
  { t: "add", text: "    )", new: 103 },
  { t: "ctx", text: "  }", old: 92, new: 104 },
  { t: "ctx", text: "", old: 93, new: 105 },
  { t: "ctx", text: "  return (", old: 94, new: 106 },
  { t: "ctx", text: "    <div className={`flex h-full flex-col`}>", old: 95, new: 107 },
  { t: "ctx", text: "      <SessionFeed items={feed} presence={presence} />", old: 96, new: 108 },
  { t: "ctx", text: "      <SteerComposer composer={composer} />", old: 97, new: 109 },
]

// ── Release v0.12 ────────────────────────────────────────────────────────────
export const RELEASE = {
  name: "v0.12",
  target: "Jul 18",
  doneAtS9: 3,
  total: 8,
  pr: 219,
  integrationBranch: "exp/rel-v0-12",
  // checked rows in the release dialog (open issues the orchestrator will run)
  dialogIssues: [
    { id: "EXP-139", title: "Release progress bar drifts when issues are cancelled" },
    { id: "EXP-141", title: "Public board hides pr_url for anonymous viewers" },
    { id: "EXP-143", title: "Confirm-merge double-fires on slow connections" },
    { id: "EXP-144", title: "Branch flow graph clips long release names" },
    { id: "EXP-145", title: "Orchestrator wave logs collapse into one tab" },
  ],
  doneIssues: ["EXP-138", "EXP-140", "EXP-142"],
  wave1: ["EXP-139", "EXP-141", "EXP-143"],
  wave2: ["EXP-144", "EXP-145"],
} as const

// Orchestrator session (dock tab "Release v0.12"). Same CLI grammar.
export const ORCH_SESSION: SessionEvent[] = [
  { kind: "tool", tool: "Bash", args: "git checkout -b exp/rel-v0-12 && git push -u origin exp/rel-v0-12", result: "Created integration branch exp/rel-v0-12" },
  { kind: "prose", text: "Planning dependency waves: 2 waves · 5 issues. Wave 1: EXP-139, EXP-141, EXP-143 — 3 subagents." },
  { kind: "tool", tool: "Agent", args: "EXP-139 · release progress bar drift", result: "Done — merged into exp/rel-v0-12" },
  { kind: "tool", tool: "Agent", args: "EXP-141 · public board pr_url leak", result: "Done — merged into exp/rel-v0-12" },
  { kind: "tool", tool: "Agent", args: "EXP-143 · confirm-merge double-fire", result: "Done — merged into exp/rel-v0-12" },
  { kind: "prose", text: "Wave 2: EXP-144, EXP-145." },
  { kind: "tool", tool: "Agent", args: "EXP-144 · flow graph clipping", result: "Done — merged into exp/rel-v0-12" },
  { kind: "tool", tool: "Agent", args: "EXP-145 · wave log tabs", result: "Done — merged into exp/rel-v0-12" },
  { kind: "prose", text: "Reviewing the combined diff — 23 files +612 −188. Looks clean." },
  { kind: "tool", tool: "mcp__exponential__exponential_release_pr_open" },
  { kind: "flash", text: "Opened PR #219 — Release v0.12" },
]

// ── Branch flow graph lanes (S10) — modeled on the real source-control tree
//    (ref/desktop-source-control-diff.png: indented rows, connector rails, worktree tags)
//    but staged big in the center pane with animated draws.
export type Lane = { name: string; depth: 0 | 1 | 2; wave?: 1 | 2; worktree?: boolean }
export const LANES: Lane[] = [
  { name: "main", depth: 0 },
  { name: "exp/rel-v0-12", depth: 1, worktree: true },
  { name: "exp/EXP-139", depth: 2, wave: 1, worktree: true },
  { name: "exp/EXP-141", depth: 2, wave: 1, worktree: true },
  { name: "exp/EXP-143", depth: 2, wave: 1, worktree: true },
  { name: "exp/EXP-144", depth: 2, wave: 2, worktree: true },
  { name: "exp/EXP-145", depth: 2, wave: 2, worktree: true },
]

// ── Reviews row (S8) ─────────────────────────────────────────────────────────
export const REVIEW_ROW = {
  id: "EXP-142",
  title: "Live-steer terminal reconnect",
  sub: "#214 · exp/EXP-142",
} as const

// ── Overlay copy (verbatim, storyboard §3) ───────────────────────────────────
export const COPY = {
  hook1: "Your issue tracker.",
  hook2: "Writing code.",
  s2: "Exponential — desktop IDE",
  s3: "Pick an issue.",
  s5: "Model. Effort. Plan mode.",
  s6a: "Claude runs in the dock.",
  s6b: "Steer it from your phone.",
  s7: "Review it in place.",
  s8: "Merge. Done.",
  s9: "Or ship a whole release.",
  s10: "One orchestrator. Waves of agents.",
  s11: "Shipped.",
  tagline: "Issue tracking that ships code.",
  url: "exponential.at",
} as const
