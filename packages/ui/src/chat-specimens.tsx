import { useState, type ReactNode } from "react"
import chats from "@exp/domain-contract/fixtures/chats.json"
import {
  agentMenuEntries,
  chatSelectionActions,
  threadMenuEntries,
  type AgentBubble,
  type AgentChatRow,
  type ChatFilter,
  type ChatRow as ChatRowModel,
} from "@exp/domain-contract/chats"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { BoardGlyph } from "./board-glyph"
import { BulkBarAction, BulkBarCapsule, BulkBarSeparator } from "./bulk-bar"
import { AgentBubbles, CHAT_SELECTION_ICONS, ChatFilterChips, ChatList, ChatsHeader } from "./chat-list"
import { Dialog } from "./dialog"
import { FAB_CHROME_CLASS } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"
import { MenuPanel } from "./menu"
import { PageHeader } from "./page-header"
import { PhoneBoardMenuStrip } from "./phone-board-menu"
import { PhoneTab, PhoneTabBar } from "./phone-tab-bar"
import { PromptLayout } from "./prompt"
import { ThreadConversation } from "./session-thread-view"
import { SurfaceCard } from "./surface-card"
import { TeamAvatar } from "./team-avatar"
import { agentMenuRows, chatMenuRows } from "./threads-list"
import { TooltipProvider } from "./tooltip"

// Thread 58cb32df (#EXP-1373, fixtures/chats.json): THE chat + phone-shell specimens,
// one source for the styleguide entries (`chat-list`, `chat-menus`,
// #EXP-1388 `chat-selection`,
// `chat-transcript`, `phone-tab-bar`; #EXP-1384 `phone-board-menu`,
// `phone-page-header`) and the app's DEV-ONLY `?dev=<id>`
// captures on the Agent route; the natives' specimens draw the same.

const COPY = chats.copy
const noop = (): void => {}

// ── chat-list ────────────────────────────────────────────────────────────
// The phone Chats screen's pieces fed the fixture's own expected model:
// `listCases[0]` and `bubbleCases[0]`. The chips filter the same rows locally.

const LIST = chats.listCases[0]!.expected
const LIST_ROWS = LIST.rows as ChatRowModel[]
const BUBBLES = chats.bubbleCases[0]!.expected as AgentBubble[]
/** S-3: the pinned agent rows (`agentRowCases[0]`: the Assistant first). */
const AGENT_ROWS = chats.agentRowCases[0]!.expected as AgentChatRow[]

export function ChatListSpecimen() {
  const [filter, setFilter] = useState<ChatFilter>(`all`)
  const rows = LIST_ROWS.filter(
    (row) =>
      filter === `all` ||
      (filter === `needs_you` && row.group === `needs_you`) ||
      (filter === `running` && row.group === `running`) ||
      (filter === `unread` && row.unreadCount > 0)
  )
  return (
    <div
      data-testid="chat-list-specimen"
      className="flex w-[390px] max-w-full flex-col gap-4 overflow-hidden rounded-xl bg-background p-4"
    >
      <ChatsHeader inboxUnread />
      <AgentBubbles bubbles={BUBBLES} />
      <ChatFilterChips value={filter} counts={LIST.counts} onChange={setFilter} />
      <ChatList rows={rows} agentRows={AGENT_ROWS} agentOf={() => `claude`} />
    </div>
  )
}

// ── chat-selection ───────────────────────────────────────────────────────
// #EXP-1388 (chats.json SELECTION + ARCHIVE): the list in selection mode:
// the Archived entry first (listCases[4]'s count), the rows of listCases[0]
// with two selected (a running chat and an unread one), over THE bulk bar
// with `chatSelectionActions` for them (the phone's icon-only cells).

const ARCHIVED_COUNT = chats.listCases[4]!.expected.archived
const SELECTED = new Set([`b`, `c`])
export function ChatSelectionSpecimen() {
  const [selected, setSelected] = useState<ReadonlySet<string>>(SELECTED)
  const toggle = (id: string) =>
    setSelected((current) => {
      const next = new Set(current)
      if (!next.delete(id)) next.add(id)
      return next.size === 0 ? SELECTED : next
    })
  const actions = chatSelectionActions({
    rows: LIST_ROWS.filter((row) => selected.has(row.sessionId)).map((row) => ({
      sessionId: row.sessionId,
      rootStatus: row.group === `recent` ? `ended` : `running`,
      unreadCount: row.unreadCount,
      canSteer: true,
      canDelete: true,
    })),
  })
  return (
    <div
      data-testid="chat-selection-specimen"
      className="flex w-[390px] max-w-full flex-col gap-4 overflow-hidden rounded-xl bg-background p-4"
    >
      <ChatFilterChips value="all" counts={LIST.counts} onChange={noop} />
      <ChatList
        rows={LIST_ROWS}
        agentOf={() => `claude`}
        archived={ARCHIVED_COUNT}
        selection={selected}
        onRowClick={toggle}
      />
      <div className="flex justify-center pt-2">
        <BulkBarCapsule count={COPY.selection.count.replace(`{n}`, String(selected.size))} onClear={noop} iconOnly>
          {actions.map(({ action }) => {
            const Icon = CHAT_SELECTION_ICONS[action]
            return (
              <span key={action} className="contents">
                {action === `delete` && <BulkBarSeparator iconOnly />}
                <BulkBarAction
                  icon={<Icon className="size-4" />}
                  label={COPY.selection[action]}
                  destructive={action === `delete`}
                  iconOnly
                />
              </span>
            )
          })}
        </BulkBarCapsule>
      </div>
    </div>
  )
}

// ── chat-menus ───────────────────────────────────────────────────────────
// THE chat menu and THE agent menu in both looks (pointer + the phone sheet
// titled with the chat's title), plus the delete confirm.

const LIVE = chatMenuRows(threadMenuEntries({ rootStatus: `running`, canSteer: true, canDelete: true }), noop)
const ENDED = chatMenuRows(threadMenuEntries({ rootStatus: `ended`, canSteer: true, canDelete: true }), noop)
const AGENT_BUSY_OWNER = agentMenuRows(agentMenuEntries({ busy: true, isOwner: true }), noop)
const AGENT_IDLE = agentMenuRows(agentMenuEntries({ busy: false, isOwner: false }), noop)

const caption = (text: string) => <div className="mb-2 text-xs text-muted-foreground">{text}</div>

/** The delete confirm at rest: the app's `Prompt` portals its card, so the
 *  specimen draws `PromptLayout` in the centred dialog panel's frame with the
 *  prompt card's padding and width, under a Radix root for its context. */
function DeleteConfirm() {
  return (
    <Dialog open>
      <div className="grid gap-2">
        <p className="text-xs text-muted-foreground">The delete confirm</p>
        <div
          className="relative flex w-full max-w-md flex-col gap-4 overflow-hidden rounded-2xl border border-glass-stroke-card bg-card/85 p-5 shadow-2xl shadow-black/40 backdrop-blur-2xl"
        >
          <PromptLayout
            title={COPY.confirmDelete.title}
            body={COPY.confirmDelete.body}
            actions={[
              { label: `Cancel`, role: `cancel` },
              { label: COPY.menu.delete, role: `destructive` },
            ]}
          />
        </div>
      </div>
    </Dialog>
  )
}

export function ChatMenusSpecimen() {
  return (
    <div data-testid="chat-menus-specimen" className="flex flex-wrap items-start gap-8">
      <div>
        {caption(`A live chat`)}
        <MenuPanel entries={LIVE} density="pointer" />
      </div>
      <div>
        {caption(`An ended chat`)}
        <MenuPanel entries={ENDED} density="pointer" />
      </div>
      <div>
        {caption(`The agent menu (busy, owner)`)}
        <MenuPanel entries={AGENT_BUSY_OWNER} density="pointer" />
      </div>
      <div>
        {caption(`The agent menu (idle, member)`)}
        <MenuPanel entries={AGENT_IDLE} density="pointer" />
      </div>
      <div className="w-[24rem]">
        {caption(`Phone: the long-press sheet`)}
        <MenuPanel entries={LIVE} look="sheet" density="touch" title="Spike: offline queue" />
      </div>
      <DeleteConfirm />
    </div>
  )
}

// ── chat-transcript ──────────────────────────────────────────────────────
// The REAL `ThreadConversation` in both layouts (phone bubbles, web-wide
// transcript): the divider pills, an agent's `SurfaceCard`, a superseded
// plan; the enter animation replays from the checkbox.

const paragraph = (text: string): ReactNode => (
  <p className="text-sm leading-relaxed whitespace-pre-wrap text-foreground">{text}</p>
)

/** An agent's inline preview (the core catalog), its two presses `reply`. */
const LAUNCH_SURFACE = {
  root: {
    id: `root`,
    component: `Stack`,
    props: { gap: `md` },
    children: [
      {
        id: `stat`,
        component: `StatTile`,
        props: { label: `Cold start, Pixel 6a`, value: `740 ms`, delta: `−47 %`, tone: `success`, trend: [1400, 1310, 1180, 980, 820, 740] },
        children: [],
      },
      {
        id: `actions`,
        component: `Stack`,
        props: { direction: `horizontal`, gap: `sm` },
        children: [
          {
            id: `ship`,
            component: `Button`,
            props: { label: `Ship snapshot` },
            children: [],
            on: { press: { event: { name: chats.surfaceCard.replyEvent, context: { text: `Ship the snapshot` } } } },
          },
          {
            id: `keep`,
            component: `Button`,
            props: { label: `Keep profiling`, variant: `secondary` },
            children: [],
            on: { press: { event: { name: chats.surfaceCard.replyEvent, context: { text: `Keep profiling` } } } },
          },
        ],
      },
    ],
  },
  data: {},
}

// The dividers' `at` carries no offset, so it parses as LOCAL time and the
// pills read 18:20 / 09:30 in every zone (the natives' specimens, the blurb).
const TRANSCRIPT_ROWS: ThreadRow[] = [
  {
    type: `message`,
    id: `m1`,
    author: `user`,
    authorId: `u-dana`,
    body: `Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it.`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  {
    type: `plan`,
    id: `p-old`,
    sessionId: `r`,
    plan: `# Plan\nProfile first`,
    questions: [],
    surface: null,
    decision: `superseded`,
    fresh: false,
    answers: null,
    feedback: null,
    surfaceData: null,
  },
  {
    type: `message`,
    id: `m2`,
    author: `agent`,
    authorId: null,
    body: `Deferred the shape subscribe until after the first frame and added a board snapshot cache. Here's where the time went:`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  { type: `surface`, id: `s1`, sessionId: `r`, title: `Launch path`, surface: LAUNCH_SURFACE },
  { type: `divider`, id: `d1`, kind: `compact`, at: `2026-10-23T18:20:00` },
  {
    type: `message`,
    id: `m3`,
    author: `user`,
    authorId: `u-dana`,
    body: `Anything new overnight?`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  {
    type: `message`,
    id: `m4`,
    author: `agent`,
    authorId: null,
    body: `Three reports, all about the iPad sign-in. I linked them to #APP-19.`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  { type: `divider`, id: `d2`, kind: `new`, at: `2026-10-24T09:30:00` },
]

const TIMES: Record<string, string> = { m1: `21:02`, m2: `21:14`, m3: `09:02`, m4: `09:03` }

function Transcript({ layout }: { layout: `transcript` | `bubbles` }) {
  return (
    <ThreadConversation
      rows={TRANSCRIPT_ROWS}
      layout={layout}
      renderText={paragraph}
      attachmentSrc={() => ``}
      messageCaption={() => `Dana Reyes · 21:02`}
      messageTime={(row) => TIMES[row.id] ?? null}
      onSurfaceReply={() => {}}
      arrived={() => false}
    />
  )
}

export function ChatTranscriptSpecimen() {
  return (
    <TooltipProvider>
      <div data-testid="chat-transcript-specimen" className="flex flex-wrap items-start gap-6">
        <div className="w-[22rem] max-w-full rounded-3xl border border-glass-stroke-card bg-background p-4">
          <div className="mb-3 text-xs text-muted-foreground">Phone (bubbles)</div>
          <Transcript layout="bubbles" />
        </div>
        <div className="w-[32rem] max-w-full">
          <div className="mb-3 text-xs text-muted-foreground">Web wide (transcript)</div>
          <Transcript layout="transcript" />
        </div>
        <div className="w-[24rem]">
          <input id="sg-surface-enter" type="checkbox" className="peer sr-only" />
          <label
            htmlFor="sg-surface-enter"
            className="mb-3 inline-flex cursor-pointer items-center rounded-full border border-glass-stroke-card bg-glass-row px-3 py-1 text-xs text-foreground peer-checked:bg-glass-active"
          >
            Replay the enter
          </label>
          <SurfaceCard
            surface={LAUNCH_SURFACE}
            title="Launch path"
            testKey="sg-enter"
            className="peer-checked:motion-enter peer-checked:slide-in-from-bottom-[12px] peer-checked:zoom-in-[0.98] peer-checked:[--tw-duration:var(--motion-duration-slow)]"
          />
        </div>
      </div>
    </TooltipProvider>
  )
}

// ── phone-tab-bar ────────────────────────────────────────────────────────
// #EXP-1384 (chats.json `shell`): THE phone bottom bar's tab capsule
// (Board · Chats · Apps · Team) beside the new chat / new issue capsule:
// Board selected on the board view (the board's glyph + name + the caret),
// Chats and Team selected, then the drag LENS mid-drag (`PhoneTab`
// `lensPreview`, a static state: the finger between Chats and Apps, Apps the
// candidate).

const AppsIcon = conceptIcon(`nav-apps`)
const ChatsIcon = conceptIcon(`nav-threads`)
const BoardCaretIcon = conceptIcon(chats.shell.boardTab.caret as `nav-team-switcher`)
const ActionChatIcon = conceptIcon(`action-chat`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const BackIcon = conceptIcon(`ui-back`)

/** The specimen team's boards in sidebar order (the screenshot seed's). */
const BOARDS = [
  { id: `app`, name: `Mobile App`, icon: `code`, color: `#6366f1` },
  { id: `mkt`, name: `Launch Marketing`, icon: `square-kanban`, color: `#f59e0b` },
  { id: `fb`, name: `Product Feedback`, icon: `megaphone`, color: `#22c55e` },
] as const
const ACTIVE_BOARD = BOARDS[0]
const TEAM = `Acme`

// The two-armed capsule as mobile-tab-bar.tsx paints it (static here).
function Capsule() {
  return (
    <div className={`flex h-[52px] shrink-0 items-stretch overflow-hidden rounded-full text-foreground ${FAB_CHROME_CLASS}`}>
      <span className="flex w-[52px] items-center justify-center">
        <ActionChatIcon className="size-5" />
      </span>
      <span aria-hidden className="my-3 w-px shrink-0 bg-glass-stroke-card" />
      <span className="flex w-[52px] items-center justify-center">
        <NavCreateIssueIcon className="size-5" />
      </span>
    </div>
  )
}

type BarTab = `board` | `chats` | `apps` | `team`

function Tabs({ selected, dragging = false }: { selected: BarTab; dragging?: boolean }) {
  return (
    // `dragging`: the static mid-drag state, the lens over Apps (the finger
    // 10px left of its centre), the selected tab's pill lifted off.
    <PhoneTabBar className="flex-1" data-dragging={dragging || undefined}>
      {/* The Board tab = the active board's glyph + name; on the board view
          the caret (its tap opens the board menu). */}
      <PhoneTab
        label={ACTIVE_BOARD.name}
        icon={<BoardGlyph board={ACTIVE_BOARD} className="size-5" />}
        selected={selected === `board`}
        trailing={selected === `board` ? <BoardCaretIcon className="size-2.5 shrink-0" /> : undefined}
      />
      <PhoneTab
        label={COPY.tabs.chats}
        icon={<ChatsIcon className="size-5" />}
        selected={selected === `chats`}
        dot="bg-yellow-400"
      />
      <PhoneTab
        label={COPY.tabs.apps}
        icon={<AppsIcon className="size-5" />}
        selected={selected === `apps`}
        lensPreview={dragging ? { offset: -10 } : undefined}
      />
      <PhoneTab label={TEAM} icon={<TeamAvatar name={TEAM} size={20} />} selected={selected === `team`} />
    </PhoneTabBar>
  )
}

function Bar({ selected, dragging = false }: { selected: BarTab; dragging?: boolean }) {
  return (
    <div className="flex w-full min-w-0 items-center" style={{ gap: chats.shell.bar.gap }}>
      <Tabs selected={selected} dragging={dragging} />
      <Capsule />
    </div>
  )
}

export function PhoneTabBarSpecimen() {
  return (
    // Phone widths drop the frame's padding into the page gutter so the bar
    // and the capsule fit side by side, as on the natives.
    <div
      data-testid="phone-tab-bar-specimen"
      className="flex w-[390px] max-w-full flex-col items-start gap-4 rounded-xl bg-background p-4 max-sm:-mx-4 max-sm:px-0"
    >
      {caption(`On the board view (the caret opens the board menu)`)}
      <Bar selected="board" />
      {caption(`Chats selected`)}
      <Bar selected="chats" />
      {caption(`Team selected`)}
      <Bar selected="team" />
      {caption(`Dragging: the lens between Chats and Apps`)}
      <Bar selected="chats" dragging />
    </div>
  )
}

// ── phone-board-menu ─────────────────────────────────────────────────────
// #EXP-1384 (chats.json `shell.boardMenu`): the board view's Board tab
// morphed the WHOLE strip into the glass board menu, shown OPEN: the
// team's boards (the active one lit, `ui-check`) then New board. The frame
// is a transform root so the menu's fixed scrim stays inside it.

export function PhoneBoardMenuSpecimen() {
  return (
    <div
      data-testid="phone-board-menu-specimen"
      className="relative h-[340px] w-[390px] max-w-full overflow-hidden rounded-xl bg-background [transform:translateZ(0)] max-sm:-mx-4"
    >
      <div className="flex flex-col gap-3 p-4 opacity-40">
        <PageHeader title={ACTIVE_BOARD.name} />
        {[0, 1, 2].map((row) => (
          <div key={row} className="h-11 rounded-xl bg-glass-row" />
        ))}
      </div>
      <div className="absolute inset-x-0 bottom-0 flex justify-center px-2 pb-4">
        <PhoneBoardMenuStrip
          open
          onOpenChange={noop}
          rows={BOARDS.map((board) => ({
            id: board.id,
            name: board.name,
            glyph: <BoardGlyph board={board} className="size-5" />,
            active: board.id === ACTIVE_BOARD.id,
          }))}
          onSelect={noop}
          onNewBoard={noop}
        >
          <Tabs selected="board" />
          <Capsule />
        </PhoneBoardMenuStrip>
      </div>
    </div>
  )
}

// ── page-header ──────────────────────────────────────────────────────────
// #EXP-1384 (chats.json `shell.header`): THE phone page header: the large
// title and the round search + inbox circles, the inbox's sky dot on and
// off; Devices (pushed from the Team page) with the back control.

export function PhonePageHeaderSpecimen() {
  return (
    <div
      data-testid="phone-page-header-specimen"
      className="flex w-[390px] max-w-full flex-col gap-4 rounded-xl bg-background p-4"
    >
      {caption(`Unread notification (the inbox dot)`)}
      <PageHeader title={COPY.headers.chats} inboxUnread />
      {caption(`All read`)}
      <PageHeader title={ACTIVE_BOARD.name} />
      {caption(`Pushed (Devices, the back control)`)}
      <PageHeader
        title={COPY.headers.devices}
        leading={
          <span className="-ml-2 flex size-9 shrink-0 items-center justify-center rounded-full text-muted-foreground">
            <BackIcon className="size-5" />
          </span>
        }
      />
    </div>
  )
}
