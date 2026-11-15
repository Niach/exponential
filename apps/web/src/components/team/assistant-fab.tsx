import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import {
  ASSISTANT_FAB,
  ASSISTANT_FAB_BUBBLE,
  ASSISTANT_FAB_COPY,
  ASSISTANT_FAB_ENTRANCE,
  ASSISTANT_WIDGET_SIZE,
  assistantFabBubble,
  assistantFabVisible,
  type AssistantFabWaitingCard,
} from "@exp/domain-contract/agent-settings"
import { chatSessionNames } from "@exp/domain-contract/ask-dock"
import { askPreview, CHATS_COPY } from "@exp/domain-contract/chats"
import type { TreeMessage } from "@exp/domain-contract/session-tree"
import {
  Badge,
  Button,
  ChatDock,
  conceptIcon,
  ExponentialLogo,
  FabButton,
  MessageBubble,
  Popover,
  PopoverContent,
  PopoverTrigger,
  SkeletonRows,
  useIsMobile,
  useMediaQuery,
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
import { useAgents, useDigestMessages, useSessionChat, useSessionReads, useTeamSessions } from "@/hooks/use-session-tree"
import { useSuggestionActions } from "@/hooks/use-suggestion-actions"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { AgentRow, SessionRow } from "@/lib/session-tree"

// #EXP-1837 (agent-settings.json `assistant.fab`): on wide web the viewer's
// Assistant lives in THE FAB bottom-right of the content card, not in the
// sidebar: #EXP-1876 a `size` (60) px circle that IS the brand mark (no
// padding) with THE count badge `rowBadge` of the conversation. A press opens the widget — a
// popover over the page anchored to the FAB holding the REAL chat (the root's
// transcript + THE composer, no tabs) — with ⇱ maximize (the Assistant's chat
// page) and — minimize. Hidden on the Assistant's own page; phones keep it
// in Chats. #EXP-1876: the first show on a device plays THE entrance
// (`assistant.fab.entrance`); THE speech bubble (`assistant.fab.bubble`,
// `assistantFabBubble`) pops above it: the welcome once per device, a
// waiting plan / question once per card.

const MaximizeIcon = conceptIcon(`ui-maximize`)
const MinimizeIcon = conceptIcon(`ui-minimize`)
// `assistant.fab.bubble.kickerIcon`: the needs-you concept.
const NeedsYouIcon = conceptIcon(`nav-needs-you`)
const REDUCED_MOTION = `(prefers-reduced-motion: reduce)`
/** tokens.json `motion.ease.decelerate` (styles.css `--motion-ease-decelerate`):
 *  the Web Animations API takes no CSS variables. */
const EASE_DECELERATE = `cubic-bezier(0, 0, 0.2, 1)`

export const ASSISTANT_FAB_TEST_IDS = {
  fab: `assistant-fab`,
  badge: `assistant-fab-badge`,
  widget: `assistant-widget`,
  maximize: `assistant-widget-maximize`,
  minimize: `assistant-widget-minimize`,
  bubble: `assistant-fab-bubble`,
} as const

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
  const fabRef = useRef<HTMLButtonElement>(null)
  useFabEntrance(fabRef, me)
  const speech = useFabSpeech({ assistant, me, widgetOpen: open })
  // The toasts rise above the FAB, and above the bubble while it shows.
  useToastLift(ASSISTANT_FAB.toastLift)
  useToastLift(speech.lift)
  const maximize = () => {
    setOpen(false)
    void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId: assistant.agentId } })
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      {/* #EXP-1860: `data-assistant-fab` reserves the corner (@exp/ui
          styles.css): page scroll content ends clear of it, THE Dock keeps
          its composer beside it. */}
      <div
        data-assistant-fab=""
        className="pointer-events-none absolute z-20"
        style={{ right: ASSISTANT_FAB.inset, bottom: ASSISTANT_FAB.inset }}
      >
        {speech.bubble && (
          <FabSpeechBubble
            bubble={speech.bubble}
            measureRef={speech.measureRef}
            onHover={speech.pause}
            onPress={() => {
              speech.dismiss()
              setOpen(true)
            }}
          />
        )}
        <PopoverTrigger asChild>
          {/* #EXP-1876: the mark IS the button: the disc fills the whole
              circle, no padding, no glass chrome around it; look (b) =
              `.fab-mark` (the ground + the lift) and the sheen on the mark
              (genUI Button `fab` `lg`). */}
          <FabButton
            ref={fabRef}
            aria-label={assistant.name || ASSISTANT_FAB_COPY.label}
            title={assistant.name || ASSISTANT_FAB_COPY.label}
            data-testid={ASSISTANT_FAB_TEST_IDS.fab}
            emphasis="primary"
            className="fab-mark relative border-0 backdrop-blur-none active:opacity-85"
            style={{ width: ASSISTANT_FAB.size, height: ASSISTANT_FAB.size }}
          >
            <ExponentialLogo variant="light" sheen size={ASSISTANT_FAB.size} className="size-full text-foreground" />
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
      <PopoverContent
        side="top"
        align="end"
        sideOffset={8}
        data-testid={ASSISTANT_FAB_TEST_IDS.widget}
        // px literals: the md+ root font is not 16px (app-shell.ts).
        style={{ width: ASSISTANT_WIDGET_SIZE.width, height: ASSISTANT_WIDGET_SIZE.height }}
        className="flex max-h-[calc(100dvh-120px)] max-w-[calc(100vw-40px)] flex-col gap-0 overflow-hidden p-0"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <div className="flex h-11 shrink-0 items-center gap-2 border-b border-glass-stroke-card px-2">
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

type FabBubbleView =
  | { kind: `welcome`; text: string }
  | { kind: `needsYou`; messageId: string; cardKind: `plan` | `question`; text: string }

const KICKER = { plan: ASSISTANT_FAB_BUBBLE.copy.plan, question: ASSISTANT_FAB_BUBBLE.copy.question } as const
const WAITING_CARD_KINDS = new Set([`plan`, `question`, `ask`])

/** #EXP-1876: what this device remembers about the FAB, per member
 *  (localStorage): the entrance played, the welcome seen, the waiting cards
 *  already bubbled (by message id, the newest 100). */
const deviceKey = (me: string | null, what: string) => `exp.assistantFab.${what}.${me ?? `anon`}`
function readFlag(key: string): boolean {
  try {
    return window.localStorage.getItem(key) === `1`
  } catch {
    return true
  }
}
function writeFlag(key: string): void {
  try {
    window.localStorage.setItem(key, `1`)
  } catch {
    // Private mode: the entrance / welcome simply replays.
  }
}
function readList(key: string): string[] {
  try {
    const value = JSON.parse(window.localStorage.getItem(key) ?? `[]`) as unknown
    return Array.isArray(value) ? value.filter((id): id is string => typeof id === `string`) : []
  } catch {
    return []
  }
}

/** #EXP-1876 THE ENTRANCE (`assistant.fab.entrance`): the first show on a
 *  device rises + scales 0.6 → 1.06 → 1 with a fade, then one soft ring
 *  ripple; reduced motion = a short fade only. Every later show is instant. */
function useFabEntrance(ref: RefObject<HTMLButtonElement | null>, me: string | null) {
  useEffect(() => {
    const el = ref.current
    const key = deviceKey(me, `entered`)
    if (!el || readFlag(key)) return
    writeFlag(key)
    if (typeof el.animate !== `function`) return
    const spec = ASSISTANT_FAB_ENTRANCE
    if (typeof window.matchMedia === `function` && window.matchMedia(REDUCED_MOTION).matches) {
      el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: spec.reducedMotion.fadeMs, easing: `linear` })
      return
    }
    const [from, over, to] = spec.scale
    const [, overAt, toAt] = spec.scaleTimes
    el.animate(
      [
        { offset: 0, opacity: 0, transform: `translateY(${spec.riseY}px) scale(${from})` },
        { offset: overAt, opacity: 1, transform: `translateY(0) scale(${over})` },
        { offset: toAt, opacity: 1, transform: `translateY(0) scale(${to})` },
        { offset: 1, opacity: 1, transform: `translateY(0) scale(${to})` },
      ],
      { duration: spec.durationMs, easing: EASE_DECELERATE }
    )
    // The one soft ring ripple, from the disc's edge outwards.
    const ring = document.createElement(`span`)
    ring.setAttribute(`aria-hidden`, `true`)
    ring.className = `pointer-events-none absolute inset-0 rounded-full border-2 border-current opacity-0`
    el.appendChild(ring)
    const grow = (ASSISTANT_FAB.size + spec.ripple.spreadPx * 2) / ASSISTANT_FAB.size
    const ripple = ring.animate(
      [
        { opacity: 0.45, transform: `scale(1)` },
        { opacity: 0, transform: `scale(${grow})` },
      ],
      { duration: spec.ripple.durationMs, delay: spec.ripple.startMs, easing: EASE_DECELERATE }
    )
    ripple.onfinish = () => ring.remove()
    ripple.oncancel = () => ring.remove()
  }, [ref, me])
}

/** The window has focus, live. */
function useWindowFocused(): boolean {
  const [focused, setFocused] = useState(() => (typeof document === `undefined` ? true : document.hasFocus()))
  useEffect(() => {
    const on = () => setFocused(true)
    const off = () => setFocused(false)
    window.addEventListener(`focus`, on)
    window.addEventListener(`blur`, off)
    return () => {
      window.removeEventListener(`focus`, on)
      window.removeEventListener(`blur`, off)
    }
  }, [])
  return focused
}

/** The Assistant conversation tree's waiting cards (an unanswered plan /
 *  question / ask card on a session that waits on the viewer): what turns
 *  the FAB's badge yellow. */
function useWaitingCards(conversation: SessionRow | null): { cards: AssistantFabWaitingCard[]; byId: Map<string, TreeMessage> } {
  const { tree } = useTeamSessions(conversation?.teamId ?? null)
  const waiting = useMemo(
    () => (conversation ? tree.filter((s) => s.rootId === conversation.id && s.needsInput).map((s) => s.id) : []),
    [tree, conversation]
  )
  const { messages } = useDigestMessages({ sessionIds: waiting })
  return useMemo(() => {
    const open = messages.filter(
      (m) => m.answeredAt === null && m.card !== null && WAITING_CARD_KINDS.has(String(m.card.kind))
    )
    return {
      cards: open.map((m) => ({ messageId: m.id, sessionId: m.sessionId, createdAt: m.createdAt })),
      byId: new Map(open.map((m) => [m.id, m])),
    }
  }, [messages])
}

/** The bubble's kicker + text of a waiting card: the chat list's preview
 *  of it without its `Plan: ` / `Question: ` lead. */
export function waitingCardView(message: TreeMessage): { cardKind: `plan` | `question`; text: string } {
  if (message.locked) return { cardKind: message.card?.kind === `plan` ? `plan` : `question`, text: `` }
  const preview = askPreview(message)
  const lead = CHATS_COPY.preview[preview.kind].split(`{text}`)[0] ?? ``
  return { cardKind: preview.kind, text: preview.text.startsWith(lead) ? preview.text.slice(lead.length) : preview.text }
}

/** #EXP-1876 THE SPEECH BUBBLE's state (`assistant.fab.bubble`): decides
 *  with `assistantFabBubble`, remembers what this device bubbled, holds the
 *  shown bubble for `stayMs` (hover pauses) and reports the toast lift. */
function useFabSpeech({ assistant, me, widgetOpen }: { assistant: AgentRow; me: string | null; widgetOpen: boolean }) {
  const conversation = useMyAgentConversation(assistant.agentId, me)
  const { cards, byId } = useWaitingCards(conversation)
  const reads = useSessionReads()
  const focused = useWindowFocused()
  const [welcomeSeen, setWelcomeSeen] = useState(() => readFlag(deviceKey(me, `welcomed`)))
  const [bubbled, setBubbled] = useState(() => readList(deviceKey(me, `bubbled`)))
  const [bubble, setBubble] = useState<FabBubbleView | null>(null)
  const [paused, setPaused] = useState(false)
  const [height, setHeight] = useState(0)
  const observer = useRef<ResizeObserver | null>(null)

  const decision = useMemo(
    () =>
      assistantFabBubble({
        welcomeSeen,
        cards,
        reads,
        bubbled,
        widgetOpen,
        // The FAB is not mounted on the Assistant's own page.
        onAssistantPage: false,
        focused,
      }),
    [welcomeSeen, cards, reads, bubbled, widgetOpen, focused]
  )

  useEffect(() => {
    if (!decision) return
    // Any bubble shown spends the welcome.
    writeFlag(deviceKey(me, `welcomed`))
    setWelcomeSeen(true)
    if (decision.kind === `welcome`) {
      const lines = ASSISTANT_FAB_BUBBLE.welcome
      const line = lines[Math.floor(Math.random() * lines.length)] ?? lines[0]!
      setBubble({ kind: `welcome`, text: line.en })
      return
    }
    const message = byId.get(decision.messageId)
    const shown = cards.find((card) => card.messageId === decision.messageId)
    if (!message || !shown) return
    // A burst bubbles once: the shown card and every older waiting one.
    const older = cards
      .filter((card) => Date.parse(card.createdAt) <= Date.parse(shown.createdAt))
      .map((card) => card.messageId)
    const next = [...new Set([...bubbled, ...older])].slice(-100)
    try {
      window.localStorage.setItem(deviceKey(me, `bubbled`), JSON.stringify(next))
    } catch {
      // Private mode: kept for this page's life.
    }
    setBubbled(next)
    setBubble({ kind: `needsYou`, messageId: decision.messageId, ...waitingCardView(message) })
  }, [decision, byId, cards, bubbled, me])

  // The widget opening (or the card being answered / read) takes it away.
  const stillWaiting = bubble?.kind !== `needsYou` || byId.has(bubble.messageId)
  useEffect(() => {
    if (widgetOpen || !stillWaiting) setBubble(null)
  }, [widgetOpen, stillWaiting])

  // `stayMs`, paused while hovered (the full time again after a hover).
  useEffect(() => {
    if (!bubble || paused) return
    const stay = bubble.kind === `welcome` ? ASSISTANT_FAB_BUBBLE.stayMs.welcome : ASSISTANT_FAB_BUBBLE.stayMs.needsYou
    const timer = window.setTimeout(() => setBubble(null), stay)
    return () => window.clearTimeout(timer)
  }, [bubble, paused])

  const measureRef = useCallback((el: HTMLDivElement | null) => {
    observer.current?.disconnect()
    observer.current = null
    if (!el) {
      setHeight(0)
      return
    }
    setHeight(el.offsetHeight)
    if (typeof ResizeObserver === `function`) {
      observer.current = new ResizeObserver(() => setHeight(el.offsetHeight))
      observer.current.observe(el)
    }
  }, [])

  return {
    bubble,
    measureRef,
    pause: setPaused,
    dismiss: () => setBubble(null),
    lift: bubble && height > 0 ? ASSISTANT_FAB.toastLift + height + ASSISTANT_FAB_BUBBLE.gap : 0,
  }
}

/** #EXP-1876 THE SPEECH BUBBLE: THE MessageBubble (side `in`) above the
 *  FAB, right-aligned with it, popping in from its tail; a press opens the
 *  widget (the waiting card sits in its Dock). */
function FabSpeechBubble({
  bubble,
  measureRef,
  onHover,
  onPress,
}: {
  bubble: FabBubbleView
  measureRef: (el: HTMLDivElement | null) => void
  onHover: (hovered: boolean) => void
  onPress: () => void
}) {
  const reduced = useMediaQuery(REDUCED_MOTION)
  const key = bubble.kind === `welcome` ? `welcome` : bubble.messageId
  const popRef = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    const el = popRef.current
    if (!el || typeof el.animate !== `function`) return
    const from = ASSISTANT_FAB_BUBBLE.popFromScale
    el.animate(
      reduced
        ? [{ opacity: 0 }, { opacity: 1 }]
        : [
            { opacity: 0, transform: `scale(${from})` },
            { opacity: 1, transform: `scale(1)` },
          ],
      { duration: ASSISTANT_FAB_BUBBLE.popInMs, easing: EASE_DECELERATE }
    )
  }, [key, reduced])
  return (
    <div
      ref={(el) => {
        popRef.current = el
        measureRef(el)
      }}
      role="button"
      tabIndex={0}
      data-testid={ASSISTANT_FAB_TEST_IDS.bubble}
      data-kind={bubble.kind}
      className="pointer-events-auto absolute right-0 origin-bottom-right cursor-pointer outline-none"
      style={{ bottom: ASSISTANT_FAB.size + ASSISTANT_FAB_BUBBLE.gap, width: `max-content`, maxWidth: ASSISTANT_FAB_BUBBLE.maxWidth }}
      onMouseEnter={() => onHover(true)}
      onMouseLeave={() => onHover(false)}
      onClick={onPress}
      onKeyDown={(event) => {
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          onPress()
        }
      }}
    >
      {/* TODO(tail-down): `tail="down"` (PR 1 of #EXP-1876) once MessageBubble takes it. */}
      <MessageBubble side="in" className="items-end [&>*]:max-w-full">
        {bubble.kind === `needsYou` ? (
          <div className="flex items-center gap-1 text-xs font-medium text-yellow-400">
            <NeedsYouIcon className="size-3.5 shrink-0" aria-hidden />
            <span>{KICKER[bubble.cardKind]}</span>
          </div>
        ) : null}
        {bubble.text ? (
          <p className="overflow-hidden" style={{ display: `-webkit-box`, WebkitBoxOrient: `vertical`, WebkitLineClamp: ASSISTANT_FAB_BUBBLE.maxLines }}>
            {bubble.text}
          </p>
        ) : null}
      </MessageBubble>
    </div>
  )
}
