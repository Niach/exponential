import { MessageBubble } from "./user-message-bubble"
import { useState, type ComponentProps, type ReactNode } from "react"
import chats from "@exp/domain-contract/fixtures/chats.json"
import {
  chatSelectionActions,
  chatMenuEntries,
  type AgentChatRow,
  type AppChatRow,
  type ChatLead,
  type ChatRow as ChatRowModel,
} from "@exp/domain-contract/chats"
import type { SessionChatRow } from "@exp/domain-contract/session-tree"
import { SEALED_PLACEHOLDER } from "@exp/domain-contract/sealed-content"
import { BoardGlyph } from "./board-glyph"
import { BulkBarAction, BulkBarCapsule, BulkBarSeparator } from "./bulk-bar"
import { Button } from "./button"
import { CHAT_SELECTION_ICONS, ChatLeadDisc, ChatList, ChatsHeader } from "./chat-list"
import { Dialog } from "./dialog"
import { FAB_CHROME_CLASS } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"
import { MenuPanel } from "./menu"
import { HEADER_BUTTON_CLASS, MobileDetailHeader } from "./mobile-detail-header"
import { PageHeader } from "./page-header"
import { AgentHeaderTitle, PhoneHeaderTitle } from "./phone-header-title"
import { PhoneBoardMenuStrip } from "./phone-board-menu"
import { PhoneTab, PhoneTabBar } from "./phone-tab-bar"
import { PromptLayout } from "./prompt"
import { ChatConversation } from "./session-chat-view"
import { SurfaceCard } from "./surface-card"
import { TeamAvatar } from "./team-avatar"
import { chatMenuRows, sessionScreenMenuRows } from "./chats-list"
import { sessionScreenMenu } from "@exp/domain-contract/session-screen"
import { TooltipProvider } from "./tooltip"
import { useIsMobile } from "./use-mobile"

// Chat 58cb32df (#EXP-1373, fixtures/chats.json): THE chat + phone-shell specimens,
// one source for the styleguide entries (`chat-list`, `chat-menus`,
// #EXP-1388 `chat-selection`,
// `chat-transcript`, `phone-tab-bar`; #EXP-1384 `phone-board-menu`,
// `phone-page-header`) and the app's DEV-ONLY `?dev=<id>`
// captures on the Agent route; the natives' specimens draw the same.

const COPY = chats.copy
const CallIcon = conceptIcon(`voice-call`)
const MoreIcon = conceptIcon(`ui-more`)
const noop = (): void => {}

// ── chat-list ────────────────────────────────────────────────────────────
// The phone Chats screen's pieces fed the fixture's own expected model:
// `listCases[0]` and `agentRowCases[0]`.

const LIST_ROWS = chats.listCases[0]!.expected.rows as ChatRowModel[]
/** S-3: the pinned agent rows (`agentRowCases[0]`: the Assistant first). */
const AGENT_ROWS = chats.agentRowCases[0]!.expected as AgentChatRow[]
const APP_ROWS = chats.appRowCases[0]!.expected as AppChatRow[]

// EXP-1647: the phone column is 390px at the PHONE root (16px) = 24.375rem:
// the wide web root is 18.5px (styles.css). #FEED-191: no rings, no chips: the
// agents, then the chats under their section headers.
export function ChatListSpecimen() {
  return (
    <div
      data-testid="chat-list-specimen"
      className="flex w-[24.375rem] max-w-full flex-col gap-4 overflow-hidden rounded-xl bg-background p-4 max-md:rounded-none max-md:p-0"
    >
      <ChatsHeader inboxCount={3} />
      <ChatList rows={LIST_ROWS} agentRows={AGENT_ROWS} appRows={APP_ROWS} agentOf={() => `claude`} />
    </div>
  )
}

// ── chat-header ──────────────────────────────────────────────────────────
// #EXP-1688 (chats.json `shell.chatHeader`): THE chat header on a phone, the
// app's own MobileDetailHeader + PhoneHeaderTitle: a chat waiting on you, a
// working one on a device, an agent chat with its telephone.

function ChatHeaderTrailing({ call = false }: { call?: boolean }) {
  return (
    <span className="flex shrink-0 items-center gap-1">
      {call ? (
        <Button variant="ghost" size="icon" className={HEADER_BUTTON_CLASS} aria-label="Call">
          <CallIcon className="size-4" />
        </Button>
      ) : null}
      <Button variant="ghost" size="icon" className={HEADER_BUTTON_CLASS} aria-label="More">
        <MoreIcon className="size-4" />
      </Button>
    </span>
  )
}

export function ChatHeaderSpecimen() {
  const header = chats.shell.chatHeader
  const titleOf = (lead: ChatLead, title: string, subtitle: ReactNode) => (
    <PhoneHeaderTitle
      disc={<ChatLeadDisc lead={lead} agent="claude" className="!size-10 [&>svg]:size-5! [&>[data-slot=run-mark-badge]]:right-1.5! [&>[data-slot=run-mark-badge]]:top-1.5!" />}
      title={title}
      subtitle={subtitle}
    />
  )
  return (
    <div
      data-testid="chat-header-specimen"
      className="flex w-[24.375rem] max-w-full flex-col gap-3 rounded-xl bg-background p-4 max-md:rounded-none max-md:px-0"
    >
      {caption(`A chat that needs you (${header.row}px, ghost buttons, always leading)`)}
      <MobileDetailHeader
        onBack={noop}
        title={titleOf(
          `needs_you`,
          `Reduce cold start`,
          <span className="text-yellow-400">{`needs input · Alex's MacBook`}</span>
        )}
        menu={<ChatHeaderTrailing />}
      />
      {caption(`A working chat`)}
      <MobileDetailHeader
        onBack={noop}
        title={titleOf(
          `working`,
          `A title long enough that it truncates on one line`,
          <span className="text-foreground">{COPY.preview.working}</span>
        )}
        menu={<ChatHeaderTrailing />}
      />
      {caption(`An agent chat (the telephone)`)}
      <MobileDetailHeader
        onBack={noop}
        title={<AgentHeaderTitle name="Triage" icon="bug" color="#a855f7" />}
        menu={<ChatHeaderTrailing call />}
      />
    </div>
  )
}

// ── chat-selection ───────────────────────────────────────────────────────
// #EXP-1388 (chats.json SELECTION + ARCHIVE): the list in selection mode:
// the Archived entry first (listCases[1]'s count), the rows of listCases[0]
// with two selected (a running chat and an unread one), over THE bulk bar
// with `chatSelectionActions` for them (the phone's icon-only cells).

const ARCHIVED_COUNT = chats.listCases[1]!.expected.archived
const SELECTED = new Set([`b`, `c`])
export function ChatSelectionSpecimen() {
  const isMobile = useIsMobile()
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
      className="flex w-[24.375rem] max-w-full flex-col gap-4 overflow-hidden rounded-xl bg-background p-4 max-md:rounded-none max-md:p-0"
    >
      {/* The phones' screen: the title over the list; the Agents section stays
          while selecting (agents are never selectable), every form factor. */}
      {isMobile && <ChatsHeader inboxCount={3} />}
      <ChatList
        rows={LIST_ROWS}
        agentRows={AGENT_ROWS}
        agentOf={() => `claude`}
        archived={ARCHIVED_COUNT}
        selection={selected}
        onRowClick={toggle}
      />
      <div className="flex pt-2">
        <BulkBarCapsule
          count={COPY.selection.count.replace(`{n}`, String(selected.size))}
          onClear={noop}
          iconOnly
          className="w-full"
        >
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

const LIVE = chatMenuRows(chatMenuEntries({ rootStatus: `running`, canSteer: true, canDelete: true }), noop)
const ENDED = chatMenuRows(chatMenuEntries({ rootStatus: `ended`, canSteer: true, canDelete: true }), noop)
// #FEED-239: THE session screen menu (`sessionScreenMenu`) of an agent.
const AGENT_MENU_INPUT = { kind: `agent`, phone: false, member: true, linkIssue: true, publish: false, export: false, delete: false }
const AGENT_BUSY_OWNER = sessionScreenMenuRows(`agent`, sessionScreenMenu({ ...AGENT_MENU_INPUT, settings: true }), noop)
const AGENT_IDLE = sessionScreenMenuRows(`agent`, sessionScreenMenu({ ...AGENT_MENU_INPUT, settings: false }), noop)

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
  // Pointers draw the dropdowns, touch the long-press sheets (titled): the
  // look each form factor really opens, the phones' set ×3.
  const isMobile = useIsMobile()
  const menu = (entries: ComponentProps<typeof MenuPanel>[`entries`], title: string) =>
    isMobile ? (
      <MenuPanel entries={entries} look="sheet" density="touch" title={title} />
    ) : (
      <MenuPanel entries={entries} density="pointer" />
    )
  return (
    // #EXP-1553: phones stack with half the gap, so the delete confirm shows
    // whole (its buttons included) in the phone frame.
    <div
      data-testid="chat-menus-specimen"
      className="flex flex-wrap items-start gap-8 max-md:gap-4"
    >
      <div className="max-md:w-full">
        {caption(`A live chat`)}
        {menu(LIVE, `Spike: offline queue`)}
      </div>
      <div className="max-md:w-full">
        {caption(`An ended chat`)}
        {menu(ENDED, `#APP-16 Fix flaky scroll restore`)}
      </div>
      <div className="max-md:w-full">
        {caption(`The agent menu (owner)`)}
        {menu(AGENT_BUSY_OWNER, `Triage`)}
      </div>
      <div className="max-md:w-full">
        {caption(`The agent menu (member)`)}
        {menu(AGENT_IDLE, `Triage`)}
      </div>
      <DeleteConfirm />
    </div>
  )
}

// ── chat-transcript ──────────────────────────────────────────────────────
// The REAL `ChatConversation` in both layouts (phone bubbles, web-wide
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
const TRANSCRIPT_ROWS: SessionChatRow[] = [
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
    authorId: `u-alex`,
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
  {
    type: `message`,
    id: `m5`,
    author: `agent`,
    authorId: null,
    body: `Started a session to fix it.`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  {
    type: `report`,
    id: `x1`,
    sessionId: `c-fix`,
    text: `Report from Fix the sign-in redirect`,
    summary: `Done: the redirect keeps the return URL now. PR #412 is open, checks are green.`,
    xref: { kind: `report`, sessionId: `c-fix`, name: `Fix the sign-in redirect`, agentId: null },
  },
  // #EXP-1772: a busy agent chat: a schedule firing the agent answered
  // (THE marker above the reply), a merge line, a relay (its name above).
  { type: `event`, id: `e1`, event: `trigger_fired`, text: `Schedule`, sessionId: null },
  {
    type: `message`,
    id: `m6`,
    author: `agent`,
    authorId: null,
    body: `PR #412 merged overnight; the iPad sign-in reports stopped.`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  { type: `event`, id: `e2`, event: `pr_merged`, text: `Merged PR #412`, sessionId: null },
  {
    type: `message`,
    id: `x2`,
    author: `agent`,
    authorId: null,
    body: `Verified on the iPad: sign-in keeps the return URL, and the redirect loop is gone on Safari too.`,
    queued: false,
    authorSessionId: `c-fix`,
    xref: { kind: `message`, sessionId: `c-fix`, name: `Fix the sign-in redirect`, agentId: null },
  },
  { type: `divider`, id: `d2`, kind: `new`, at: `2026-10-24T09:30:00` },
]

const TIMES: Record<string, string> = { m1: `21:02`, m2: `21:14`, m3: `09:02`, m4: `09:03`, m5: `09:03`, m6: `07:00`, x2: `08:41` }

/** #EXP-1736: the specimens' chat (a plain session) and its people. */
const SPECIMEN_CHAT = { sessionId: `r`, title: `Reduce cold start`, agentId: null }
const SPECIMEN_VIEWER = `u-alex`
const specimenMember = (id: string) => (id === `u-dana` ? { id, name: `Dana Reyes` } : null)

/** #FEED-237 (message-delivery.json): the viewer's own messages in each
 *  delivery state, WhatsApp-style, at the transcript's bottom. */
const DELIVERY_BUBBLES = [
  { id: `o1`, text: `Use the Pixel 6a numbers as the baseline.`, time: `09:04`, status: `sent` },
  { id: `o2`, text: `And keep the cache behind a flag.`, time: `09:05`, status: `sending` },
  { id: `o3`, text: `Ping me when it's under a second.`, time: `09:05`, status: `failed` },
] as const

function Transcript({ layout }: { layout: `transcript` | `bubbles` }) {
  return (
    <>
    <ChatConversation
      rows={TRANSCRIPT_ROWS}
      layout={layout}
      renderText={paragraph}
      attachmentSrc={() => ``}
      chat={SPECIMEN_CHAT}
      viewerId={SPECIMEN_VIEWER}
      member={specimenMember}
      messageTime={(row) => TIMES[row.id] ?? null}
      onSurfaceReply={() => {}}
      arrived={() => false}
    />
    <div data-testid="chat-delivery-specimen" className="mt-3 flex flex-col gap-1.5">
      {DELIVERY_BUBBLES.map((bubble) => (
        <MessageBubble
          key={bubble.id}
          side="out"
          text={bubble.text}
          renderText={paragraph}
          time={bubble.time}
          status={bubble.status}
          menu={bubble.status === `failed` ? [{ kind: `item`, id: `retry`, label: `Retry`, onSelect: () => {} }] : undefined}
        />
      ))}
    </div>
    </>
  )
}

export function ChatTranscriptSpecimen({ single = false }: { single?: boolean }) {
  const isMobile = useIsMobile()
  // The shot (`?dev=chat-transcript`): only the layout this form factor really
  // draws, as desktop (transcript) and the phones (bubbles) shoot it. The
  // styleguide entry keeps both labelled, and the enter replay.
  if (single) {
    return (
      <TooltipProvider>
        <div data-testid="chat-transcript-specimen" className="w-[42rem] max-w-full">
          <Transcript layout={isMobile ? `bubbles` : `transcript`} />
        </div>
      </TooltipProvider>
    )
  }
  return (
    <TooltipProvider>
      <div data-testid="chat-transcript-specimen" className="flex flex-wrap items-start gap-6">
        <div className="w-[22rem] max-w-full rounded-3xl border border-glass-stroke-card bg-background p-4">
          <div className="mb-3 text-xs text-muted-foreground">Phone</div>
          <Transcript layout="bubbles" />
        </div>
        <div className="w-[32rem] max-w-full">
          <div className="mb-3 text-xs text-muted-foreground">Web wide</div>
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

// ── sealed-locked ────────────────────────────────────────────────────────
// EXP-1459 (sealed-content.json `placeholder`): content this install cannot
// open (yet). In a transcript THE pill with the lock (both layouts); in the
// list the preview line italic muted, like `working`.

const LOCKED_ROWS: SessionChatRow[] = [
  { ...TRANSCRIPT_ROWS[0]!, id: `l-m1` } as SessionChatRow,
  { type: `locked`, id: `l-k1`, reason: `noKey`, author: `agent` },
  {
    type: `message`,
    id: `l-m2`,
    author: `agent`,
    authorId: null,
    body: `Deferred the shape subscribe until after the first frame.`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  { type: `locked`, id: `l-k2`, reason: `unreadable`, author: `user` },
]

const LOCKED_LIST_ROWS: ChatRowModel[] = LIST_ROWS.slice(0, 3).map((row, index) =>
  index === 0
    ? { ...row, preview: { kind: `locked`, text: SEALED_PLACEHOLDER.copy.noKey } }
    : index === 2
      ? { ...row, preview: { kind: `locked`, text: SEALED_PLACEHOLDER.copy.unreadable } }
      : row
)

function LockedTranscript({ layout }: { layout: `transcript` | `bubbles` }) {
  return (
    <ChatConversation
      rows={LOCKED_ROWS}
      layout={layout}
      renderText={paragraph}
      attachmentSrc={() => ``}
      chat={SPECIMEN_CHAT}
      viewerId={SPECIMEN_VIEWER}
      member={specimenMember}
      messageTime={(row) => TIMES[row.id.replace(`l-`, ``)] ?? null}
      onSurfaceReply={() => {}}
      arrived={() => false}
    />
  )
}

export function SealedLockedSpecimen() {
  return (
    <TooltipProvider>
      <div data-testid="sealed-locked-specimen" className="flex flex-wrap items-start gap-6">
        {/* EXP-1676: a phone draws what the native phones draw: its own bubbles
            and the rows, no board around them. */}
        <div className="w-[22rem] max-w-full rounded-3xl border border-glass-stroke-card bg-background p-4 max-md:w-full max-md:rounded-none max-md:border-0 max-md:bg-transparent max-md:p-0">
          <div className="mb-3 text-xs text-muted-foreground max-md:hidden">Phone</div>
          <LockedTranscript layout="bubbles" />
        </div>
        <div className="w-[32rem] max-w-full max-md:hidden">
          <div className="mb-3 text-xs text-muted-foreground">Web wide</div>
          <LockedTranscript layout="transcript" />
        </div>
        <div className="flex w-[390px] max-w-full flex-col gap-4 overflow-hidden rounded-xl bg-background p-4 max-md:w-full max-md:rounded-none max-md:bg-transparent max-md:p-0">
          <div className="text-xs text-muted-foreground max-md:hidden">Chats list</div>
          <ChatList flat rows={LOCKED_LIST_ROWS} agentOf={() => `claude`} />
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
const ChatsIcon = conceptIcon(`nav-chats`)
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
        caret={selected === `board` ? <BoardCaretIcon className="size-2.5 shrink-0" /> : undefined}
      />
      <PhoneTab
        label={COPY.tabs.chats}
        icon={<ChatsIcon className="size-5" />}
        selected={selected === `chats`}
        badge={{ count: 2, tone: `attention` }}
      />
      <PhoneTab
        label={COPY.tabs.apps}
        icon={<AppsIcon className="size-5" />}
        selected={selected === `apps`}
        lensPreview={dragging ? { offset: -10 } : undefined}
      />
      <PhoneTab label={TEAM} icon={<TeamAvatar iconUrl={null} size={20} />} selected={selected === `team`} caret={selected === `team` ? <BoardCaretIcon className="size-2.5 shrink-0" /> : undefined} />
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
      // EXP-1554: on a phone the specimen IS the screen (over the live tab
      // bar), so the panel visibly grows from the bottom strip's frame, the
      // live strip's own spot (px-4, pb-4), as Android pictures it.
      className="relative h-[340px] w-[390px] max-w-full overflow-hidden rounded-xl bg-background [transform:translateZ(0)] max-md:fixed max-md:inset-0 max-md:z-50 max-md:h-auto max-md:w-auto max-md:rounded-none"
    >
      <div className="flex flex-col gap-3 p-4 opacity-40">
        <PageHeader title={ACTIVE_BOARD.name} />
        {[0, 1, 2].map((row) => (
          <div key={row} className="h-11 rounded-xl bg-glass-row" />
        ))}
      </div>
      <div className="absolute inset-x-0 bottom-0 flex justify-center px-2 pb-4 max-md:px-4 max-md:pb-[max(1rem,env(safe-area-inset-bottom))]">
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
// title and the round search + inbox circles, the inbox count badge on and
// off; Devices (pushed from the Team page) with the back control.

export function PhonePageHeaderSpecimen() {
  return (
    <div
      data-testid="phone-page-header-specimen"
      className="flex w-[390px] max-w-full flex-col gap-4 rounded-xl bg-background p-4"
    >
      {caption(`Unread notification (the inbox count badge)`)}
      <PageHeader title={COPY.headers.chats} inboxCount={3} />
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
