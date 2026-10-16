import threadFixture from "@exp/domain-contract/fixtures/session-thread.json"
import listFixture from "@exp/domain-contract/fixtures/threads-list.json"
import {
  buildSessionThread,
  partitionThreadStrip,
  partitionThreads,
  sessionCard,
  type ThreadsListGroup,
} from "@exp/domain-contract/session-tree"
import { SEMANTIC_ICONS } from "@exp/icons"
import {
  AgentRunMark,
  AgentWorkingMark,
  BoardGlyph,
  cn,
  conceptIcon,
  getDeviceIcon,
  GlassSectionHeader,
  MENU_SURFACE_CLASS,
  MenuPanel,
  SessionCard,
  SessionRow,
  ThreadChipSubtree,
  ThreadIssueTab,
  ThreadIssueTree,
  ThreadSessionTab,
  ThreadSessionTree,
  ThreadTabs,
  ThreadTabsAddButton,
  ThreadsListItem,
  treeGuides,
  type ThreadIssueTreeRow,
  type ThreadStripItem,
  type ThreadTreeRow,
} from "@exp/ui"

import type { ReactNode } from "react"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2: the THREAD's parts, the REAL `@exp/ui` components
// (`session-thread-view.tsx`, `work-card.tsx`) painted from the contract's
// own models: the header (title + ONE tab strip, EXP-1334: "+" ·
// Conversation | issue chips … +X | session chips … +Y | Guide, nothing
// scrolls; folded chips open their subtree on a click), THE session card at every state (`session-card.json`) and
// the Threads list (`partitionThreads`, `threads-list.json`) in the Reviews
// list language. The web app binds them to the synced rows
// (`apps/web/src/routes/t/$teamSlug/threads/`).

type ThreadInput = Parameters<typeof buildSessionThread>[0]
type ListInput = Parameters<typeof partitionThreads>[0]

const noop = (): void => {}
/** EXP-1318: the specimen rows' root device (the Devices page's glyph). */
const DEVICE = { icon: getDeviceIcon({ kind: `desktop` }), name: `Alex's MacBook Pro` }
const PrivateIcon = conceptIcon(`ui-private`)
const NeedsYouIcon = conceptIcon(`coding-needs-input`)
const RecentIcon = conceptIcon(`ui-check`)
const PrOpenIcon = conceptIcon(`pr-open`)
const PrStackIcon = conceptIcon(`pr-stack`)

const STATUS = [
  { icon: SEMANTIC_ICONS[`status-in-progress`], colorClass: `text-amber-400` },
  { icon: SEMANTIC_ICONS[`status-backlog`], colorClass: `text-muted-foreground` },
] as const

const THREAD = buildSessionThread(threadFixture.cases[0]!.input as unknown as ThreadInput)
const PRIVATE = buildSessionThread(threadFixture.cases[1]!.input as unknown as ThreadInput)
const LIST = partitionThreads(listFixture.cases[0]!.input as unknown as ListInput)
/** EXP-1323: a live tree nesting its children, unread rows bold. */
const NESTED = partitionThreads(
  listFixture.cases.find((entry) => entry.name.startsWith(`EXP-1323`))!.input as unknown as ListInput
)

const NOW = `2026-10-13T10:00:00.000Z`
const BASE = {
  id: `s`,
  teamId: `t`,
  parentId: null,
  rootId: `s`,
  kind: `thread`,
  userId: `u`,
  status: `running`,
  busy: false,
  needsInput: false,
  blocked: null,
  shared: true,
  title: null,
  guide: null,
  actionName: null,
  name: null,
  icon: null,
  color: null,
  sortOrder: 0,
  startedAt: `2026-10-13T09:47:26.000Z`,
  endedAt: null,
  createdAt: `2026-10-13T09:47:00.000Z`,
  updatedAt: `2026-10-13T09:59:00.000Z`,
}

const CARDS = [
  sessionCard({ ...BASE, busy: true }, 1, NOW),
  // EXP-1323: the live caption, the card's muted second line.
  sessionCard(
    { ...BASE, title: `Add width and height to cart thumbnails`, busy: true, caption: `Reading CartLineItem.tsx` },
    7,
    NOW
  ),
  sessionCard({ ...BASE, title: `Profile the launch path`, needsInput: true }, 2, NOW),
  sessionCard({ ...BASE, title: `Review the fix` }, 3, NOW),
  sessionCard({ ...BASE, status: `queued`, startedAt: null }, 4, NOW),
  sessionCard({ ...BASE, blocked: { window: `5h` } }, 5, NOW),
  sessionCard({ ...BASE, status: `ended`, endedAt: `2026-10-13T09:51:10.000Z` }, 6, NOW),
]

const issueTitle = (issueId: string) =>
  [...threadFixture.cases[0]!.input.issues, ...threadFixture.cases[11]!.input.issues, ...EXTRA_ISSUES].find(
    (issue) => issue.id === issueId
  )?.title ?? issueId

/** EXP-1334: the folded case (sub-issues + a session subtree), plus two
 *  more issues so the strip has something to fold. */
const FOLDED = buildSessionThread(threadFixture.cases[11]!.input as unknown as ThreadInput)
const EXTRA_ISSUES = [
  { id: `i-5`, identifier: `EXP-25`, title: `Context ring` },
  { id: `i-6`, identifier: `EXP-26`, title: `Model picker` },
]
const FOLDED_TABS: typeof FOLDED.tabs = [
  ...FOLDED.tabs.filter((tab) => tab.kind !== `session` && tab.kind !== `guide`),
  ...EXTRA_ISSUES.map((issue) => ({
    kind: `issue` as const,
    issueId: issue.id,
    identifier: issue.identifier,
    nested: [],
  })),
  ...FOLDED.tabs.filter((tab) => tab.kind === `session` || tab.kind === `guide`),
]

type IssueTab = Extract<(typeof THREAD.tabs)[number], { kind: `issue` }>
type SessionTab = Extract<(typeof THREAD.tabs)[number], { kind: `session` }>

const statusOf = (index: number) => STATUS[index % STATUS.length]!

/** THE strip from a view's tabs, the way the app binds it (issue chips =
 *  `ThreadIssueTab`, session chips = `ThreadSessionTab`, a chip with nested
 *  items = `ThreadChipSubtree`). `open` = the open issue / session id. */
function stripItems(tabs: typeof THREAD.tabs, open: string | null) {
  const issues: ThreadStripItem[] = tabs
    .filter((tab): tab is IssueTab => tab.kind === `issue`)
    .map((tab, index) => {
      const nested = tab.nested.find((row) => row.issueId === open) ?? null
      const active = open === tab.issueId || nested !== null
      const folded = tab.nested.length > 0
      return {
        key: tab.issueId,
        active,
        entry: { kind: `item`, id: tab.issueId, label: `${tab.identifier} ${issueTitle(tab.issueId)}`, onSelect: noop },
        render: ({ short, measure }) => {
          const chip = (
            <ThreadIssueTab
              identifier={tab.identifier}
              title={issueTitle(tab.issueId)}
              status={statusOf(index)}
              active={active}
              short={short}
              path={nested?.identifier ?? null}
              chevron={folded}
            />
          )
          if (measure || !folded) return chip
          return (
            <ThreadChipSubtree title={tab.identifier} trigger={chip}>
              {(close) => (
                <ThreadIssueTree menu rows={issueSubtree(tab)} activeId={open} onOpen={close} />
              )}
            </ThreadChipSubtree>
          )
        },
      }
    })
  const sessions: ThreadStripItem[] = tabs
    .filter((tab): tab is SessionTab => tab.kind === `session`)
    .map((tab) => {
      const nested = tab.nested.find((row) => row.sessionId === open) ?? null
      const active = open === tab.sessionId || nested !== null
      const folded = tab.nested.length > 0
      return {
        key: tab.sessionId,
        active,
        entry: { kind: `item`, id: tab.sessionId, label: tab.name, onSelect: noop },
        render: ({ measure }) => {
          const chip = (
            <ThreadSessionTab
              agent="claude"
              markState={tab.markState}
              name={tab.name}
              active={active}
              path={nested?.name ?? null}
              chevron={folded}
            />
          )
          if (measure || !folded) return chip
          return (
            <ThreadChipSubtree title={tab.name} trigger={chip}>
              {(close) => (
                <ThreadSessionTree menu rows={sessionSubtree(tab)} activeId={open} onOpen={close} />
              )}
            </ThreadChipSubtree>
          )
        },
      }
    })
  return { issues, sessions }
}

function issueSubtree(tab: IssueTab): ThreadIssueTreeRow[] {
  return [
    { issueId: tab.issueId, identifier: tab.identifier, title: issueTitle(tab.issueId), status: statusOf(0), depth: 0 },
    ...tab.nested.map((row, index) => ({
      issueId: row.issueId,
      identifier: row.identifier,
      title: issueTitle(row.issueId),
      status: statusOf(index + 1),
      depth: row.depth,
    })),
  ]
}

/** The nested rows as THE tree rows (depth → parent: the nearest shallower
 *  row before it). */
function sessionSubtree(tab: SessionTab): ThreadTreeRow[] {
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
}

const STATE_WORD: Record<string, string | undefined> = {
  working: `Working`,
  needs_you: `Needs you`,
  ended: `Ended`,
}

/** The islands render static markup (no effects, nothing measures), so the
 *  specimen hands `ThreadTabs` the contract's own partition over ESTIMATED
 *  chip widths for a zone of `zone` px. */
function estimateLayout(tabs: typeof THREAD.tabs, open: string | null, zone: number, pager: boolean) {
  const text = (chars: number) => chars * 7.6
  const frame = 20 + 14 + 6
  const issues = tabs.filter((tab): tab is IssueTab => tab.kind === `issue`)
  const sessions = tabs.filter((tab): tab is SessionTab => tab.kind === `session`)
  const chevron = (nested: readonly unknown[]) => (nested.length > 0 ? 20 : 0)
  const short = (tab: IssueTab) => frame + text(tab.identifier.length) + chevron(tab.nested)
  return partitionThreadStrip({
    issues: issues.map((tab) => ({
      full: pager ? short(tab) : short(tab) + 6 + text(issueTitle(tab.issueId).length),
      short: short(tab),
    })),
    sessions: sessions.map((tab) => {
      const nested = tab.nested.find((row) => row.sessionId === open)
      return frame + text(tab.name.length + (nested ? nested.name.length + 2 : 0)) + chevron(tab.nested)
    }),
    available: zone,
    gap: 4,
    overflowW: 32,
    activeIssue: (() => {
      const index = issues.findIndex(
        (tab) => tab.issueId === open || tab.nested.some((row) => row.issueId === open)
      )
      return index < 0 ? null : index
    })(),
    activeSession: (() => {
      const index = sessions.findIndex(
        (tab) => tab.sessionId === open || tab.nested.some((row) => row.sessionId === open)
      )
      return index < 0 ? null : index
    })(),
  })
}

function Strip({
  tabs,
  open = null,
  width,
  zone,
  pager = false,
}: {
  tabs: typeof THREAD.tabs
  open?: string | null
  width?: string
  /** The zone's px between Conversation and Guide (the estimate). */
  zone?: number
  pager?: boolean
}) {
  const { issues, sessions } = stripItems(tabs, open)
  const guide = tabs.find((tab) => tab.kind === `guide`)
  return (
    <div style={width ? { width } : undefined}>
      <ThreadTabs
        variant={pager ? `pager` : `bar`}
        layout={zone === undefined ? undefined : estimateLayout(tabs, open, zone, pager)}
        conversation={{ label: threadFixture.copy.conversation, active: open === null, onSelect: noop }}
        issues={issues}
        sessions={sessions}
        add={<ThreadTabsAddButton />}
        guide={{
          label: threadFixture.copy.guide,
          active: false,
          dot: guide?.kind === `guide` && guide.dot,
          onSelect: noop,
        }}
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
      <Strip tabs={view.tabs} width="48rem" zone={520} />
    </div>
  )
}

const Caption = ({ children }: { children: ReactNode }) => (
  <p className="text-xs text-muted-foreground">{children}</p>
)

/** EXP-1334: the strip at three widths (full → identifiers → folded "+N"),
 *  a nested session open (`Threads impl › Android lane`), the phone pager,
 *  a chip's subtree and the "+" menu, painted open. */
function FoldedStrips() {
  return (
    <div className="flex w-[84rem] flex-col gap-3" data-testid="thread-strip-widths">
      <Caption>Wide: every chip full (identifier + title)</Caption>
      <Strip tabs={FOLDED_TABS} width="84rem" zone={1150} />
      <Caption>Narrower: the issues give up their titles first</Caption>
      <Strip tabs={FOLDED_TABS} width="54rem" zone={640} />
      <Caption>Narrow: each zone folds into its own "+N"; the open chip (a nested session) stays</Caption>
      <Strip tabs={FOLDED_TABS} open="g2" width="36rem" zone={340} />
      <Caption>Phone pager: Conversation and Guide icon-only, identifiers only, the same zones</Caption>
      <Strip tabs={FOLDED_TABS} open="i-3" width="28rem" zone={200} pager />
      <div className="mt-2 flex flex-wrap items-start gap-8">
        <div className="flex flex-col gap-2">
          <div className={cn(MENU_SURFACE_CLASS, `w-80 p-1`)}>
            <ThreadSessionTree rows={sessionSubtree(FOLDED.tabs.find((tab): tab is SessionTab => tab.kind === `session`)!)} activeId="g2" onOpen={noop} />
          </div>
          <Caption>A session chip's subtree, opened by a click (phones: the bottom sheet)</Caption>
        </div>
        <div className="flex flex-col gap-2">
          <div className={cn(MENU_SURFACE_CLASS, `w-80 p-1`)}>
            <ThreadIssueTree rows={issueSubtree(FOLDED.tabs.find((tab): tab is IssueTab => tab.kind === `issue`)!)} activeId="i-3" onOpen={noop} />
          </div>
          <Caption>An issue chip's sub-issues</Caption>
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

const GROUP_GLYPH: Record<ThreadsListGroup[`key`], ReactNode> = {
  needs_you: <NeedsYouIcon className="size-3.5 shrink-0 text-amber-400" />,
  running: <AgentWorkingMark agent={null} />,
  recent: <RecentIcon className="size-3.5 shrink-0 text-muted-foreground" />,
}

/** The specimen's words: one per row, like the app derives them. */
const WORDS: Record<string, { word: string; lead?: ReactNode; icon?: ReactNode }> = {
  a: { word: `question` },
  b: { word: `stack`, icon: <PrStackIcon className="size-3 shrink-0" /> },
  d: { word: `ready to merge`, lead: <PrOpenIcon className="size-3.5 text-emerald-500" /> },
}

function ThreadsList({
  groups = LIST,
  agents = true,
}: {
  groups?: readonly ThreadsListGroup[]
  agents?: boolean
}) {
  return (
    <div className="flex max-w-3xl flex-col gap-6" data-testid="threads-list">
      {agents && (
        <div>
          <GlassSectionHeader
            label={listFixture.copy.agents}
            leading={<BoardGlyph board={{ icon: `bot` }} className="size-3.5 text-muted-foreground" />}
          />
          <ThreadsListItem
            testId="agents-row"
            lead={<BoardGlyph board={{ icon: `bug`, color: `#a78bfa` }} className="size-3.5" />}
            title="Triage"
            word="needs input"
            onOpen={noop}
          />
          <ThreadsListItem
            testId="agents-row"
            lead={<BoardGlyph board={{ icon: `palette`, color: `#f472b6` }} className="size-3.5" />}
            title="Designer"
            onOpen={noop}
          />
        </div>
      )}
      {groups.map((group) => {
        const guides = treeGuides(group.rows.map((row) => row.depth))
        return (
          <div key={group.key}>
            <GlassSectionHeader label={group.title} leading={GROUP_GLYPH[group.key]} />
            {group.rows.map((row, index) => {
              if (row.depth > 0) {
                return (
                  <SessionRow
                    key={row.sessionId}
                    size={row.caption ? `big` : `small`}
                    agent="claude"
                    markState={row.needsYou ? `needs_input` : row.busy ? `working` : undefined}
                    title={row.title}
                    caption={row.caption}
                    unread={row.unread}
                    depth={row.depth}
                    guide={guides[index]}
                    onClick={noop}
                  />
                )
              }
              const word = WORDS[row.sessionId]
              return (
                <ThreadsListItem
                  key={row.sessionId}
                  lead={
                    row.busy ? (
                      <AgentWorkingMark agent="claude" />
                    ) : (
                      (word?.lead ?? (
                        <AgentRunMark
                          agent="claude"
                          state={row.needsYou ? `needs_input` : group.key === `recent` ? `ended` : undefined}
                          ringClassName="ring-background"
                        />
                      ))
                    )
                  }
                  title={row.title}
                  unread={row.unread}
                  caption={row.caption}
                  word={word?.word ?? null}
                  wordIcon={word?.icon}
                  locked={row.private}
                  device={row.deviceId ? DEVICE : null}
                  onOpen={noop}
                />
              )
            })}
          </div>
        )
      })}
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `thread-parts`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Thread`,
  blurb: `A thread IS a root session. Header: the title (\`Implement EXP-12\`, \`Run <action>\`, the agent's auto-name; nobody renames it; a lock when private) and ONE tab strip that never scrolls (EXP-1334): "+" (THE Menu: Link an issue › the search list, New issue) and Conversation pinned left, Guide pinned right (a green dot while the root's Guide holds anything); between them the TOP-LEVEL issue chips then the dispatcher's DIRECT child sessions, THE strip chip (\`StripChip\`, shared with the work tabs strip): everything full → issues identifier-only → each zone folds into its own "+N" (\`partitionThreadStrip\`), the open chip always drawn. A chip with sub-issues / deeper sessions wears a chevron and a CLICK opens its subtree (THE tree rows with guides; phones: the bottom sheet); it reads \`<chip> › <open one>\` while a nested item is open. The conversation IS the dispatcher: no card of its own. No captions, no right sidebar; md+: while the Guide's PR is open the title row's trailing slot carries Merge (\`Merge PR\` | \`Merge stack\`) on every tab, then the \`…\` on the Guide tab; phones use the strip as the pager and keep the bottom Merge capsule on the Guide. THE session card = the Work card's header row: the state icon, the name (\`Session N\` untitled), the muted state word (amber Needs you, rose Usage limit), trailing the elapsed and a spinner while working; it opens that session. The Threads page (and the phones' Threads tab) = Needs you · Running · Recent bands (filled, no count) over flat gapless rows, no dividers: the lead (spinner while busy, amber badge when it needs you, else the PR glyph), the title, ONE quiet word (\`ready to merge\`, \`stack\`, \`merged\`, \`question\`), the lock when private, then the root's device glyph (its name on hover, EXP-1318); phones put the Agents band on top. Models = \`buildSessionThread()\`, \`sessionCard()\`, \`partitionThreads()\` (fixtures \`session-thread.json\`, \`session-card.json\`, \`threads-list.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadTabs / ThreadsListItem / SessionCard`,
      file: `packages/ui/src/session-thread-view.tsx`,
      note: `pages: apps/web/src/routes/t/$teamSlug/threads/; card: packages/ui/src/work-card.tsx`,
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
      <FoldedStrips />
      <div className="flex max-w-xl flex-col gap-2">
        {CARDS.map((card) => (
          <SessionCard key={card.name} card={card} onOpen={noop} />
        ))}
      </div>
      <ThreadsList />
      <ThreadsList groups={NESTED} agents={false} />
    </div>
  ),
}
