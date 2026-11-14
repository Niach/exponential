import { useMemo, useState } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import {
  ASSISTANT_FAB,
  ASSISTANT_FAB_COPY,
  ASSISTANT_WIDGET_SIZE,
  assistantFabVisible,
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
  PopoverAnchor,
  PopoverContent,
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
  maximize: `assistant-widget-maximize`,
  minimize: `assistant-widget-minimize`,
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
  useToastLift(ASSISTANT_FAB.toastLift)
  const maximize = () => {
    setOpen(false)
    void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId: assistant.agentId } })
  }
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverAnchor asChild>
        <div
          className="pointer-events-none absolute z-20"
          style={{ right: ASSISTANT_FAB.inset, bottom: ASSISTANT_FAB.inset }}
        >
          <FabButton
            aria-label={assistant.name || ASSISTANT_FAB_COPY.label}
            aria-expanded={open}
            title={assistant.name || ASSISTANT_FAB_COPY.label}
            data-testid={ASSISTANT_FAB_TEST_IDS.fab}
            emphasis="primary"
            className="relative"
            onClick={() => setOpen((value) => !value)}
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
        </div>
      </PopoverAnchor>
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
