import threadFixture from "@exp/domain-contract/fixtures/session-thread.json"
import listFixture from "@exp/domain-contract/fixtures/threads-list.json"
import {
  buildSessionThread,
  partitionThreads,
  sessionCard,
  type ThreadsListGroup,
} from "@exp/domain-contract/session-tree"
import { SEMANTIC_ICONS } from "@exp/icons"
import {
  cn,
  conceptIcon,
  getDeviceIcon,
  MENU_SURFACE_CLASS,
  MenuPanel,
  SessionCard,
  ThreadStripGroupMenu,
  ThreadTabs,
  ThreadTabsAddButton,
  ThreadsList,
  type ThreadStripIssue,
  type ThreadStripOpen,
  type ThreadTreeRow,
} from "@exp/ui"

import type { ReactNode } from "react"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2: the THREAD's parts, the REAL `@exp/ui` components
// (`session-thread-view.tsx`, `work-card.tsx`) painted from the contract's
// own models: the header (title + ONE tab strip, #EXP-1433: "+" ·
// Conversation | the one issue chip OR ONE group tab | Stories, nothing
// scrolls; the group opens THE Menu over every issue and session), THE session card at every state (`session-card.json`) and
// the Threads list (`partitionThreads`, `threads-list.json`) in the Reviews
// list language. The web app binds them to the synced rows
// (`apps/web/src/routes/t/$teamSlug/threads/`).

type ThreadInput = Parameters<typeof buildSessionThread>[0]
type ListInput = Parameters<typeof partitionThreads>[0]

const noop = (): void => {}
/** EXP-1318: the specimen rows' root device (the Devices page's glyph). */
const DEVICE = { icon: getDeviceIcon({ kind: `desktop` }), name: `Alex's MacBook Pro` }
const PrivateIcon = conceptIcon(`ui-private`)

const STATUS = [
  { icon: SEMANTIC_ICONS[`status-in-progress`], colorClass: `text-amber-400` },
  { icon: SEMANTIC_ICONS[`status-backlog`], colorClass: `text-muted-foreground` },
] as const

const THREAD = buildSessionThread(threadFixture.cases[0]!.input as unknown as ThreadInput)
const PRIVATE = buildSessionThread(threadFixture.cases[1]!.input as unknown as ThreadInput)
const LIST = partitionThreads(listFixture.cases[0]!.input as unknown as ListInput)

const NOW = `2026-10-13T10:00:00.000Z`
const BASE = {
  id: `s`,
  teamId: `t`,
  parentId: null,
  rootId: `s`,
  userId: `u`,
  status: `running`,
  busy: false,
  needsInput: false,
  blocked: null,
  shared: true,
  title: null,
  guide: null,
  startedAt: `2026-10-13T09:47:26.000Z`,
  endedAt: null,
  createdAt: `2026-10-13T09:47:00.000Z`,
  updatedAt: `2026-10-13T09:59:00.000Z`,
}

// EXP-1341: the 2nd arg = the session's resolved `sessionTitle` (its
// stored title, `#IDENT title`, the first prompt).
const CARDS = [
  sessionCard({ ...BASE, busy: true }, `#APP-12 Login fails on Safari`, NOW),
  sessionCard(
    { ...BASE, title: `Add width and height to cart thumbnails`, busy: true },
    `Add width and height to cart thumbnails`,
    NOW
  ),
  sessionCard({ ...BASE, title: `Profile the launch path`, needsInput: true }, `Profile the launch path`, NOW),
  sessionCard({ ...BASE, title: `Review the fix` }, `Review the fix`, NOW),
  sessionCard({ ...BASE, status: `queued`, startedAt: null }, `Deploy the staging build`, NOW),
  sessionCard(
    { ...BASE, needsInput: true, blocked: { kind: `rate_limit`, window: `5h` } },
    `Why does the cart badge lag a tap behind?`,
    NOW
  ),
  sessionCard({ ...BASE, status: `ended`, endedAt: `2026-10-13T09:51:10.000Z` }, `Bump the CLI floor to 0.14.72`, NOW),
]

/** EXP-1334: the folded case, found by NAME (a case inserted upstream once
 *  shifted the index and broke the whole styleguide suite). */
const FOLDED_CASE = threadFixture.cases.find((fixtureCase) =>
  fixtureCase.name.startsWith(`EXP-1334: the root has no chip or card`)
)!
const issueTitle = (issueId: string) =>
  [...threadFixture.cases[0]!.input.issues, ...FOLDED_CASE.input.issues, ...EXTRA_ISSUES].find(
    (issue) => issue.id === issueId
  )?.title ?? issueId

/** #EXP-1433: the strip's two modes: ONE issue (chips) and the folded
 *  case's issues + sessions plus two more issues (the group). */
const FOLDED = buildSessionThread(FOLDED_CASE.input as unknown as ThreadInput)
const EXTRA_ISSUES = [
  { id: `i-5`, identifier: `EXP-25`, title: `Context ring` },
  { id: `i-6`, identifier: `EXP-26`, title: `Model picker` },
]
const GROUP_TABS: typeof FOLDED.tabs = [
  ...FOLDED.tabs.filter((tab) => tab.kind !== `session`),
  ...EXTRA_ISSUES.map((issue) => ({
    kind: `issue` as const,
    issueId: issue.id,
    identifier: issue.identifier,
    nested: [],
  })),
  ...FOLDED.tabs.filter((tab) => tab.kind === `session`),
]
const ONE_ISSUE_TABS: typeof FOLDED.tabs = [
  ...FOLDED.tabs.filter((tab) => tab.kind === `conversation` || tab.kind === `add`),
  { kind: `issue` as const, issueId: `i-5`, identifier: `EXP-25`, nested: [] },
]

type IssueTab = Extract<(typeof THREAD.tabs)[number], { kind: `issue` }>
type SessionTab = Extract<(typeof THREAD.tabs)[number], { kind: `session` }>

const statusOf = (index: number) => STATUS[index % STATUS.length]!

/** #EXP-1433: the strip's issues = every linked issue (a chip's sub-issues
 *  after it), the way the app binds them. */
function stripIssues(tabs: typeof THREAD.tabs): ThreadStripIssue[] {
  return tabs
    .filter((tab): tab is IssueTab => tab.kind === `issue`)
    .flatMap((tab) => [{ issueId: tab.issueId, identifier: tab.identifier }, ...tab.nested])
    .map((ref, index) => ({
      issueId: ref.issueId,
      identifier: ref.identifier,
      title: issueTitle(ref.issueId),
      status: statusOf(index),
    }))
}

/** The sessions under the dispatcher as THE tree rows (depth → parent: the
 *  nearest shallower row before it). */
function stripSessions(tabs: typeof THREAD.tabs): ThreadTreeRow[] {
  return tabs
    .filter((tab): tab is SessionTab => tab.kind === `session`)
    .flatMap((tab) => {
      const rows: ThreadTreeRow[] = [
        { sessionId: tab.sessionId, parentId: null, name: tab.name, agent: `claude`, markState: tab.markState },
      ]
      const stack = [tab.sessionId]
      for (const row of tab.nested) {
        stack.length = row.depth
        rows.push({
          sessionId: row.sessionId,
          parentId: stack[row.depth - 1] ?? tab.sessionId,
          name: row.name,
          agent: `claude`,
          markState: row.markState,
          stateWord: STATE_WORD[row.markState],
          stateTone: row.markState === `needs_you` ? `amber` : undefined,
        })
        stack.push(row.sessionId)
      }
      return rows
    })
}

const STATE_WORD: Record<string, string | undefined> = {
  working: `Working`,
  needs_you: `Needs you`,
  ended: `Ended`,
}

function Strip({
  tabs,
  open = null,
  width,
  pager = false,
}: {
  tabs: typeof THREAD.tabs
  open?: ThreadStripOpen
  width?: string
  pager?: boolean
}) {
  return (
    <div style={width ? { width } : undefined}>
      <ThreadTabs
        variant={pager ? `pager` : `bar`}
        conversation={{ label: threadFixture.copy.conversation, active: open === null, onSelect: noop }}
        issues={stripIssues(tabs)}
        sessions={stripSessions(tabs)}
        open={open}
        add={<ThreadTabsAddButton />}
        stories={{ label: `Stories`, active: false, dot: true, onSelect: noop }}
      />
    </div>
  )
}

function Header({ view }: { view: typeof THREAD }) {
  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <div className="flex min-w-0 items-center gap-2">
        <h1 className="truncate text-2xl font-semibold tracking-tight">{view.title}</h1>
        {view.private && <PrivateIcon className="size-4 shrink-0 text-muted-foreground" />}
      </div>
      <Strip tabs={view.tabs} width="48rem" />
    </div>
  )
}

const Caption = ({ children }: { children: ReactNode }) => (
  <p className="text-xs text-muted-foreground">{children}</p>
)

/** #EXP-1433: one issue keeps its chip; anything more = ONE group tab (at
 *  rest: the glyph + the count; while an item of it is open: active, its
 *  label); the phone pager; the group's Menu and the "+" menu, painted open. */
function StripModes() {
  const issues = stripIssues(GROUP_TABS)
  const sessions = stripSessions(GROUP_TABS)
  return (
    <div className="flex w-[48rem] flex-col gap-3" data-testid="thread-strip-modes">
      <Caption>One issue, no session: its chip stays</Caption>
      <Strip tabs={ONE_ISSUE_TABS} width="48rem" />
      <Caption>More: ONE group tab, the count muted</Caption>
      <Strip tabs={GROUP_TABS} width="48rem" />
      <Caption>A session open from the group: the tab is active and names it</Caption>
      <Strip tabs={GROUP_TABS} open={{ kind: `session`, id: `g2` }} width="48rem" />
      <Caption>An issue open from the group</Caption>
      <Strip tabs={GROUP_TABS} open={{ kind: `issue`, id: `i-3` }} width="48rem" />
      <Caption>Phone pager: Conversation and Stories icon-only; the one issue = its identifier</Caption>
      <Strip tabs={ONE_ISSUE_TABS} width="24rem" pager />
      <Strip tabs={GROUP_TABS} open={{ kind: `session`, id: `g2` }} width="24rem" pager />
      <div className="mt-2 flex flex-wrap items-start gap-8">
        <div className="flex flex-col gap-2">
          <div className={cn(MENU_SURFACE_CLASS, `w-80 p-1`)}>
            <ThreadStripGroupMenu
              issues={issues}
              sessions={sessions}
              open={{ kind: `session`, id: `g2` }}
              onOpenIssue={noop}
              onOpenSession={noop}
            />
          </div>
          <Caption>The group's Menu: issues, a separator, the session tree (phones: the bottom sheet)</Caption>
        </div>
        <div className="flex flex-col gap-2">
          <MenuPanel
            density="pointer"
            entries={[
              { kind: `submenu`, id: `link`, label: `Link an issue`, icon: conceptIcon(`nav-search`), entries: [] },
              { kind: `item`, id: `new`, label: `New issue`, icon: conceptIcon(`ui-add`), onSelect: noop },
            ]}
          />
          <Caption>The "+" (left of Conversation)</Caption>
        </div>
      </div>
    </div>
  )
}

/** The specimen's PRs: `b` sits in an open stack (on another thread's open
 *  PR), `d` is ready to merge, `c` merged (the violet lead, no word); `a`
 *  waits on a question. The words come from the REAL `threadWordOf`. */
const TEAM_PRS = [
  { id: `pb`, state: `open`, basePullRequestId: `px` },
  { id: `px`, state: `open`, basePullRequestId: null },
  { id: `pd`, state: `open`, basePullRequestId: null },
  { id: `pc`, state: `merged`, basePullRequestId: null },
]
const PRS_BY_ROOT = new Map([
  [`b`, [TEAM_PRS[0]!]],
  [`d`, [TEAM_PRS[2]!]],
  [`c`, [TEAM_PRS[3]!]],
])
const CLAUDE = new Map(
  LIST.flatMap((group) => group.rows.map((row) => [row.sessionId, `claude`] as const))
)
const AGENTS = [
  { agentId: `ag1`, teamId: `t`, name: `Triage`, icon: `bug`, color: `#a78bfa`, busy: false, needsYou: true, assistant: false },
  { agentId: `ag2`, teamId: `t`, name: `Designer`, icon: `palette`, color: `#f472b6`, busy: false, needsYou: false, assistant: false },
]
const deviceGlyph = (deviceId: string | null | undefined) => (deviceId ? DEVICE : null)

/** THE app list (`@exp/ui` `ThreadsList`, EXP-1339: same component, no drift). */
function ThreadsListSpecimen({
  groups = LIST,
  agents = true,
}: {
  groups?: readonly ThreadsListGroup[]
  agents?: boolean
}) {
  return (
    <div className="max-w-3xl">
      <ThreadsList
        groups={groups}
        cliByRoot={CLAUDE}
        prsByRoot={PRS_BY_ROOT}
        teamPrs={TEAM_PRS}
        agents={agents ? AGENTS : undefined}
        deviceGlyph={deviceGlyph}
        onOpenThread={noop}
        onOpenAgent={noop}
      />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `thread-parts`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Thread`,
  blurb: `A thread IS a root session. Header: the title (THE \`sessionTitle()\` chain: its stored title (an explicit name or the agent's own), \`#IDENT title\`, the first prompt; nobody renames it; a lock when private) and ONE tab strip that never scrolls (#EXP-1433): "+" (THE Menu: Link an issue › the search list, New issue) and Conversation pinned left, Stories pinned right (S-3: no Guide tab); between them \`threadStripMode\`: at most ONE linked issue and no session = that issue's chip (THE strip chip, \`StripChip\`: status glyph, identifier, title), anything more = ONE group tab (the \`session-tree\` glyph, the count muted) opening THE Menu (phones: the bottom sheet): the issue rows (THE search list's rows), a separator, then THE session tree rows (SessionRow small + the guides, every session under the dispatcher); a pick opens it in place and the open row carries the check. While one of them is open the group tab is active and reads its identifier or name instead of the count. No "+N", no nested chip menus. Phones' pager: Conversation · the one issue | the item open from the group · Stories. The conversation IS the dispatcher: no card of its own. No captions, no right sidebar; while the thread's PR is open the header's trailing slot carries Merge (\`Merge PR\` | \`Merge stack\`) and the \`…\` on every form factor, phones included; phones use the strip as the pager. THE session card = the Work card's header row: the state icon in its tone, the name (the same \`sessionTitle()\` chain, never a number), the muted state word (amber while it needs you: Needs you, Usage limit, the natives' semantic yellow ×4), trailing the elapsed and a spinner while working; it opens that session. The Threads page (and the phones' Threads tab) = Needs you · Running · Recent bands (filled, no count) over flat gapless rows, no dividers: the lead (spinner while busy, amber badge when it needs you, else the PR glyph), the title, ONE quiet word (\`ready to merge\`, \`stack\`, \`question\`; a merged thread shows only the violet lead, no word, and no row carries a lock, EXP-1339), then the root's device glyph (its name on hover, EXP-1318); the page and this specimen render THE same \`ThreadsList\`; phones put the Agents band on top. Models = \`buildSessionThread()\`, \`sessionCard()\`, \`partitionThreads()\`, \`sessionTitle()\` (fixtures \`session-thread.json\`, \`session-card.json\`, \`threads-list.json\`, \`session-title.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadTabs / SessionCard`,
      file: `packages/ui/src/session-thread-view.tsx`,
      note: `ThreadsList: packages/ui/src/threads-list.tsx; card: packages/ui/src/work-card.tsx; pages: apps/web/src/routes/t/$teamSlug/threads/`,
    },
    desktop: {
      state: `ok`,
      symbol: `ThreadScreenView`,
      file: `apps/desktop/crates/ui/src/thread_screen.rs`,
      note: `model: crates/domain/src/session_tree.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/ios/Exponential/UI/Thread/ThreadScreen.swift`,
      note: `model: ExpCore SessionTree.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ThreadScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/thread/ThreadScreen.kt`,
      note: `model: domain/SessionTree.kt`,
    },
  },
  island: () => (
    <div className="flex flex-col gap-10 text-sm">
      <Header view={THREAD} />
      <Header view={PRIVATE} />
      <StripModes />
      <div className="flex max-w-xl flex-col gap-2">
        {CARDS.map((card) => (
          <SessionCard key={card.name} card={card} onOpen={noop} />
        ))}
      </div>
      <ThreadsListSpecimen />
    </div>
  ),
}
