import { useState, type ReactNode } from "react"
import chats from "@exp/domain-contract/fixtures/chats.json"
import {
  agentMenuEntries,
  threadMenuEntries,
  type AgentBubble,
  type ChatFilter,
  type ChatRow as ChatRowModel,
} from "@exp/domain-contract/chats"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { AgentBubbles, ChatFilterChips, ChatList, ChatsHeader } from "./chat-list"
import { Dialog } from "./dialog"
import { FAB_CHROME_CLASS } from "./fab-chrome"
import { conceptIcon } from "./icons.generated"
import { MenuPanel } from "./menu"
import { PhoneTab, PhoneTabBar } from "./phone-tab-bar"
import { PromptLayout } from "./prompt"
import { ThreadConversation } from "./session-thread-view"
import { SurfaceCard } from "./surface-card"
import { TeamAvatar } from "./team-avatar"
import { agentMenuRows, chatMenuRows } from "./threads-list"
import { TooltipProvider } from "./tooltip"

// Thread 58cb32df (#EXP-1373, fixtures/chats.json): THE four chat specimens,
// one source for the styleguide entries (`chat-list`, `chat-menus`,
// `chat-transcript`, `phone-tab-bar`) and the app's DEV-ONLY `?dev=<id>`
// captures on the Agent route; the natives' specimens draw the same.

const COPY = chats.copy
const noop = (): void => {}

// ── chat-list ────────────────────────────────────────────────────────────
// The phone Chats screen's pieces fed the fixture's own expected model:
// `listCases[0]` and `bubbleCases[0]`. The chips filter the same rows locally.

const LIST = chats.listCases[0]!.expected
const LIST_ROWS = LIST.rows as ChatRowModel[]
const BUBBLES = chats.bubbleCases[0]!.expected as AgentBubble[]

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
      <ChatList rows={rows} agentOf={() => `claude`} />
    </div>
  )
}

// ── chat-menus ───────────────────────────────────────────────────────────
// THE chat menu and THE agent menu in both looks (pointer + the phone sheet
// titled with the chat's title), plus the delete confirm.

const LIVE = chatMenuRows(threadMenuEntries({ rootStatus: `running`, canSteer: true }), noop)
const ENDED = chatMenuRows(threadMenuEntries({ rootStatus: `ended`, canSteer: true }), noop)
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
  },
  {
    type: `message`,
    id: `m4`,
    author: `agent`,
    authorId: null,
    body: `Three reports, all about the iPad sign-in. I linked them to #APP-19.`,
    queued: false,
    authorSessionId: null,
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
// THE phone bottom bar's tab capsule beside the new chat / new issue capsule,
// three states: Chats, Team and Board selected, the amber dot on Chats.

const BoardIcon = conceptIcon(`nav-issues`)
const DevicesIcon = conceptIcon(`nav-devices`)
const ChatsIcon = conceptIcon(`nav-threads`)
const ActionChatIcon = conceptIcon(`action-chat`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)

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

function Bar({ selected }: { selected: `chats` | `team` | `board` }) {
  return (
    <div className="flex items-center gap-2">
      <PhoneTabBar>
        <PhoneTab label={COPY.tabs.board} icon={<BoardIcon className="size-5" />} selected={selected === `board`} />
        <PhoneTab label={COPY.tabs.devices} icon={<DevicesIcon className="size-5" />} />
        <PhoneTab
          label={COPY.tabs.chats}
          icon={<ChatsIcon className="size-5" />}
          selected={selected === `chats`}
          dot="bg-yellow-400"
        />
        <PhoneTab label="Acme" icon={<TeamAvatar name="Acme" size={20} />} selected={selected === `team`} />
      </PhoneTabBar>
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
      className="flex flex-col items-start gap-4 rounded-xl bg-background p-4 max-sm:-mx-4 max-sm:px-0"
    >
      <Bar selected="chats" />
      <Bar selected="team" />
      <Bar selected="board" />
    </div>
  )
}
