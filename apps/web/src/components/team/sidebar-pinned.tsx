import { useMemo } from "react"
import type * as React from "react"
import { Link, useLocation, useParams, useSearch } from "@tanstack/react-router"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { conceptIcon } from "@/lib/icons.generated"
import { getActionIcon } from "@/lib/board-icons"
import type { Board, Issue, Pin, SyncedAction } from "@/db/schema"
import { actionCollection, issueCollection } from "@/lib/collections"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useTeamPins } from "@/hooks/use-pins"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// EXP-778: the sidebar's Pinned group — the caller's favourites in this team
// (issues and actions), in pin order. A row renders only when its target is
// synced: the pins shape is per user and never trash-scoped, so an issue on a
// trashed board simply has no row until it comes back. Sits between the nav
// entries and Boards, hidden when empty; hovering a row offers Unpin.
//
// EXP-877: a pinned SESSION draws nothing any more. A live run of mine is
// already a top tab (`work-tabs-strip.tsx`) and an ended one belongs to the
// Agent page's Past list, so a second copy in the sidebar said the same thing
// twice. `pinKind.session` stays in the contract and old rows stay in the
// database — they simply resolve to no row.

const UiUnpinIcon = conceptIcon(`ui-unpin`)
const NavIssuesIcon = conceptIcon(`nav-issues`)

// EXP-862: the sidebar's compact density — 28px rows (the twin of
// `SIDEBAR_ROW_COMPACT` in `sidebar.tsx`; the two files cannot import from
// each other without a cycle).
const PINNED_ROW_COMPACT = `h-7 text-sm`

export function SidebarPinned({
  teamId,
  teamSlug,
}: {
  teamId: string
  teamSlug: string
}) {
  const pins = useTeamPins(teamId)
  if (pins.length === 0) return null
  return <PinnedRows pins={pins} teamId={teamId} teamSlug={teamSlug} />
}

/** One pin whose target resolved — the rows and the compact rail's icons
 *  render the same list. */
type ResolvedPin =
  | { kind: `issue`; pin: Pin; issue: Issue; board: Board; active: boolean }
  | { kind: `action`; pin: Pin; action: SyncedAction; active: boolean }

/** EXP-870: resolve the caller's pins against the synced targets, once, for
 *  both rail states. A pin with no synced target yields nothing. */
function useResolvedPins(pins: Pin[], teamId: string): ResolvedPin[] {
  const boards = useTeamBoards(teamId)
  const issueIds = useMemo(
    () =>
      [
        ...new Set(pins.flatMap((pin) => (pin.issueId ? [pin.issueId] : []))),
      ].sort(),
    [pins]
  )
  const { data: issues } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueIds]
  )
  const { data: actions } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )
  const { issueIdentifier: routeIssueIdentifier } = useParams({ strict: false })
  // EXP-862: a pinned ACTION row is active while the composer is seeded with
  // it — the Agent page's `?action=` (the desktop's `active_chat_action`).
  // The Agent nav entry drops its own highlight for the same reason
  // (`sidebar.tsx`), so exactly one row claims the page.
  const onAgentPage = useLocation({
    select: (current) => current.pathname.endsWith(`/agent`),
  })
  const composerActionId = useSearch({
    strict: false,
    select: (search) => {
      const value = (search as { action?: unknown }).action
      return typeof value === `string` && value !== `` ? value : null
    },
  })
  const seededActionId = onAgentPage ? composerActionId : null

  return useMemo(() => {
    const boardsById = new Map((boards ?? []).map((board) => [board.id, board]))
    const issuesById = new Map(
      ((issues ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const actionsById = new Map(
      ((actions ?? []) as SyncedAction[]).map((action) => [action.id, action])
    )
    return pins.flatMap((pin): ResolvedPin[] => {
      if (pin.kind === `issue` && pin.issueId) {
        const issue = issuesById.get(pin.issueId)
        const board = issue ? boardsById.get(issue.boardId) : undefined
        if (!issue || !board) return []
        return [
          {
            kind: `issue`,
            pin,
            issue,
            board,
            active: routeIssueIdentifier === issue.identifier,
          },
        ]
      }
      if (pin.kind === `action` && pin.actionId) {
        const action = actionsById.get(pin.actionId)
        if (!action) return []
        return [
          { kind: `action`, pin, action, active: seededActionId === action.id },
        ]
      }
      return []
    })
  }, [pins, boards, issues, actions, routeIssueIdentifier, seededActionId])
}

function PinnedRows({
  pins,
  teamId,
  teamSlug,
}: {
  pins: Pin[]
  teamId: string
  teamSlug: string
}) {
  const resolved = useResolvedPins(pins, teamId)
  const openComposer = useOpenComposer()

  const unpin = (pin: Pin) => {
    const targetId = pin.issueId ?? pin.actionId
    if (!targetId) return
    void trpc.pins.toggle.mutate({ teamId, kind: pin.kind, targetId })
  }

  const items = resolved.map((entry) => {
    if (entry.kind === `issue`) {
      const { pin, issue, board } = entry
      return (
        <SidebarMenuItem key={pin.id}>
          <SidebarMenuButton
            asChild
            className={PINNED_ROW_COMPACT}
            isActive={entry.active}
          >
            {/* EXP-870: no `from` — a pinned issue opens full-width. */}
            <Link
              to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
              params={{
                teamSlug,
                boardSlug: board.slug,
                issueIdentifier: issue.identifier,
              }}
            >
              <NavIssuesIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
              <span className="shrink-0 font-mono text-xs text-muted-foreground">
                {issue.identifier}
              </span>
              <span className="min-w-0 flex-1 truncate">{issue.title}</span>
            </Link>
          </SidebarMenuButton>
          <UnpinAction label={issue.identifier} onClick={() => unpin(pin)} />
        </SidebarMenuItem>
      )
    }
    const { pin, action } = entry
    const ActionIcon = getActionIcon(action)
    return (
      <SidebarMenuItem key={pin.id}>
        <SidebarMenuButton
          className={PINNED_ROW_COMPACT}
          isActive={entry.active}
          // EXP-870: context-free like the pinned issue rows — the composer
          // opens full-width, no list nav.
          onClick={() => openComposer({ actionId: action.id }, { origin: null })}
          title={`Run ${action.name}`}
        >
          <ActionIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate">{action.name}</span>
        </SidebarMenuButton>
        <UnpinAction label={action.name} onClick={() => unpin(pin)} />
      </SidebarMenuItem>
    )
  })

  if (items.length === 0) return null
  return (
    <SidebarGroup>
      <SidebarGroupLabel>Pinned</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>{items}</SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** EXP-870: the compact rail's pinned icons — the same resolved pins as the
 *  rows, one 32px icon each with the name in a tooltip. Context-free like
 *  the rows: nothing here ever brings a list nav along. */
export function SidebarPinnedIcons({
  teamId,
  teamSlug,
  renderItem,
  separator,
}: {
  teamId: string
  teamSlug: string
  /** Drawn above the icons, only when there are any. */
  separator?: React.ReactNode
  /** The rail's own button shell (size, tooltip, active fill). */
  renderItem: (item: {
    key: string
    label: string
    active: boolean
    icon: React.ReactNode
    link?: {
      to: string
      params: Record<string, string>
    }
    onClick?: () => void
  }) => React.ReactNode
}) {
  const pins = useTeamPins(teamId)
  const resolved = useResolvedPins(pins, teamId)
  const openComposer = useOpenComposer()
  if (resolved.length === 0) return null
  return (
    <>
      {separator}
      {resolved.map((entry) => {
        if (entry.kind === `issue`) {
          return renderItem({
            key: entry.pin.id,
            label: `${entry.issue.identifier} ${entry.issue.title}`,
            active: entry.active,
            icon: <NavIssuesIcon className="size-4" />,
            link: {
              to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
              params: {
                teamSlug,
                boardSlug: entry.board.slug,
                issueIdentifier: entry.issue.identifier,
              },
            },
          })
        }
        const ActionIcon = getActionIcon(entry.action)
        return renderItem({
          key: entry.pin.id,
          label: entry.action.name,
          active: entry.active,
          icon: <ActionIcon className="size-4" />,
          onClick: () =>
            openComposer({ actionId: entry.action.id }, { origin: null }),
        })
      })}
    </>
  )
}

function UnpinAction({
  label,
  onClick,
}: {
  label: string
  onClick: () => void
}) {
  return (
    <SidebarMenuAction
      showOnHover
      title="Unpin"
      aria-label={`Unpin ${label}`}
      onClick={onClick}
    >
      <UiUnpinIcon />
    </SidebarMenuAction>
  )
}
