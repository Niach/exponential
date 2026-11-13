import { useEffect } from "react"
import type { ComponentProps, ReactNode } from "react"
import { viaAppCaption } from "@exp/domain-contract/via-app"
import { useAppName } from "@/hooks/use-apps"
import { Link } from "@tanstack/react-router"
import {
  Button,
  CardHighlightContext,
  conceptIcon,
  FloatingComposer,
  useIsMobile,
  CHAT_COLUMN_CLASS,
  WORK_GUTTER_CLASS,
} from "@exp/ui"
import type { IconConcept } from "@exp/icons"
import { AgentSessionView } from "@/components/agent-session"
import { SessionComposer, SessionComposerReplies } from "@/components/session-composer"
import {
  useSessionCard,
  useSessionRow,
  useSessionChat,
} from "@/hooks/use-session-tree"
import { useAgentRow } from "@/hooks/use-agent-row"
import { DeepLinkContext, useDeepLinkHost } from "@/hooks/use-deep-link"
import { useSessionNav } from "@/hooks/use-session-nav"
import type { SessionXref as SessionXrefModel } from "@exp/domain-contract/session-tree"
import { useMarkRead, useNewestAgentMessageAt } from "@/hooks/use-mark-read"
import { useTeamUsers } from "@/hooks/use-team-data"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { CHAT_COPY, type SessionRow } from "@/lib/session-tree"
import { cn } from "@/lib/utils"

// EXP-1293 / EXP-1316: ONE session's RAW WORK. Two hosts:
//
//   InPlaceSession   #EXP-1732: an opened session on THE session screen
//                    (`?tab=sessions&open=session&session=<id>`, the tab
//                    pane wide, a pushed page on phones); S-3: no Stop pill
//   SessionScreen    sessions under a team AGENT (`/sessions/$id`): its own header,
//                    back to the chat (or the agent's channel), the
//                    session's name
//
// The feed always shows the work (no faces, no Show work, no Guide), and the
// reply box posts to THIS session. Visibility is the shape's. EXP-1323:
// opening either marks THIS session read (`useOpenedSessionRead`).

const BackIcon = conceptIcon(`ui-back`)
const PopIcon = conceptIcon(`ui-chevron-left`)

/** VAPP-229 (via-app.json): a session an app's tool started reads one
 *  muted line `via <app>` in its view's header (none without one). */
export function SessionViaApp({ viaAppId, className }: { viaAppId: string | null | undefined; className?: string }) {
  const caption = viaAppCaption(useAppName(viaAppId))
  if (!caption) return null
  return (
    <div className={cn(CHAT_COLUMN_CLASS, WORK_GUTTER_CLASS, `shrink-0 truncate pt-1 text-xs text-muted-foreground`, className)} data-testid="session-via-app">
      {caption}
    </div>
  )
}

/** S-3: the header's `‹` popping the route's `back` stack (absent = none). */
export function BackStackButton({ onPop, className }: { onPop?: () => void; className?: string }) {
  if (!onPop) return null
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Back"
      data-testid="chat-back"
      onClick={onPop}
      className={cn(`shrink-0 text-muted-foreground`, className)}
    >
      <PopIcon className="size-4" />
    </Button>
  )
}
/** session-chat.json: THE session composer's placeholder ×4. */
const REPLY_PLACEHOLDER = CHAT_COPY.placeholder

/** The reply box under a session's feed. */
function SessionReply({
  row,
  opaque = false,
  surface,
  browseTeamId,
}: {
  row: SessionRow
  opaque?: boolean
  /** The team the viewer browses (the `/` prompts; the Assistant's lives in
   *  their personal team). */
  browseTeamId?: string
  /** The + menu's surface (an agent's conversation: `agent`). */
  surface?: `agent` | `session`
}) {
  const { users } = useTeamUsers(row.teamId)
  const live = row.status !== `ended`
  return (
    <SessionComposer
      teamId={row.teamId}
      sessionId={row.id}
      placeholder={REPLY_PLACEHOLDER}
      users={users}
      busy={row.busy}
      canStop={live}
      opaque={opaque}
      hideReplies={opaque}
      testId="session-composer"
      {...(surface ? { surface } : {})}
      {...(browseTeamId ? { browseTeamId } : {})}
    />
  )
}

/** EXP-1323: an open session view marks the session read, again whenever
 *  its newer agent reply lands while it stays open. */
function useOpenedSessionRead(sessionId: string) {
  useMarkRead(sessionId, useNewestAgentMessageAt([sessionId]))
}

/** #EXP-1732: a session's OWN view on THE session screen: the ROOT's is
 *  the Chat tab (every kind), a descendant's the opened Sessions row (the
 *  tab pane wide, a pushed item on phones); phones reply through THE
 *  floating composer capsule. */
export function InPlaceSession({
  row,
  topInset = 0,
  floating = false,
  origin = null,
  onOpenXref,
  surface,
  composer,
  browseTeamId,
  chat,
}: {
  browseTeamId?: string
  /** #EXP-1732: the Chat tab's extras (the ROOT's view): the tree, its
   *  issues, the Dock's children, the suggestions' actions. */
  chat?: Pick<ComponentProps<typeof AgentSessionView>, `tree` | `issues` | `dockChildren` | `suggestions` | `renderIssueRef`>
  /** The reply box's + menu surface (an agent's conversation: `agent`). */
  surface?: `agent` | `session`
  /** Replaces the reply box (an app editor's, with its device menu);
   *  `opaque` = it floats in the phone capsule. */
  composer?: (opaque: boolean) => ReactNode
  row: SessionRow
  /** Phones: the overlay header band's height. */
  topInset?: number
  floating?: boolean
  /** S-3: who started it (the chat view's `origin` with it in focus):
   *  #EXP-1736 its first prompt reads as that session's bubble. */
  origin?: SessionXrefModel | null
  /** #EXP-1736: a cross-session bubble opens that session (the back stack). */
  onOpenXref?: (sessionId: string) => void
}) {
  useOpenedSessionRead(row.id)
  let header: ReactNode = topInset ? <div aria-hidden className="shrink-0" style={{ height: topInset }} /> : undefined
  if (row.viaAppId) {
    header = (
      <>
        {header}
        <SessionViaApp viaAppId={row.viaAppId} />
      </>
    )
  }
  return (
    <div className="min-h-0 flex-1" data-testid="chat-session-body">
      <AgentSessionView
        key={row.id}
        session={row}
        header={header}
        composerFloats={floating}
        origin={origin}
        onOpenSession={onOpenXref}
        {...chat}
        composer={
          floating ? (
            <FloatingComposer
              placeholder={REPLY_PLACEHOLDER}
              above={<SessionComposerReplies sessionId={row.id} />}
            >
              {composer ? composer(true) : <SessionReply row={row} opaque surface={surface} browseTeamId={browseTeamId} />}
            </FloatingComposer>
          ) : composer ? (
            composer(false)
          ) : (
            <SessionReply row={row} surface={surface} browseTeamId={browseTeamId} />
          )
        }
      />
    </div>
  )
}

/** The session on its own screen (phones; sessions under an agent). */
export function SessionScreen({
  teamSlug,
  row,
  back,
  card: cardId,
}: {
  teamSlug: string
  row: SessionRow
  /** S-3: the route's `back` stack. */
  back?: string
  /** waiting-cards.json DEEP LINKS: `?card=`, a card of this session. */
  card?: string
}) {
  useOpenedSessionRead(row.id)
  const deepLinkHost = useDeepLinkHost()
  const takeDeepLink = deepLinkHost.take
  useEffect(() => {
    if (cardId) takeDeepLink(cardId, row.id)
  }, [cardId, row.id, takeDeepLink])
  const isMobile = useIsMobile()
  const rootId = row.rootId ?? row.id
  const isRoot = rootId === row.id
  const { row: syncedRoot } = useSessionRow(rootId === row.id ? null : rootId)
  const rootRow = isRoot ? row : syncedRoot
  // Under an agent's conversation (NEW-2: a root with `agentId`) the way
  // back is the agent's page, never /chats.
  const rootAgentId = rootRow?.agentId ?? null
  const { agent: rootAgent } = useAgentRow(rootAgentId)
  const card = useSessionCard(row)
  const { view } = useSessionChat(rootId, row.id)
  const nav = useSessionNav({ teamSlug, here: row.id, back })
  const chatTitle = view?.title ?? null
  // EXP-1341: the card's `sessionTitle`, else the chat's; never a generic
  // word (both hold while their shapes load).
  const name = card?.name ?? (isRoot ? chatTitle : null)
  usePageTitle(name ? pageTitle(name, isRoot ? null : chatTitle) : undefined)

  const CardIcon = conceptIcon((card?.icon ?? `coding-assistant`) as IconConcept)

  const header = (
    <div
      className={cn(CHAT_COLUMN_CLASS, WORK_GUTTER_CLASS, `shrink-0 pt-4 pb-2`)}
      data-testid="session-header"
    >
      <div className="flex min-w-0 items-center gap-1">
      <BackStackButton onPop={nav.pop} className="-ml-1.5" />
      {rootAgentId ? (
        <Link
          to="/t/$teamSlug/agents/$agentId"
          params={{ teamSlug, agentId: rootAgentId }}
          className="inline-flex max-w-full min-w-0 items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
          data-testid="session-back"
        >
          <BackIcon className="size-4 shrink-0" />
          <span className="truncate">{rootAgent?.name || `Back`}</span>
        </Link>
      ) : !isRoot || chatTitle ? (
        <Link
          to="/t/$teamSlug/sessions/$sessionId"
          params={{ teamSlug, sessionId: rootId }}
          className="inline-flex max-w-full min-w-0 items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
          data-testid="session-back"
        >
          <BackIcon className="size-4 shrink-0" />
          <span className="truncate">{chatTitle ?? `Back`}</span>
        </Link>
      ) : null}
      </div>
      <div className="mt-3 flex min-w-0 items-center gap-3">
        <CardIcon className="size-5 shrink-0 text-muted-foreground" />
        <h1
          className="min-w-0 flex-1 truncate text-xl font-semibold"
          data-testid="session-title"
        >
          {name}
        </h1>
      </div>
      <SessionViaApp viaAppId={row.viaAppId} className="px-0" />
    </div>
  )

  return (
    <DeepLinkContext.Provider value={deepLinkHost}>
    <CardHighlightContext.Provider value={deepLinkHost.highlight}>
    <AgentSessionView
      session={row}
      header={header}
      origin={view?.origin ?? null}
      onOpenSession={nav.open}
      // EXP-1316: phones reply through THE floating composer capsule.
      composerFloats={isMobile}
      composer={
        isMobile ? (
          <FloatingComposer
              placeholder={REPLY_PLACEHOLDER}
              above={<SessionComposerReplies sessionId={row.id} />}
            >
            <SessionReply row={row} opaque />
          </FloatingComposer>
        ) : (
          <SessionReply row={row} />
        )
      }
    />
    </CardHighlightContext.Provider>
    </DeepLinkContext.Provider>
  )
}
