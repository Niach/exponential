import { useCallback, useMemo, type ComponentProps, type ReactElement } from "react"
import { Link, useParams } from "@tanstack/react-router"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import type { EntityChipLinkProps } from "@exp/ui"
import type { Attachment, Board, Comment, Notification, Team } from "@/db/schema"
import {
  attachmentCollection,
  boardCollection,
  commentCollection,
  notificationCollection,
  teamCollection,
} from "@/lib/collections"
import { useIssueRefs } from "@/components/issue-ref-provider"
import type { EntityRef } from "@/lib/mcp/preview"
import {
  entityRefRoute,
  type EntityRefRouteContext,
  type RouteTarget,
} from "./entity-ref-route"

// EXP-920: the target behind a tool-row chip (and behind every row of a
// list chip's card). ONE hook resolves a whole ref set at once — the issues
// come from `IssueRefProvider` (already in context under the team layout),
// the boards/comments/attachments/notifications/teams from one `inArray`
// live query each over just the ids the refs name — so a card with eight
// member rows costs five queries, not forty. `entityRefRoute` is the pure
// mapping; this file only gathers its context.

export interface EntityRefTarget {
  /** The route, or null when this client cannot place the ref. */
  target: RouteTarget | null
  /** A real router `<Link>` around a chip or row body — ⌘-click, copy
   *  address, preloading. Undefined when there is no target. */
  link?: (props: EntityChipLinkProps | { className: string; children: React.ReactNode }) => ReactElement
}

function idsOf(refs: readonly EntityRef[], kind: EntityRef[`kind`]): string[] {
  // Sorted: the id list is a live-query dependency, and an unstable order
  // would re-run the query on every unrelated render.
  return refs
    .filter((ref) => ref.kind === kind)
    .map((ref) => ref.id)
    .sort()
}

function useRowsByIds<T extends { id: string }>(
  collection: typeof boardCollection | typeof commentCollection | typeof attachmentCollection | typeof notificationCollection | typeof teamCollection,
  ids: readonly string[]
): Map<string, T> {
  const key = ids.join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      // `undefined` (never `false`) is what skips a live query.
      ids.length > 0
        ? query
            .from({ rows: collection as typeof boardCollection })
            .where(({ rows }) => inArray(rows.id, ids as string[]))
        : undefined,
    [key]
  )
  return useMemo(
    () => new Map(((data ?? []) as unknown as T[]).map((row) => [row.id, row])),
    [data]
  )
}

/** The route resolver for a set of refs — `resolve(ref)` is synchronous
 *  once the rows are in, so a card can map its member rows in a plain loop. */
export function useEntityRefTargets(
  refs: readonly EntityRef[]
): (ref: EntityRef) => EntityRefTarget {
  const { teamSlug } = useParams({ strict: false })
  const issueRefs = useIssueRefs()

  const boards = useRowsByIds<Board>(boardCollection, idsOf(refs, `board`))
  const comments = useRowsByIds<Comment>(commentCollection, idsOf(refs, `comment`))
  const attachments = useRowsByIds<Attachment>(
    attachmentCollection,
    idsOf(refs, `attachment`)
  )
  const notifications = useRowsByIds<Notification>(
    notificationCollection,
    idsOf(refs, `notification`)
  )
  const teams = useRowsByIds<Team>(teamCollection, idsOf(refs, `team`))

  const ctx = useMemo<EntityRefRouteContext>(
    () => ({
      teamSlug,
      issueById: (id) => issueRefs?.resolveById(id) ?? null,
      issueByIdentifier: (identifier) => issueRefs?.resolve(identifier) ?? null,
      boardSlugById: (id) => boards.get(id)?.slug ?? null,
      commentIssueId: (id) => comments.get(id)?.issueId ?? null,
      attachmentIssueId: (id) => attachments.get(id)?.issueId ?? null,
      notificationIssueId: (id) => notifications.get(id)?.issueId ?? null,
      teamSlugById: (id) => teams.get(id)?.slug ?? null,
    }),
    [teamSlug, issueRefs, boards, comments, attachments, notifications, teams]
  )

  return useCallback(
    (ref: EntityRef): EntityRefTarget => {
      const target = entityRefRoute(ref, ctx)
      if (!target) return { target: null }
      return { target, link: (props) => <EntityRefLink route={target} {...props} /> }
    },
    [ctx]
  )
}

/** One ref's target. */
export function useEntityRefTarget(ref: EntityRef): EntityRefTarget {
  const refs = useMemo(() => [ref], [ref])
  return useEntityRefTargets(refs)(ref)
}

/** The router link a chip, a row or the phone sheet's "Open" renders around
 *  its body. The target is a computed route, so the props are cast once here
 *  rather than at every call site (the router's `to` is a literal union);
 *  everything else (`className`, `onClick`, aria) passes through, so a
 *  `Button asChild` can wear it. */
export function EntityRefLink({
  route,
  children,
  ...rest
}: {
  route: RouteTarget
  children: React.ReactNode
} & Omit<ComponentProps<`a`>, `href` | `children` | `target`>) {
  const props = {
    to: route.to,
    params: route.params,
    search: route.search,
  } as unknown as ComponentProps<typeof Link>
  return (
    <Link {...props} {...(rest as object)}>
      {children}
    </Link>
  )
}
