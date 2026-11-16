import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import {
  ASSISTANT_FAB,
  ASSISTANT_FAB_COPY,
  ASSISTANT_WIDGET,
  assistantFabVisible,
  assistantWidgetSize,
  type WidgetSize,
} from "@exp/domain-contract/agent-settings"
import { chatSessionNames } from "@exp/domain-contract/ask-dock"
import {
  Badge,
  Button,
  ChatDock,
  conceptIcon,
  ExponentialLogo,
  FabButton,
  Popover,
  PopoverContent,
  PopoverTrigger,
  SkeletonRows,
  useIsMobile,
  useToastLift,
} from "@exp/ui"
import type { Team, User } from "@/db/schema"
import { InPlaceSession } from "@/components/chat/session-body"
import { renderChatText } from "@/components/chat/chat-markdown"
import { SessionComposer } from "@/components/session-composer"
import { useAgentBadges } from "@/components/team/sidebar-agents"
import { useMyAgentConversation } from "@/hooks/use-agent-row"
import { useSession } from "@/hooks/use-session"
import { useSessionNav } from "@/hooks/use-session-nav"
import { useAgents, useSessionChat } from "@/hooks/use-session-tree"
import { useSuggestionActions } from "@/hooks/use-suggestion-actions"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { AgentRow } from "@/lib/session-tree"

// #EXP-1837 (agent-settings.json `assistant.fab`): on wide web the viewer's
// Assistant lives in THE FAB bottom-right of the content card, not in the
// sidebar: the phones' 52px glass circle with the brand mark and THE count
// badge `rowBadge` of the conversation. A press opens the widget — a
// popover over the page anchored to the FAB holding the REAL chat (the root's
// transcript + THE composer, no tabs) — with ⇱ maximize (the Assistant's chat
// page) and — minimize. Hidden on the Assistant's own page; phones keep it
// in Chats.

const MaximizeIcon = conceptIcon(`ui-maximize`)
const MinimizeIcon = conceptIcon(`ui-minimize`)

export const ASSISTANT_FAB_TEST_IDS = {
  fab: `assistant-fab`,
  badge: `assistant-fab-badge`,
  widget: `assistant-widget`,
  header: `assistant-widget-header`,
  maximize: `assistant-widget-maximize`,
  minimize: `assistant-widget-minimize`,
  resizeTop: `assistant-widget-resize-top`,
  resizeLeft: `assistant-widget-resize-left`,
  resizeCorner: `assistant-widget-resize-corner`,
} as const

// #EXP-1876 (`assistant.fab.widget`): the size the person dragged, per
// device (this browser) and per member; always read back through
// `assistantWidgetSize`'s clamp, so a smaller window never rewrites it.
const WIDGET_SIZE_KEY = (me: string | null) => `exp.assistant-widget-size.${me ?? `anon`}`

export function readWidgetSize(me: string | null): WidgetSize | null {
  try {
    const raw = JSON.parse(localStorage.getItem(WIDGET_SIZE_KEY(me)) ?? `null`) as Partial<WidgetSize> | null
    return raw && Number.isFinite(raw.width) && Number.isFinite(raw.height)
      ? { width: raw.width as number, height: raw.height as number }
      : null
  } catch {
    return null
  }
}

export function writeWidgetSize(me: string | null, size: WidgetSize | null) {
  try {
    if (size) localStorage.setItem(WIDGET_SIZE_KEY(me), JSON.stringify(size))
    else localStorage.removeItem(WIDGET_SIZE_KEY(me))
  } catch {
    // Storage off (private mode): the size only lives for this session.
  }
}

/** The content card's size: the FAB anchor's parent (the team layout's
 *  <main>), followed through resizes. */
function useCardSize(anchor: HTMLElement | null): WidgetSize | null {
  const [size, setSize] = useState<WidgetSize | null>(null)
  useEffect(() => {
    const card = anchor?.parentElement
    if (!card) return
    const read = () => setSize({ width: card.clientWidth, height: card.clientHeight })
    read()
    const observer = new ResizeObserver(read)
    observer.observe(card)
    return () => observer.disconnect()
  }, [anchor])
  return size
}

type ResizeAxes = { width: boolean; height: boolean }
const RESIZE_EDGES: { testId: string; axes: ResizeAxes; className: string }[] = [
  { testId: ASSISTANT_FAB_TEST_IDS.resizeTop, axes: { width: false, height: true }, className: `inset-x-0 top-0 cursor-ns-resize` },
  { testId: ASSISTANT_FAB_TEST_IDS.resizeLeft, axes: { width: true, height: false }, className: `inset-y-0 left-0 cursor-ew-resize` },
  { testId: ASSISTANT_FAB_TEST_IDS.resizeCorner, axes: { width: true, height: true }, className: `left-0 top-0 z-10 cursor-nwse-resize` },
]

export function AssistantFab({ team, teamSlug }: { team: Team | null | undefined; teamSlug: string }) {
  const isMobile = useIsMobile()
  const { data: session } = useSession()
  const me = session?.user?.id ?? null
  const { agents } = useAgents(team?.id, me)
  const assistant = useMemo(() => agents.find((agent) => agent.assistant) ?? null, [agents])
  const activeAgentId = useRouterState({
    select: (s) => /\/agents\/([^/]+)$/.exec(s.location.pathname)?.[1] ?? null,
  })
  const visible = assistantFabVisible({ wide: !isMobile, assistantId: assistant?.agentId, activeAgentId })
  if (!visible || !team || !assistant) return null
  return <AssistantFabButton team={team} teamSlug={teamSlug} assistant={assistant} me={me} />
}

function AssistantFabButton({
  team,
  teamSlug,
  assistant,
  me,
}: {
  team: Team
  teamSlug: string
  assistant: AgentRow
  me: string | null
}) {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const assistantRows = useMemo(() => [assistant], [assistant])
  const badge = useAgentBadges(team.id, assistantRows, me).get(assistant.agentId)
  useToastLift(ASSISTANT_FAB.toastLift)
  const [anchor, setAnchor] = useState<HTMLDivElement | null>(null)
  const card = useCardSize(anchor)
  const [stored, setStored] = useState<WidgetSize | null>(() => readWidgetSize(me))
  const drag = useRef<{ x: number; y: number; start: WidgetSize; axes: ResizeAxes; next: WidgetSize | null } | null>(null)
  const [dragged, setDragged] = useState<WidgetSize | null>(null)
  const size = card
    ? assistantWidgetSize({ card, stored: dragged ?? stored })
    : { width: ASSISTANT_WIDGET.min.width, height: ASSISTANT_WIDGET.min.height }
  const maximize = () => {
    setOpen(false)
    void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId: assistant.agentId } })
  }
  const startResize = useCallback(
    (axes: ResizeAxes) => (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      event.currentTarget.setPointerCapture(event.pointerId)
      drag.current = { x: event.clientX, y: event.clientY, start: size, axes, next: null }
    },
    [size]
  )
  const moveResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    const live = drag.current
    if (!live || !card) return
    // Anchored bottom-right: dragging left / up grows the widget.
    const next = {
      width: live.axes.width ? live.start.width + live.x - event.clientX : live.start.width,
      height: live.axes.height ? live.start.height + live.y - event.clientY : live.start.height,
    }
    live.next = assistantWidgetSize({ card, stored: next })
    setDragged(live.next)
  }
  const endResize = () => {
    const live = drag.current
    drag.current = null
    if (live?.next) {
      writeWidgetSize(me, live.next)
      setStored(live.next)
    }
    setDragged(null)
  }
  const resetSize = (event: ReactMouseEvent) => {
    if ((event.target as HTMLElement).closest(`button`)) return
    writeWidgetSize(me, null)
    setStored(null)
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      {/* #EXP-1860: `data-assistant-fab` reserves the corner (@exp/ui
          styles.css): page scroll content ends clear of it, THE Dock keeps
          its composer beside it. */}
      <div
        ref={setAnchor}
        data-assistant-fab=""
        className="pointer-events-none absolute z-20"
        style={{ right: ASSISTANT_FAB.inset, bottom: ASSISTANT_FAB.inset }}
      >
        <PopoverTrigger asChild>
          <FabButton
            aria-label={assistant.name || ASSISTANT_FAB_COPY.label}
            title={assistant.name || ASSISTANT_FAB_COPY.label}
            data-testid={ASSISTANT_FAB_TEST_IDS.fab}
            emphasis="primary"
            className="relative"
          >
            <ExponentialLogo variant="light" className="size-5 text-foreground" />
            {badge && badge.count > 0 && (
              <Badge
                count={badge.count}
                tone={badge.tone}
                data-testid={ASSISTANT_FAB_TEST_IDS.badge}
                className="absolute -top-0.5 -right-0.5"
              />
            )}
          </FabButton>
        </PopoverTrigger>
      </div>
      {/* #EXP-1876: the CONTENT CARD's own ground (never the menu surface the
          `in` bubbles and the composer card paint), the card hairline, the
          popover shadow, a 16px corner; sized off the card, resizable from
          its top / left edges and top-left corner (no grip, the cursor only). */}
      <PopoverContent
        side="top"
        align="end"
        sideOffset={8}
        data-testid={ASSISTANT_FAB_TEST_IDS.widget}
        // px literals: the md+ root font is not 16px (app-shell.ts).
        style={{ width: size.width, height: size.height, borderRadius: ASSISTANT_WIDGET.radius }}
        className="relative flex flex-col gap-0 overflow-hidden bg-glass-card-ground p-0 shadow-[0_12px_32px_-12px_oklch(0_0_0/0.45)]"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        {RESIZE_EDGES.map((edge) => (
          <div
            key={edge.testId}
            aria-hidden
            data-testid={edge.testId}
            className={`absolute touch-none ${edge.className}`}
            style={{
              ...(edge.axes.width ? { width: edge.axes.height ? ASSISTANT_WIDGET.handle * 2 : ASSISTANT_WIDGET.handle } : {}),
              ...(edge.axes.height ? { height: edge.axes.width ? ASSISTANT_WIDGET.handle * 2 : ASSISTANT_WIDGET.handle } : {}),
            }}
            onPointerDown={startResize(edge.axes)}
            onPointerMove={moveResize}
            onPointerUp={endResize}
            onPointerCancel={endResize}
          />
        ))}
        <div
          data-testid={ASSISTANT_FAB_TEST_IDS.header}
          className="flex h-11 shrink-0 items-center gap-2 border-b border-glass-stroke-card px-2"
          onDoubleClick={resetSize}
        >
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={ASSISTANT_FAB_COPY.maximize}
            title={ASSISTANT_FAB_COPY.maximize}
            data-testid={ASSISTANT_FAB_TEST_IDS.maximize}
            className="shrink-0 text-muted-foreground"
            onClick={maximize}
          >
            <MaximizeIcon className="size-4" />
          </Button>
          <ExponentialLogo variant="light" className="size-4 shrink-0 text-foreground" />
          <span className="min-w-0 flex-1 truncate text-sm font-semibold">{assistant.name}</span>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={ASSISTANT_FAB_COPY.minimize}
            title={ASSISTANT_FAB_COPY.minimize}
            data-testid={ASSISTANT_FAB_TEST_IDS.minimize}
            className="shrink-0 text-muted-foreground"
            onClick={() => setOpen(false)}
          >
            <MinimizeIcon className="size-4" />
          </Button>
        </div>
        <AssistantWidgetChat team={team} teamSlug={teamSlug} assistant={assistant} me={me} onLeave={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  )
}

/** The Assistant's Chat tab body at the widget's compact width: the root's
 *  own session view (transcript + THE composer), else the empty chat whose
 *  first send opens the conversation. */
function AssistantWidgetChat({
  team,
  teamSlug,
  assistant,
  me,
  onLeave,
}: {
  team: Team
  teamSlug: string
  assistant: AgentRow
  me: string | null
  /** A cross-session bubble navigates away: the widget closes. */
  onLeave: () => void
}) {
  const permissions = useTeamPermissions(team)
  const conversation = useMyAgentConversation(assistant.agentId, me)
  const chat = useSessionChat(conversation?.id ?? null)
  const nav = useSessionNav({ teamSlug, here: chat.root?.id ?? null, back: undefined })
  const suggestions = useSuggestionActions({ teamId: team.id, enabled: permissions.isMember, agentFor: () => chat.root?.cli ?? null })
  const { users } = useTeamUsers(team.id)
  const sessionName = useMemo(() => chatSessionNames(chat.view?.tabs ?? []), [chat.view?.tabs])
  const dockChildren = useMemo(
    () =>
      chat.rows
        .filter((row) => row.id !== chat.root?.id)
        .map((row) => ({ session: row, name: sessionName(row.id) ?? row.title ?? `` })),
    [chat.rows, chat.root?.id, sessionName]
  )

  if (conversation && chat.root) {
    return (
      <InPlaceSession
        key={chat.root.id}
        row={chat.root}
        origin={chat.view?.origin ?? null}
        surface="agent"
        browseTeamId={team.id}
        onOpenXref={(sessionId) => {
          onLeave()
          nav.open(sessionId)
        }}
        chat={{ tree: chat.rows, dockChildren, suggestions }}
      />
    )
  }
  if (conversation) {
    return (
      <div className="p-4">
        <SkeletonRows />
      </div>
    )
  }
  const composer = permissions.isMember ? (
    <SessionComposer
      teamId={assistant.teamId}
      browseTeamId={team.id}
      sessionId={null}
      agentId={assistant.agentId}
      surface="agent"
      placeholder={`Message ${assistant.name}`}
      users={users as User[]}
      testId="agent-composer"
    />
  ) : null
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div className="relative min-h-0 flex-1">
        <div data-testid="chat-scroll" className="h-full" />
        {composer && (
          <ChatDock
            rows={[]}
            sessionName={() => null}
            renderText={renderChatText}
            onAnswer={() => undefined}
            onDecide={() => undefined}
            composer={composer}
            onHeight={() => undefined}
          />
        )}
      </div>
    </div>
  )
}
