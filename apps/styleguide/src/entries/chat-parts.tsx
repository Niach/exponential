import chatFixture from "@exp/domain-contract/fixtures/session-chat.json"
import listFixture from "@exp/domain-contract/fixtures/chat-list.json"
import {
  buildSessionChat,
  partitionChats,
  sessionCard,
  type ChatsListGroup,
} from "@exp/domain-contract/session-tree"
import {
  sessionScreen,
  sessionScreenSubtitle,
  SESSION_SCREEN_BESIDE,
  SESSION_SCREEN_COPY,
  SESSION_SCREEN_TEST_IDS,
  type SessionScreenCounts,
} from "@exp/domain-contract/session-screen"
import { SEMANTIC_ICONS } from "@exp/icons"
import {
  BoardGlyph,
  Button,
  ChatLeadDisc,
  ChatSessionTree,
  conceptIcon,
  getDeviceIcon,
  ListRow,
  MenuPanel,
  Pill,
  SegmentedControl,
  SessionCard,
  StatusGlyph,
  ChatTabsAddButton,
  ChatsList,
  type ChatTreeRow,
} from "@exp/ui"

import type { ReactNode } from "react"

import type { StyleguideEntry } from "./types.ts"

// EXP-1293: the CHAT's parts, the REAL `@exp/ui` components
// (`session-chat-view.tsx`, `work-card.tsx`) painted from the contract's
// own models: #EXP-1732 THE session screen's header (the lead avatar, the
// title over ONE subtitle line, THE gear, the `…`) and THE SegmentedControl
// of `sessionScreen().tabs` with their muted counts, per kind; the Issues
// and Sessions tab bodies; THE session card at every state (`session-card.json`) and
// the Chats list (`partitionChats`, `chat-list.json`) in the Reviews
// list language. The web app binds them to the synced rows
// (`apps/web/src/routes/t/$teamSlug/chats/`).

type ChatInput = Parameters<typeof buildSessionChat>[0]
type ListInput = Parameters<typeof partitionChats>[0]

const noop = (): void => {}
const NavSettingsIcon = conceptIcon(`nav-settings`)
const UiMoreIcon = conceptIcon(`ui-more`)
/** EXP-1318: the specimen rows' root device (the Devices page's glyph). */
const DEVICE = { icon: getDeviceIcon({ kind: `desktop` }), name: `Alex's MacBook Pro` }
const PrivateIcon = conceptIcon(`ui-private`)

const STATUS = [
  { icon: SEMANTIC_ICONS[`status-in-progress`], colorClass: `text-yellow-400` },
  { icon: SEMANTIC_ICONS[`status-backlog`], colorClass: `text-muted-foreground` },
] as const

const CHAT = buildSessionChat(chatFixture.cases[0]!.input as unknown as ChatInput)
const PRIVATE = buildSessionChat(chatFixture.cases[1]!.input as unknown as ChatInput)
const LIST = partitionChats(listFixture.cases[0]!.input as unknown as ListInput)

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
const FOLDED_CASE = chatFixture.cases.find((fixtureCase) =>
  fixtureCase.name.startsWith(`EXP-1334: the root has no chip or card`)
)!
const issueTitle = (issueId: string) =>
  [...chatFixture.cases[0]!.input.issues, ...FOLDED_CASE.input.issues, ...EXTRA_ISSUES].find(
    (issue) => issue.id === issueId
  )?.title ?? issueId

/** EXP-1334: the folded case's issues + sessions plus two more issues. */
const FOLDED = buildSessionChat(FOLDED_CASE.input as unknown as ChatInput)
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

type IssueTab = Extract<(typeof CHAT.tabs)[number], { kind: `issue` }>
type SessionTab = Extract<(typeof CHAT.tabs)[number], { kind: `session` }>

const statusOf = (index: number) => STATUS[index % STATUS.length]!

/** The Issues tab's rows = every linked issue (a sub-issue indented under
 *  its parent), the way the app binds them. */
function treeIssues(tabs: typeof CHAT.tabs) {
  return tabs
    .filter((tab): tab is IssueTab => tab.kind === `issue`)
    .flatMap((tab) => [
      { issueId: tab.issueId, identifier: tab.identifier, nested: false },
      ...tab.nested.map((ref) => ({ ...ref, nested: true })),
    ])
    .map((ref, index) => ({ ...ref, title: issueTitle(ref.issueId), status: statusOf(index) }))
}

/** The sessions under the dispatcher as THE tree rows (depth → parent: the
 *  nearest shallower row before it). */
function treeSessions(tabs: typeof CHAT.tabs): ChatTreeRow[] {
  return tabs
    .filter((tab): tab is SessionTab => tab.kind === `session`)
    .flatMap((tab) => {
      const rows: ChatTreeRow[] = [
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

const Caption = ({ children }: { children: ReactNode }) => (
  <p className="text-xs text-muted-foreground">{children}</p>
)

/** #EXP-1732: THE session screen's header ×web (`session-screen-frame.tsx`
 *  draws the same parts): the lead avatar, the title over ONE subtitle
 *  line, THE gear (agent, Assistant, app), the `…`; under it THE
 *  SegmentedControl, each label followed by its muted count. */
function ScreenHeader({
  kind,
  avatar,
  title,
  titleTrailing,
  subtitle,
  trailing,
  counts,
  requested = null,
  width = `48rem`,
}: {
  kind: string
  avatar: ReactNode
  title: string
  titleTrailing?: ReactNode
  subtitle: string
  trailing?: ReactNode
  counts: SessionScreenCounts
  requested?: string | null
  width?: string
}) {
  const screen = sessionScreen({ kind, wide: true, requested, open: null, counts })
  return (
    <div className="flex flex-col gap-3 border-b border-border pb-3" style={{ width }}>
      <div className="flex items-center gap-3">
        {avatar}
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight">{title}</h1>
            {titleTrailing}
          </div>
          {subtitle && <p className="truncate text-xs text-muted-foreground">{subtitle}</p>}
        </div>
        {trailing}
        {screen.gear && (
          <Button variant="ghost" size="icon-sm" aria-label={SESSION_SCREEN_COPY.settings} className="text-muted-foreground">
            <NavSettingsIcon className="size-4" />
          </Button>
        )}
        <Button variant="ghost" size="icon-sm" aria-label="More" className="text-muted-foreground">
          <UiMoreIcon />
        </Button>
      </div>
      <div data-testid={SESSION_SCREEN_TEST_IDS.tabs} className="min-w-0 overflow-x-auto">
        <SegmentedControl
          value={screen.active}
          onValueChange={noop}
          options={screen.tabs.map((tab) => ({
            value: tab.id,
            label: (
              <span className="flex items-center gap-1.5 whitespace-nowrap">
                {tab.label}
                {tab.count !== null && <span className="text-muted-foreground tabular-nums">{tab.count}</span>}
              </span>
            ),
          }))}
        />
      </div>
    </div>
  )
}

const DISC_CLASS = `!size-10 [&>svg]:size-5!`

/** #EXP-1732: the header per kind + the Issues and Sessions bodies (the
 *  wide beside rule's tab pane, 5fr chat : 7fr pane, min 360px chat). */
function SessionScreenSpecimens() {
  const issues = treeIssues(GROUP_TABS)
  const sessions = treeSessions(GROUP_TABS)
  const counts = { issues: issues.length, sessions: sessions.length, results: 3 }
  return (
    <div className="flex flex-col gap-6" data-testid="session-screen-headers">
      <Caption>A chat: Chat · Issues · Sessions · Results, no gear</Caption>
      <ScreenHeader
        kind="session"
        avatar={<ChatLeadDisc lead="working" agent="claude" className={DISC_CLASS} />}
        title={CHAT.title}
        subtitle={sessionScreenSubtitle({ status: `working`, device: DEVICE.name, trigger: null })}
        counts={counts}
      />
      <ScreenHeader
        kind="session"
        avatar={<ChatLeadDisc lead="waiting" agent="claude" className={DISC_CLASS} />}
        title={PRIVATE.title}
        titleTrailing={<PrivateIcon className="size-3.5 shrink-0 text-muted-foreground" />}
        subtitle={sessionScreenSubtitle({ status: `idle`, device: DEVICE.name, trigger: null })}
        counts={{ issues: 0, sessions: 0, results: 0 }}
        requested="results"
      />
      <Caption>An agent: its disc, the trigger in the subtitle, THE gear</Caption>
      <ScreenHeader
        kind="agent"
        avatar={
          <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted">
            <BoardGlyph board={{ icon: `bug`, color: `#a78bfa` }} className="size-5" />
          </span>
        }
        title="Triage"
        subtitle={sessionScreenSubtitle({ status: `idle`, device: DEVICE.name, trigger: `Every 30 minutes` })}
        counts={{ issues: 2, sessions: 4, results: 0 }}
      />
      <Caption>An app: its glyph + the version pill, Publish, the gear; Preview by default</Caption>
      <ScreenHeader
        kind="app"
        avatar={
          <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted">
            <BoardGlyph board={{ icon: `clock`, color: `#38bdf8` }} className="size-5" />
          </span>
        }
        title="Working hours"
        titleTrailing={<Pill size="sm" className="tabular-nums">v3</Pill>}
        subtitle={sessionScreenSubtitle({ status: `idle`, device: DEVICE.name, trigger: null })}
        trailing={<Button size="sm">Publish</Button>}
        counts={{ issues: 0, sessions: 1, results: 2 }}
      />
      <Caption>Phone: the same strip scrolls sideways when it overflows</Caption>
      <ScreenHeader
        kind="app"
        avatar={
          <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-muted">
            <BoardGlyph board={{ icon: `clock`, color: `#38bdf8` }} className="size-5" />
          </span>
        }
        title="Working hours"
        subtitle="idle"
        counts={{ issues: 0, sessions: 12, results: 7 }}
        requested="chat"
        width="22rem"
      />
      <Caption>
        Wide: the chat ({SESSION_SCREEN_BESIDE.chat}fr, min {SESSION_SCREEN_BESIDE.chatMinWidth}px) beside the tab&apos;s pane
        ({SESSION_SCREEN_BESIDE.pane}fr), a hairline between. The Issues pane (led by the &quot;+&quot;) and the Sessions pane:
      </Caption>
      <div className="flex flex-wrap items-start gap-8">
        <div className="flex w-96 flex-col gap-1" data-testid={SESSION_SCREEN_TEST_IDS.issues}>
          <div className="flex items-center px-1">
            <ChatTabsAddButton />
          </div>
          {issues.map((issue) => (
            <ListRow key={issue.issueId} interactive className={issue.nested ? `gap-2 pl-8 text-sm` : `gap-2 text-sm`}>
              <StatusGlyph {...issue.status} className="size-3.5 shrink-0" />
              <span className="shrink-0 font-mono text-xs text-muted-foreground">{issue.identifier}</span>
              <span className="min-w-0 flex-1 truncate">{issue.title}</span>
            </ListRow>
          ))}
        </div>
        <div className="w-96" data-testid={SESSION_SCREEN_TEST_IDS.sessions}>
          <ChatSessionTree rows={sessions} activeId="g2" onOpen={noop} />
        </div>
        <div className="flex flex-col gap-2">
          <MenuPanel
            density="pointer"
            entries={[
              { kind: `submenu`, id: `link`, label: `Link an issue`, icon: conceptIcon(`nav-search`), entries: [] },
              { kind: `item`, id: `new`, label: `New issue`, icon: conceptIcon(`ui-add`), onSelect: noop },
            ]}
          />
          <Caption>The &quot;+&quot; menu</Caption>
        </div>
      </div>
    </div>
  )
}

/** The specimen's PRs: `b` sits in an open stack (on another chat's open
 *  PR), `d` is ready to merge, `c` merged (the violet lead, no word); `a`
 *  waits on a question. The words come from the REAL `chatWordOf`. */
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

/** THE app list (`@exp/ui` `ChatsList`, EXP-1339: same component, no drift). */
function ChatsListSpecimen({
  groups = LIST,
  agents = true,
}: {
  groups?: readonly ChatsListGroup[]
  agents?: boolean
}) {
  return (
    <div className="max-w-3xl">
      <ChatsList
        groups={groups}
        cliByRoot={CLAUDE}
        prsByRoot={PRS_BY_ROOT}
        teamPrs={TEAM_PRS}
        agents={agents ? AGENTS : undefined}
        deviceGlyph={deviceGlyph}
        onOpenChat={noop}
        onOpenAgent={noop}
      />
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `chat-parts`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Chat`,
  blurb: `A chat IS a root session. #EXP-1732 THE session screen ×4 (session-screen.json, ONE screen for a chat, an agent's or the Assistant's conversation and an app's editor session): the header = the lead avatar (a chat: THE lead disc; an agent: its disc; the Assistant: THE brand mark; an app: its BoardGlyph + the version pill), the title (THE \`sessionTitle()\` chain; nobody renames it; a lock when private) over ONE subtitle line \`status · device · trigger\`, then the merge pill, an app's Publish, THE gear (agents, the Assistant, apps: their settings as an opened item) and the \`…\`; under it ONE SegmentedControl of \`sessionScreen().tabs\`, each label followed by its count as a muted number (0 = none): Chat · Issues · Sessions · Results (an app: Chat · Preview · Data · Versions · Sessions · Results); a phone scrolls it sideways. Wide (≥ 768px) = the beside rule: the chat pane (5fr, min 360px) beside the active tab's pane (7fr), a hairline between; Chat alone = the chat full width; phones = one pane, an opened item (issue, session, plan, settings) a pushed page with its \`‹\`. Issues = THE issue rows (status glyph, mono identifier, title; nested indented) led by the \`+\` (Link an issue ›, New issue); Sessions = THE tree rows (SessionRow small + the guides) of every session under the root; a row opens it in place. No chip strip, no plan tab, no pager. The conversation IS the dispatcher: no card of its own. No captions, no right sidebar; while the chat's PR is open the header's trailing slot carries Merge (\`Merge PR\` | \`Merge stack\`) and the \`…\` on every form factor, phones included; phones use the strip as the pager. THE session card = the Work card's header row: the state icon in its tone, the name (the same \`sessionTitle()\` chain, never a number), the muted state word (amber while it needs you: Needs you, Usage limit, the natives' semantic yellow ×4), trailing the elapsed and a spinner while working; it opens that session. The Chats page (and the phones' Chats tab) = Needs you · Running · Recent bands (filled, no count) over flat gapless rows, no dividers: the lead (spinner while busy, amber badge when it needs you, else the PR glyph), the title, ONE quiet word (\`ready to merge\`, \`stack\`, \`question\`; a merged chat shows only the violet lead, no word, and no row carries a lock, EXP-1339), then the root's device glyph (its name on hover, EXP-1318); the page and this specimen render THE same \`ChatsList\`; phones put the Agents band on top. Models = \`buildSessionChat()\`, \`sessionCard()\`, \`partitionChats()\`, \`sessionTitle()\` (fixtures \`session-chat.json\`, \`session-card.json\`, \`chat-list.json\`, \`session-title.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionScreenFrame / SegmentedControl / SessionCard`,
      file: `packages/ui/src/session-chat-view.tsx`,
      note: `#EXP-1732 the screen: apps/web/src/components/chat/session-screen-frame.tsx (header + tabs + beside), bodies: session-screen-bodies.tsx; ChatsList: packages/ui/src/chats-list.tsx; card: packages/ui/src/work-card.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `SessionChatScreenView`,
      file: `apps/desktop/crates/ui/src/session_chat.rs`,
      note: `model: crates/domain/src/session_tree.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `ChatScreen`,
      file: `apps/ios/Exponential/UI/Chat/ChatScreen.swift`,
      note: `model: ExpCore SessionTree.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ChatScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/chat/ChatScreen.kt`,
      note: `model: domain/SessionTree.kt`,
    },
  },
  island: () => (
    <div className="flex flex-col gap-10 text-sm">
      <SessionScreenSpecimens />
      <div className="flex max-w-xl flex-col gap-2">
        {CARDS.map((card) => (
          <SessionCard key={card.name} card={card} onOpen={noop} />
        ))}
      </div>
      <ChatsListSpecimen />
    </div>
  ),
}
