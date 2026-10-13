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
  AgentRunMark,
  AgentWorkingMark,
  BoardGlyph,
  conceptIcon,
  GlassSectionHeader,
  IssueChip,
  SessionCard,
  ThreadTabs,
  ThreadTabsAddButton,
  ThreadsListItem,
  threadTabChipClass,
} from "@exp/ui"

import type { ReactNode } from "react"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293 Threads v2: the THREAD's parts, the REAL `@exp/ui` components
// (`session-thread-view.tsx`, `work-card.tsx`) painted from the contract's
// own models: the header (title + ONE tab strip: Conversation · issue chips
// · "+" · Guide), THE session card at every state (`session-card.json`) and
// the Threads list (`partitionThreads`, `threads-list.json`) in the Reviews
// list language. The web app binds them to the synced rows
// (`apps/web/src/routes/t/$teamSlug/threads/`).

type ThreadInput = Parameters<typeof buildSessionThread>[0]
type ListInput = Parameters<typeof partitionThreads>[0]

const noop = (): void => {}
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
const LIST_INPUT = listFixture.cases[0]!.input as unknown as ListInput

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
  sessionCard({ ...BASE, title: `Profile the launch path`, needsInput: true }, 2, NOW),
  sessionCard({ ...BASE, title: `Review the fix` }, 3, NOW),
  sessionCard({ ...BASE, status: `queued`, startedAt: null }, 4, NOW),
  sessionCard({ ...BASE, blocked: { window: `5h` } }, 5, NOW),
  sessionCard({ ...BASE, status: `ended`, endedAt: `2026-10-13T09:51:10.000Z` }, 6, NOW),
]

const issueTitle = (issueId: string) =>
  (threadFixture.cases[0]!.input.issues as { id: string; title: string }[]).find(
    (issue) => issue.id === issueId
  )?.title ?? issueId

function Header({ view, active }: { view: typeof THREAD; active: string | null }) {
  const issues = view.tabs.flatMap((tab, index) =>
    tab.kind === `issue`
      ? [
          <IssueChip
            key={tab.issueId}
            identifier={tab.identifier}
            title={issueTitle(tab.issueId)}
            status={STATUS[index % STATUS.length]!}
            className={threadTabChipClass(active === tab.issueId)}
            onClick={noop}
          />,
        ]
      : []
  )
  const guide = view.tabs.find((tab) => tab.kind === `guide`)
  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <div className="flex min-w-0 items-center gap-2">
        <h1 className="truncate text-2xl font-semibold tracking-tight">{view.title}</h1>
        {view.private && <PrivateIcon className="size-4 shrink-0 text-muted-foreground" />}
      </div>
      <ThreadTabs
        conversation={{ label: threadFixture.copy.conversation, active: active === null, onSelect: noop }}
        issues={issues}
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

function identifierOf(sessionId: string): string | null {
  const link = LIST_INPUT.sessionIssues.find((row) => row.sessionId === sessionId)
  return link ? (LIST_INPUT.issues.find((issue) => issue.id === link.issueId)?.identifier ?? null) : null
}

function ThreadsList() {
  return (
    <div className="flex max-w-3xl flex-col gap-6" data-testid="threads-list">
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
      {LIST.map((group) => (
        <div key={group.key}>
          <GlassSectionHeader label={group.title} leading={GROUP_GLYPH[group.key]} />
          {group.rows.map((row) => {
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
                identifier={identifierOf(row.sessionId)}
                title={row.title}
                word={word?.word ?? null}
                wordIcon={word?.icon}
                locked={row.private}
                onOpen={noop}
              />
            )
          })}
        </div>
      ))}
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `thread-parts`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Thread`,
  blurb: `A thread IS a root session. Header: the title (renamed in place; a lock when private) and ONE tab strip: Conversation · one issue chip per linked issue (status glyph, mono identifier, title; its context menu unlinks) · "+" (link an issue) · Guide (a green dot while the root's Guide holds anything). No captions, no Stop, no right sidebar; phones use the strip as the pager. THE session card = the Work card's header row: the state icon, the name (\`Session N\` untitled), the muted state word (amber Needs you, rose Usage limit), trailing the elapsed and a spinner while working; it opens that session. The Threads page (and the phones' Threads tab) = Needs you · Running · Recent bands (filled, no count) over flat rows: the lead (spinner while busy, amber badge when it needs you, else the PR glyph), the first issue's identifier, the title, ONE quiet word (\`ready to merge\`, \`stack\`, \`merged\`, \`question\`) or a lock; phones put the Agents band on top. Models = \`buildSessionThread()\`, \`sessionCard()\`, \`partitionThreads()\` (fixtures \`session-thread.json\`, \`session-card.json\`, \`threads-list.json\` ×4).`,
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
      <Header view={THREAD} active={null} />
      <Header view={PRIVATE} active={null} />
      <div className="flex max-w-xl flex-col gap-2">
        {CARDS.map((card) => (
          <SessionCard key={card.name} card={card} onOpen={noop} />
        ))}
      </div>
      <ThreadsList />
    </div>
  ),
}
