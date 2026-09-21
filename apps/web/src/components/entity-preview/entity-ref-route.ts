import type { EntityRef } from "@/lib/mcp/preview"

// EXP-920: where a tool-row chip GOES — the pure half of
// `useEntityRefTarget`. A ref carries ids, never slugs (three of the four
// clients have no URLs at all), so the web resolves the route from what it
// has synced: the issue behind an issue/comment/attachment/notification ref
// (`IssueRefProvider`), a board's slug, another team's slug. Anything the
// client cannot place yields null, and the chip stays inert.
//
// A `list` chip never navigates: its card lists the members, and each of
// those is its own target.

export interface RouteTarget {
  to: string
  params: Record<string, string>
  search?: Record<string, string>
}

export interface IssueRouteRow {
  identifier: string
  boardSlug: string
}

/** The lookups the mapping needs, all synchronous over synced rows. */
export interface EntityRefRouteContext {
  teamSlug: string | undefined
  issueById: (id: string) => IssueRouteRow | null
  /** An issue ref may name the row by identifier alone (an input echo). */
  issueByIdentifier: (identifier: string) => IssueRouteRow | null
  boardSlugById: (id: string) => string | null
  /** Rows that point AT an issue: comments, attachments, notifications. */
  commentIssueId: (id: string) => string | null
  attachmentIssueId: (id: string) => string | null
  notificationIssueId: (id: string) => string | null
  teamSlugById: (id: string) => string | null
}

function issueRoute(
  teamSlug: string,
  row: IssueRouteRow | null
): RouteTarget | null {
  if (!row) return null
  return {
    to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
    params: {
      teamSlug,
      boardSlug: row.boardSlug,
      issueIdentifier: row.identifier,
    },
  }
}

function resolveIssue(
  ref: EntityRef,
  ctx: EntityRefRouteContext
): IssueRouteRow | null {
  return (
    ctx.issueById(ref.id) ??
    (ref.identifier ? ctx.issueByIdentifier(ref.identifier) : null) ??
    ctx.issueByIdentifier(ref.id)
  )
}

export function entityRefRoute(
  ref: EntityRef,
  ctx: EntityRefRouteContext
): RouteTarget | null {
  const teamSlug = ctx.teamSlug
  if (!teamSlug) return null
  switch (ref.kind) {
    case `issue`:
      return issueRoute(teamSlug, resolveIssue(ref, ctx))
    case `board`: {
      const boardSlug = ctx.boardSlugById(ref.id)
      return boardSlug
        ? { to: `/t/$teamSlug/boards/$boardSlug`, params: { teamSlug, boardSlug } }
        : null
    }
    case `action`:
      return {
        to: `/t/$teamSlug/actions`,
        params: { teamSlug },
        search: { editAction: ref.id },
      }
    case `automation`:
      return {
        to: `/t/$teamSlug/automations`,
        params: { teamSlug },
        search: { editAutomation: ref.id },
      }
    case `comment`: {
      const issueId = ctx.commentIssueId(ref.id)
      return issueId ? issueRoute(teamSlug, ctx.issueById(issueId)) : null
    }
    case `session`:
      return {
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: ref.id },
      }
    case `label`:
      return { to: `/t/$teamSlug/settings/labels`, params: { teamSlug } }
    case `status`:
      return { to: `/t/$teamSlug/settings/statuses`, params: { teamSlug } }
    case `workflow`:
      return {
        to: `/t/$teamSlug/workflows/$workflowId`,
        params: { teamSlug, workflowId: ref.id },
      }
    case `device`:
      return { to: `/t/$teamSlug/devices`, params: { teamSlug } }
    case `member`:
    case `invite`:
      return { to: `/t/$teamSlug/settings/members`, params: { teamSlug } }
    case `repository`:
      return { to: `/t/$teamSlug/settings/repositories`, params: { teamSlug } }
    case `team`: {
      const slug = ctx.teamSlugById(ref.id)
      return slug ? { to: `/t/$teamSlug`, params: { teamSlug: slug } } : null
    }
    case `notification`: {
      const issueId = ctx.notificationIssueId(ref.id)
      const viaIssue = issueId ? issueRoute(teamSlug, ctx.issueById(issueId)) : null
      return viaIssue ?? { to: `/t/$teamSlug/inbox`, params: { teamSlug } }
    }
    case `thread`:
      return {
        to: `/t/$teamSlug/support/$threadId`,
        params: { teamSlug, threadId: ref.id },
      }
    case `attachment`: {
      const issueId = ctx.attachmentIssueId(ref.id)
      return issueId ? issueRoute(teamSlug, ctx.issueById(issueId)) : null
    }
    case `list`:
      return null
  }
  return null
}
