import { useMemo, type ReactNode, type RefObject } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Issue, IssueLabel, Label, User } from "@/db/schema"
import {
  issueCollection,
  issueLabelCollection,
  labelCollection,
  userCollection,
} from "@/lib/collections"
import { ISSUE_PRIORITY_FALLBACK } from "@/lib/domain"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { useIsMobile } from "@/hooks/use-mobile"
import { cn, getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
import { getPriorityConfig } from "@/components/issue-properties/priority-dropdown"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card"
import { Pill } from "@/components/ui/pill"
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
} from "@/components/ui/popover"

// EXP-760 — the Linear-style issue PREVIEW: identifier + assignee, the title,
// then status · priority · labels. One card body, three hosts:
//
//   * `IssuePreviewHoverCard` wraps an ELEMENT trigger (a relation row, a
//     timeline chip, an `IssueRefPill`);
//   * `IssuePreviewAnchoredPopover` is anchored to a DOM rect instead, for the
//     `#IDENT` pills that are ProseMirror DECORATIONS and therefore have no
//     React element to wrap (see issue-editor/issue-ref-hover-layer.tsx);
//   * the desktop IDE draws the same fields in `crates/ui/src/issue_preview.rs`.
//
// Everything is read from the already-synced shapes — a preview never hits the
// server. The card renders nothing at all when the issue is not visible to
// this viewer (another team, a trashed board), which is also what makes the
// hover hosts safe to point at any identifier an agent happened to write.
//
// Hover is desktop-only: on phones a chip is a LINK, and a preview that opens
// under the finger would just swallow the tap. Both hosts return their
// children untouched when `useIsMobile()`.

/** The preview body. Null while the issue is unknown to this client. */
export function IssuePreviewCard({ issueId }: { issueId: string }) {
  const { resolve: resolveStatus } = useTeamStatusesContext()

  const { data: issueRows } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.id, issueId)),
    [issueId]
  )
  const issue = (issueRows?.[0] ?? null) as Issue | null

  const { data: issueLabelRows } = useLiveQuery(
    (query) =>
      query
        .from({ issueLabels: issueLabelCollection })
        .where(({ issueLabels }) => eq(issueLabels.issueId, issueId)),
    [issueId]
  )
  // Sorted: the id list is a live-query dependency, and an unstable order
  // would re-run the labels query on every unrelated row change.
  const labelIds = useMemo(
    () =>
      ((issueLabelRows ?? []) as IssueLabel[])
        .map((row) => row.labelId)
        .sort(),
    [issueLabelRows]
  )

  const { data: labelRows } = useLiveQuery(
    (query) =>
      // `undefined` (never `false`) is what skips a live query.
      labelIds.length > 0
        ? query
            .from({ labels: labelCollection })
            .where(({ labels }) => inArray(labels.id, labelIds))
        : undefined,
    [labelIds.join(`,`)]
  )

  const assigneeId = issue?.assigneeId ?? null
  const { data: userRows } = useLiveQuery(
    (query) =>
      assigneeId
        ? query
            .from({ users: userCollection })
            .where(({ users }) => eq(users.id, assigneeId))
        : undefined,
    [assigneeId]
  )
  const assignee = (userRows?.[0] ?? null) as User | null

  if (!issue) return null

  const status = resolveStatus(issue)
  const priority = getPriorityConfig(issue.priority)
  const PriorityGlyph = priority.icon
  const labels = (labelRows ?? []) as Label[]

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex min-w-0 items-center gap-2">
        <span className="shrink-0 font-mono text-xs text-muted-foreground">
          {issue.identifier}
        </span>
        {assigneeId && (
          <span className="ml-auto flex min-w-0 items-center gap-1.5">
            <Avatar className="size-4 shrink-0">
              {assignee?.image && (
                <AvatarImage
                  src={assignee.image}
                  alt={displayUserName(assignee ?? undefined, assigneeId)}
                />
              )}
              <AvatarFallback className="text-[0.5rem]" userId={assigneeId}>
                {getInitials(displayUserName(assignee ?? undefined, assigneeId))}
              </AvatarFallback>
            </Avatar>
            <span className="min-w-0 truncate text-xs text-muted-foreground">
              {displayUserName(assignee ?? undefined, assigneeId)}
            </span>
          </span>
        )}
      </div>

      <p className="line-clamp-2 text-sm font-medium text-foreground">
        {issue.title}
      </p>

      <div className="flex flex-wrap items-center gap-1.5">
        <Pill
          size="sm"
          leading={<StatusIcon option={status} className="!h-3 !w-3" />}
        >
          {status.name}
        </Pill>
        {/* "No priority" is the absence of a decision, not a property worth a
            chip — the natives leave it out of their previews too. */}
        {issue.priority !== ISSUE_PRIORITY_FALLBACK && (
          <Pill
            size="sm"
            leading={
              <PriorityGlyph className={cn(`!h-3 !w-3`, priority.color)} />
            }
          >
            {priority.label}
          </Pill>
        )}
        {labels.map((label) => (
          <Pill key={label.id} size="sm" dot={label.color}>
            {label.name}
          </Pill>
        ))}
      </div>
    </div>
  )
}

/**
 * Wrap an element trigger in the preview. `children` must accept a ref (the
 * trigger is `asChild`), so pass a DOM element or a `forwardRef` component.
 */
export function IssuePreviewHoverCard({
  issueId,
  children,
}: {
  issueId: string | null | undefined
  children: ReactNode
}) {
  const isMobile = useIsMobile()
  if (isMobile || !issueId) return <>{children}</>

  return (
    <HoverCard>
      <HoverCardTrigger asChild>{children}</HoverCardTrigger>
      {/* Radix's hover card never takes focus, so there is no autofocus to
          suppress here (unlike the anchored popover below). */}
      <HoverCardContent>
        <IssuePreviewCard issueId={issueId} />
      </HoverCardContent>
    </HoverCard>
  )
}

/** What Radix's popper needs of a virtual anchor: a live rect, nothing else. */
export interface PreviewAnchor {
  getBoundingClientRect: () => DOMRect
}

/**
 * The same card, anchored to a measured rect rather than an element — the
 * host owns the open/close timing (a ProseMirror decoration is not a React
 * element, so there is nothing to wrap). `modal={false}` keeps the page
 * scrollable and pointer-interactive underneath.
 */
export function IssuePreviewAnchoredPopover({
  issueId,
  anchorRef,
  onClose,
}: {
  issueId: string | null
  anchorRef: RefObject<PreviewAnchor>
  onClose: () => void
}) {
  if (!issueId) return null

  return (
    <Popover open modal={false} onOpenChange={(next) => !next && onClose()}>
      <PopoverAnchor virtualRef={anchorRef} />
      <PopoverContent
        align="start"
        side="bottom"
        sideOffset={6}
        className="w-80 p-3"
        onOpenAutoFocus={(event) => event.preventDefault()}
        // Purely a hover affordance — the layer above closes it on pointer
        // leave, scroll and Escape, and a click anywhere should reach the
        // document rather than being eaten by a dismiss.
        onPointerDownOutside={(event) => event.preventDefault()}
      >
        <IssuePreviewCard issueId={issueId} />
      </PopoverContent>
    </Popover>
  )
}
