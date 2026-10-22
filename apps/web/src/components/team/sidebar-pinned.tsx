import { useMemo } from "react"
import type * as React from "react"
import { Link, useParams } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import {
  conceptIcon,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@exp/ui"
import type { Board, Issue, Pin } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { useTeamPins } from "@/hooks/use-pins"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"

// EXP-778: the sidebar's Pinned group — the caller's favourites in this team
// (issues; NEW-2: a pin of kind `action` never renders), in pin order. A row renders only when its target is
// synced: the pins shape is per user and never trash-scoped, so an issue on a
// trashed board simply has no row until it comes back. Sits between the nav
// entries and Boards, hidden when empty; hovering a row offers Unpin.
//
// EXP-877: a pinned SESSION draws nothing any more. A live run of mine is
// already listed (EXP-923: the Running section right below this one) and an
// ended one belongs to the Agent page's Recent list, so a second copy in the
// sidebar said the same thing twice. `pinKind.session` stays in the contract
// and old rows stay in the database — they simply resolve to no row.

const UiUnpinIcon = conceptIcon(`ui-unpin`)
const NavIssuesIcon = conceptIcon(`nav-issues`)

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
type ResolvedPin = {
  kind: `issue`
  pin: Pin
  issue: Issue
  board: Board
  active: boolean
}

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
  const { issueIdentifier: routeIssueIdentifier } = useParams({ strict: false })

  return useMemo(() => {
    const boardsById = new Map((boards ?? []).map((board) => [board.id, board]))
    const issuesById = new Map(
      ((issues ?? []) as Issue[]).map((issue) => [issue.id, issue])
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
      return []
    })
  }, [pins, boards, issues, routeIssueIdentifier])
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

  const unpin = (pin: Pin) => {
    const targetId = pin.issueId
    if (!targetId) return
    void trpc.pins.toggle.mutate({ teamId, kind: pin.kind, targetId })
  }

  const items = resolved.map((entry) => {
    const { pin, issue, board } = entry
    return (
      <SidebarMenuItem key={pin.id}>
        <SidebarMenuButton
          asChild
          density="compact"
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
  if (resolved.length === 0) return null
  return (
    <>
      {separator}
      {resolved.map((entry) =>
        renderItem({
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
      )}
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
