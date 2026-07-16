/* ─── IDE playground fixture data — dogfood: Exponential building Exponential ─── */

export type IssueStatus = `backlog` | `todo` | `in_progress` | `done`
export type IssuePriority = `none` | `urgent` | `high` | `medium` | `low`
export type FilterTab = `all` | `active` | `backlog`

export type Assignee = { initials: string; name: string }
export type Label = { name: string; color: string }

export type Issue = {
  id: string
  title: string
  status: IssueStatus
  priority: IssuePriority
  assignee?: Assignee
  labels?: Label[]
  due?: string
}

export const DS: Assignee = { initials: `DS`, name: `Danny Strähhuber` }

export const PROJECT = { name: `Exponential`, color: `#4f46e5` }

export const ISSUES: Issue[] = [
  {
    id: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    status: `in_progress`,
    priority: `high`,
    assignee: DS,
    due: `Jul 9`,
  },
  {
    id: `EXP-11`,
    title: `Issue board keyboard navigation`,
    status: `todo`,
    priority: `medium`,
    due: `Jul 15`,
  },
  {
    id: `EXP-12`,
    title: `Attachment paste uploads`,
    status: `todo`,
    priority: `none`,
  },
  {
    id: `EXP-9`,
    title: `Issue board keyboard polish`,
    status: `backlog`,
    priority: `none`,
  },
  {
    id: `EXP-13`,
    title: `Widget screenshot annotations`,
    status: `backlog`,
    priority: `none`,
    labels: [{ name: `feedback`, color: `#22c55e` }],
  },
  {
    id: `EXP-5`,
    title: `Side-by-side diff view`,
    status: `done`,
    priority: `medium`,
    assignee: DS,
  },
  {
    id: `EXP-7`,
    title: `Terminal exit-code badges`,
    status: `done`,
    priority: `low`,
  },
]

export const getIssue = (id: string): Issue =>
  ISSUES.find((i) => i.id === id) ?? ISSUES[0]

/* ─── Releases — workspace-level bundles of issues (EXP-56) ─── */

export type Release = {
  id: string
  name: string
  target?: string
  shippedAt?: string
  issueIds: string[]
}

export const RELEASES: Release[] = [
  {
    id: `rel-steer`,
    name: `Live steer v2`,
    target: `Jul 15`,
    issueIds: [`EXP-8`, `EXP-11`, `EXP-12`, `EXP-5`],
  },
  {
    id: `rel-terminal`,
    name: `Terminal polish`,
    shippedAt: `Jul 2`,
    issueIds: [`EXP-7`],
  },
]

export const getRelease = (id: string): Release =>
  RELEASES.find((r) => r.id === id) ?? RELEASES[0]

/* An issue ships in at most ONE release. */
export const releaseFor = (issueId: string): Release | undefined =>
  RELEASES.find((r) => r.issueIds.includes(issueId))

/* Progress derives client-side, like the real apps. */
export const releaseProgress = (release: Release): { done: number; total: number } => {
  const issues = release.issueIds.map(getIssue)
  return { done: issues.filter((i) => i.status === `done`).length, total: issues.length }
}

export const releaseSubline = (release: Release): string => {
  const { done, total } = releaseProgress(release)
  const when = release.shippedAt
    ? `Shipped ${release.shippedAt}`
    : release.target
      ? `Target ${release.target}`
      : undefined
  const progress = `${done} of ${total} done`
  return when ? `${when} · ${progress}` : progress
}

export const GROUP_ORDER: { status: IssueStatus; label: string }[] = [
  { status: `in_progress`, label: `In Progress` },
  { status: `todo`, label: `Todo` },
  { status: `backlog`, label: `Backlog` },
  { status: `done`, label: `Done` },
]

export const FILTER_STATUSES: Record<FilterTab, IssueStatus[]> = {
  all: [`in_progress`, `todo`, `backlog`, `done`],
  active: [`in_progress`, `todo`],
  backlog: [`backlog`],
}

export const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: `Backlog`,
  todo: `Todo`,
  in_progress: `In Progress`,
  done: `Done`,
}

export const PRIORITY_LABEL: Record<IssuePriority, string> = {
  none: `No priority`,
  urgent: `Urgent`,
  high: `High`,
  medium: `Medium`,
  low: `Low`,
}

/* ─── Inbox fixtures — single Linear-style activity stream ─── */

export type InboxType =
  | `issue_assigned`
  | `issue_comment`
  | `issue_status_changed`
  | `pr_opened`
  | `pr_merged`

export type InboxItem = {
  id: string
  type: InboxType
  issueId: string
  title: string
  sentence: string
  time: string
  unread: boolean
}

export const INBOX_ITEMS: InboxItem[] = [
  {
    id: `n1`,
    type: `pr_opened`,
    issueId: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    sentence: `Claude opened pull request #214 for EXP-8`,
    time: `2m`,
    unread: true,
  },
  {
    id: `n2`,
    type: `issue_comment`,
    issueId: `EXP-12`,
    title: `Attachment paste uploads`,
    sentence: `Danny commented: paste should reuse the drag-drop path`,
    time: `26m`,
    unread: true,
  },
  {
    id: `n3`,
    type: `issue_assigned`,
    issueId: `EXP-11`,
    title: `Issue board keyboard navigation`,
    sentence: `Danny assigned you EXP-11`,
    time: `1h`,
    unread: true,
  },
  {
    id: `n4`,
    type: `pr_merged`,
    issueId: `EXP-5`,
    title: `Side-by-side diff view`,
    sentence: `Danny merged the pull request for EXP-5`,
    time: `3h`,
    unread: false,
  },
  {
    id: `n5`,
    type: `issue_status_changed`,
    issueId: `EXP-7`,
    title: `Terminal exit-code badges`,
    sentence: `Danny changed the status to Done`,
    time: `5h`,
    unread: false,
  },
]

/* ─── My Issues — subset of ISSUES assigned to the signed-in user ─── */

export const MY_ISSUE_IDS: string[] = [`EXP-8`, `EXP-5`]

/* ─── Reviews — issues with an open PR (one issue = one PR = one branch) ─── */

export type Review = {
  issueId: string
  identifier: string
  title: string
  branch: string
  prNumber: number
}

export const REVIEWS: Review[] = [
  {
    issueId: `EXP-8`,
    identifier: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    branch: `exp/EXP-8`,
    prNumber: 214,
  },
  {
    issueId: `EXP-11`,
    identifier: `EXP-11`,
    title: `Issue board keyboard navigation`,
    branch: `exp/EXP-11`,
    prNumber: 209,
  },
]

/* ─── Issue detail bodies (rendered GFM, statically) ───
   `ref` renders a same-workspace #issue pill, `mention` an @member pill —
   both are plain text in the markdown source, pills only at render time. */

export type Inline = { t: string; code?: boolean; ref?: boolean; mention?: boolean }

export const ISSUE_BODY: Record<string, Inline[][]> = {
  [`EXP-8`]: [
    [
      {
        t: `When the steer relay drops a WebSocket mid-session, the activity feed goes stale and never recovers. Reconnect with exponential backoff and resume the `,
      },
      { t: `pty`, code: true },
      { t: ` stream from the last acked offset.` },
    ],
    [
      {
        t: `Repro: kill the relay while a Claude session is streaming — the viewer freezes until a full page reload.`,
      },
    ],
    [
      { t: `Same reconnect contract as ` },
      { t: `EXP-5`, ref: true },
      { t: ` — ping ` },
      { t: `Danny Strähhuber`, mention: true },
      { t: ` once the relay patch lands.` },
    ],
  ],
}

export type ActivityItem =
  | { kind: `comment`; author: string; initials: string; time: string; body: string }
  | { kind: `event`; text: string; time: string }

export const ISSUE_ACTIVITY: Record<string, ActivityItem[]> = {
  [`EXP-8`]: [
    {
      kind: `comment`,
      author: `Danny Strähhuber`,
      initials: `DS`,
      time: `3 hours ago`,
      body: `Backoff should cap at 15s — the relay load balancer kills idle sockets after 60s anyway.`,
    },
    {
      kind: `event`,
      text: `Danny Strähhuber changed status to In Progress`,
      time: `2 hours ago`,
    },
  ],
}

/* ─── Git fixtures ─── */

/* Branch-flow lanes: default branch → exp/… lanes joined to issues by
   branch name, with a status indicator, ↑↓ counts and worktree tags —
   matches the real Source Control panel's semantic lanes. */
export type LaneIndicator = `none` | `progress` | `open` | `merged`

export type Lane = {
  branch: string
  label?: string
  indent: 0 | 1
  indicator: LaneIndicator
  ahead?: number
  behind?: number
  current?: boolean
  worktree?: boolean
}

export const LANES: Lane[] = [
  { branch: `master`, indent: 0, indicator: `none`, current: true },
  {
    branch: `exp/EXP-8`,
    label: `EXP-8 · Live-steer terminal reconnect`,
    indent: 1,
    indicator: `progress`,
    ahead: 2,
    worktree: true,
  },
  {
    branch: `exp/EXP-11`,
    label: `EXP-11 · Issue board keyboard navigation`,
    indent: 1,
    indicator: `open`,
    ahead: 1,
  },
  {
    branch: `exp/EXP-5`,
    label: `EXP-5 · Side-by-side diff view`,
    indent: 1,
    indicator: `merged`,
  },
]

export type Commit = { subject: string; meta: string }

export const COMMITS: Commit[] = [
  { subject: `feat(desktop): JetBrains-style IDE shell`, meta: `niach · 3 hours ago` },
  { subject: `fix(ios): show compose button only inside a project`, meta: `niach · 5 hours ago` },
  { subject: `fix(mobile): Android issue-open crash`, meta: `niach · 9 hours ago` },
  { subject: `feat!: masterplan v5 — per-seat billing`, meta: `niach · 11 hours ago` },
]

export type GitLetter = `M` | `A` | `D` | `R`
export type Change = { path: string; status: GitLetter }

export const CHANGES: Change[] = [
  { path: `apps/web/src/components/steer-terminal.tsx`, status: `M` },
]

/* ─── Diff fixture: steer-terminal.tsx +24 −6, one hunk ─── */

export type DiffCell = { n: number; text: string; kind: `ctx` | `add` | `del` } | null
export type DiffRow = { l: DiffCell; r: DiffCell }

export const DIFF_FILE = {
  path: `apps/web/src/components/steer-terminal.tsx`,
  add: 24,
  del: 6,
}

export const DIFF_HUNK = `@@ -42,10 +42,28 @@ export function SteerTerminal({ sessionId }: SteerTerminalProps)`

const OLD_REMOVED = [
  `  useEffect(() => {`,
  `    const ws = new WebSocket(steerUrl(sessionId))`,
  `    ws.onmessage = (e) => term.write(decode(e.data))`,
  `    socket.current = ws`,
  `    return () => ws.close()`,
  `  }, [sessionId])`,
]

const NEW_ADDED = [
  `  const retries = useRef(0)`,
  `  const reconnectTimer = useRef<ReturnType<typeof setTimeout>>()`,
  ``,
  `  const connect = useCallback(() => {`,
  `    const ws = new WebSocket(steerUrl(sessionId))`,
  `    ws.onmessage = (e) => term.write(decode(e.data))`,
  `    ws.onopen = () => {`,
  `      retries.current = 0`,
  `    }`,
  `    ws.onclose = () => {`,
  `      const delay = Math.min(1_000 * 2 ** retries.current, 15_000)`,
  `      retries.current += 1`,
  `      reconnectTimer.current = setTimeout(connect, delay)`,
  `    }`,
  `    socket.current = ws`,
  `  }, [sessionId])`,
  ``,
  `  useEffect(() => {`,
  `    connect()`,
  `    return () => {`,
  `      clearTimeout(reconnectTimer.current)`,
  `      socket.current?.close()`,
  `    }`,
  `  }, [connect])`,
]

const buildDiffRows = (): DiffRow[] => {
  const rows: DiffRow[] = [
    {
      l: { n: 42, text: `  const socket = useRef<WebSocket | null>(null)`, kind: `ctx` },
      r: { n: 42, text: `  const socket = useRef<WebSocket | null>(null)`, kind: `ctx` },
    },
    { l: { n: 43, text: ``, kind: `ctx` }, r: { n: 43, text: ``, kind: `ctx` } },
  ]
  NEW_ADDED.forEach((text, i) => {
    rows.push({
      l: i < OLD_REMOVED.length ? { n: 44 + i, text: OLD_REMOVED[i], kind: `del` } : null,
      r: { n: 44 + i, text, kind: `add` },
    })
  })
  rows.push({ l: { n: 50, text: ``, kind: `ctx` }, r: { n: 68, text: ``, kind: `ctx` } })
  rows.push({
    l: { n: 51, text: `  return <TerminalView ref={mount} onData={handleInput} />`, kind: `ctx` },
    r: { n: 69, text: `  return <TerminalView ref={mount} onData={handleInput} />`, kind: `ctx` },
  })
  return rows
}

export const DIFF_ROWS: DiffRow[] = buildDiffRows()

/* ─── File tree ─── */

export type FileNode = {
  name: string
  path: string
  children?: FileNode[]
  git?: GitLetter
  dim?: boolean
}

type RawNode = {
  name: string
  children?: RawNode[]
  git?: GitLetter
  dim?: boolean
}

const d = (name: string, children: RawNode[]): RawNode => ({ name, children })
const f = (name: string, extra?: { git?: GitLetter; dim?: boolean }): RawNode => ({
  name,
  ...extra,
})

const attachPaths = (nodes: RawNode[], base: string): FileNode[] =>
  nodes.map((n) => {
    const path = base ? `${base}/${n.name}` : n.name
    return {
      name: n.name,
      path,
      git: n.git,
      dim: n.dim,
      children: n.children ? attachPaths(n.children, path) : undefined,
    }
  })

export const FILE_TREE: FileNode[] = attachPaths(
  [
    d(`.github`, [d(`workflows`, [f(`build-desktop.yml`), f(`build-issues-web.yml`)])]),
    d(`apps`, [
      d(`android`, [f(`build.gradle.kts`), f(`settings.gradle.kts`)]),
      d(`desktop`, [d(`crates`, [f(`Cargo.toml`)]), f(`Cargo.toml`)]),
      d(`ios`, [f(`Project.swift`), f(`Tuist.swift`)]),
      d(`web`, [
        d(`src`, [
          d(`components`, [f(`issue-list.tsx`), f(`steer-terminal.tsx`, { git: `M` })]),
          d(`routes`, [f(`index.tsx`)]),
          f(`styles.css`),
        ]),
        f(`package.json`),
        f(`vite.config.ts`),
      ]),
    ]),
    d(`packages`, [
      d(`db-schema`, [f(`package.json`)]),
      d(`design-tokens`, [f(`tokens.json`), f(`package.json`)]),
      d(`widget`, [f(`package.json`)]),
    ]),
    f(`Caddyfile`, { dim: true }),
    f(`docker-compose.yaml`),
    f(`package.json`),
    f(`README.md`),
  ],
  ``,
)

/* ─── Read-only code tab: root package.json ─── */

export const PACKAGE_JSON = `{
  "name": "exponential",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "dev": "bun run --filter @exp/web dev",
    "build": "bun run build:widget && bun run build:web",
    "typecheck": "bun run --filter @exp/web typecheck",
    "migrate": "cd apps/web && drizzle-kit migrate",
    "test": "bun run --filter @exp/web test"
  },
  "packageManager": "bun@1.2.19"
}`

/* ─── "Start coding" scripted agent session (~8 lines), per issue ─── */

export type ScriptLineKind = `ok` | `cmd` | `claude`
export type ScriptLine = { kind: ScriptLineKind; text: string }

/* Issues with an open PR fixture keep their number; anyone else gets a
   plausible one derived from the issue number. */
const prNumberFor = (issue: Issue): number =>
  REVIEWS.find((r) => r.issueId === issue.id)?.prNumber ??
  200 + Number(issue.id.split(`-`)[1] ?? `0`)

/* EXP-8 keeps its bespoke plan line (it matches the diff fixture); every
   other issue plays the same canned change so the terminal script and the
   Changes-tab diff stay consistent. */
const PLAN_LINES: Record<string, string> = {
  [`EXP-8`]: `Plan: reconnect with exponential backoff, resume stream`,
}

export const codingScriptFor = (issue: Issue): ScriptLine[] => [
  { kind: `ok`, text: `Created worktree .worktrees/${issue.id} on branch exp/${issue.id}` },
  { kind: `ok`, text: `Launched Claude on ${issue.id}` },
  { kind: `claude`, text: `Reading issue ${issue.id} — ${issue.title}` },
  { kind: `claude`, text: PLAN_LINES[issue.id] ?? `Plan: implement the change, verify, open a PR` },
  { kind: `claude`, text: `Edited apps/web/src/components/steer-terminal.tsx (+24 -6)` },
  { kind: `cmd`, text: `git push -u origin exp/${issue.id}` },
  { kind: `claude`, text: `Opened PR #${prNumberFor(issue)} — ${issue.title}` },
  { kind: `ok`, text: `Session finished · 1 file changed` },
]

/* ─── Release orchestrator run — ONE session plans waves, merges issues
   back into the integration branch, opens the ONE release PR ─── */

export const releaseCodingScriptFor = (release: Release): ScriptLine[] => {
  const slug = release.name.toLowerCase().replace(/[^a-z0-9]+/g, `-`)
  const open = release.issueIds.filter((id) => getIssue(id).status !== `done`)
  return [
    {
      kind: `ok`,
      text: `Created integration branch exp/rel-${slug} · ${open.length} issues in scope`,
    },
    { kind: `claude`, text: `Planning dependency waves across ${open.length} issues` },
    { kind: `claude`, text: `Wave 1 — ${open.slice(0, 2).join(`, `)} in parallel worktrees` },
    ...open.map(
      (id): ScriptLine => ({
        kind: `claude`,
        text: `Merged exp/${id} into exp/rel-${slug} — PR #${prNumberFor(getIssue(id))} targets the integration branch`,
      }),
    ),
    { kind: `claude`, text: `Reviewing the combined diff` },
    { kind: `cmd`, text: `git push -u origin exp/rel-${slug}` },
    { kind: `claude`, text: `Opened release PR #221 — ${release.name}` },
    { kind: `ok`, text: `Release run finished · ${open.length} issues merged` },
  ]
}

export const SHELL_TAB_TITLE = `~/E/r/N/exponential`
export const claudeTabTitle = (issueId: string): string => `claude · ${issueId}`
export const releaseTabTitle = (name: string): string => `claude · release ${name}`
