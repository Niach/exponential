import { beforeEach, describe, expect, it, vi } from "vitest"
import { createShapeRouteHandler } from "@/lib/shape-route"
import { Route as boardsRoute } from "@/routes/api/shapes/boards"
import { Route as usersRoute } from "@/routes/api/shapes/users"
import { Route as teamInvitesRoute } from "@/routes/api/shapes/team-invites"
import { Route as issuesRoute } from "@/routes/api/shapes/issues"
import { Route as commentsRoute } from "@/routes/api/shapes/comments"
import { Route as issueEventsRoute } from "@/routes/api/shapes/issue-events"
import { Route as issueLabelsRoute } from "@/routes/api/shapes/issue-labels"
import { Route as issueSubscribersRoute } from "@/routes/api/shapes/issue-subscribers"
import { Route as attachmentsRoute } from "@/routes/api/shapes/attachments"
import { Route as codingSessionsRoute } from "@/routes/api/shapes/coding-sessions"
import { Route as notificationsRoute } from "@/routes/api/shapes/notifications"

const { resolveSession, prepareElectricUrl, proxyElectricRequest } = vi.hoisted(
  () => ({
    resolveSession: vi.fn(),
    prepareElectricUrl: vi.fn(),
    proxyElectricRequest: vi.fn(),
  })
)

// The real shape proxies resolve their scope through team-membership; keep
// the pure clause builders (andClauses/buildWhereClause) real and only stub the
// DB-touching scope resolvers.
const membership = vi.hoisted(() => ({
  getUserTeamIds: vi.fn(),
  getReadableUserIdsInTeams: vi.fn(),
}))

vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSession,
}))

vi.mock(`@/lib/electric-proxy`, () => ({
  prepareElectricUrl,
  proxyElectricRequest,
}))

vi.mock(`@/lib/team-membership`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/team-membership")>()
  return {
    ...actual,
    getUserTeamIds: membership.getUserTeamIds,
    getReadableUserIdsInTeams: membership.getReadableUserIdsInTeams,
  }
})

type ShapeHandler = (args: { request: Request }) => Promise<Response>
function shapeHandler(route: unknown): ShapeHandler {
  return (
    route as { options: { server: { handlers: { GET: ShapeHandler } } } }
  ).options.server.handlers.GET
}

describe(`shape route handler`, () => {
  beforeEach(() => {
    resolveSession.mockReset()
    prepareElectricUrl.mockReset()
    proxyElectricRequest.mockReset()
  })

  it(`returns 401 for unauthenticated requests when requireAuth is true`, async () => {
    resolveSession.mockResolvedValue(null)

    const handler = createShapeRouteHandler({
      table: `users`,
      requireAuth: true,
    })

    const response = await handler({
      request: new Request(`https://example.com/api/shapes/users`),
    })

    expect(response.status).toBe(401)
  })

  it(`returns 401 when a bearer token is presented but resolves to no session`, async () => {
    resolveSession.mockResolvedValue(null)

    const handler = createShapeRouteHandler({
      table: `issues`,
      getWhere: async () => `"board_id" = 'p-1'`,
    })

    // A dead token must NOT degrade to the anonymous where clause (that
    // rotates the shape identity with HTTP 200) — it must 401.
    const response = await handler({
      request: new Request(`https://example.com/api/shapes/issues`, {
        headers: { authorization: `Bearer dead-token` },
      }),
    })

    expect(response.status).toBe(401)
    expect(proxyElectricRequest).not.toHaveBeenCalled()
  })

  it(`returns 401 when an x-api-key is presented but resolves to no session`, async () => {
    resolveSession.mockResolvedValue(null)

    const handler = createShapeRouteHandler({
      table: `issues`,
      getWhere: async () => `"board_id" = 'p-1'`,
    })

    const response = await handler({
      request: new Request(`https://example.com/api/shapes/issues`, {
        headers: { "x-api-key": `expu_revoked` },
      }),
    })

    expect(response.status).toBe(401)
    expect(proxyElectricRequest).not.toHaveBeenCalled()
  })

  it(`keeps the anonymous fallback for cookie-only requests with a dead session`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue(null)
    prepareElectricUrl.mockReturnValue(originUrl)
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))

    const getWhere = vi.fn().mockResolvedValue(`"is_public" = true`)
    const handler = createShapeRouteHandler({
      table: `teams`,
      getWhere,
    })

    // The web collection layer has no 401 recovery, so an expired session
    // cookie falls back to the anonymous where clause instead of erroring;
    // the router auth guard re-authenticates on next navigation.
    const response = await handler({
      request: new Request(`https://example.com/api/shapes/teams`, {
        headers: { cookie: `better-auth.session_token=expired` },
      }),
    })

    expect(response.status).toBe(200)
    expect(getWhere).toHaveBeenCalledWith(null)
  })

  it(`forwards anonymous requests to getWhere with a null userId`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue(null)
    prepareElectricUrl.mockReturnValue(originUrl)
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))

    const getWhere = vi.fn().mockResolvedValue(`"is_public" = true`)
    const handler = createShapeRouteHandler({
      table: `teams`,
      getWhere,
    })

    await handler({
      request: new Request(`https://example.com/api/shapes/teams`),
    })

    expect(getWhere).toHaveBeenCalledWith(null)
    expect(originUrl.searchParams.get(`where`)).toBe(`"is_public" = true`)
  })

  it(`forwards a server-side columns allowlist to Electric`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue(null)
    prepareElectricUrl.mockReturnValue(originUrl)
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))

    const handler = createShapeRouteHandler({
      table: `issue_subscribers`,
      columns: [`id`, `issue_id`, `team_id`],
    })

    // A client-supplied columns param must not widen the allowlist.
    await handler({
      request: new Request(
        `https://example.com/api/shapes/issue-subscribers?columns=id,email`
      ),
    })

    expect(originUrl.searchParams.get(`columns`)).toBe(
      `id,issue_id,team_id`
    )
  })

  it(`applies the scoped where clause before proxying`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)

    resolveSession.mockResolvedValue({
      user: {
        id: `user-1`,
      },
    })
    prepareElectricUrl.mockReturnValue(originUrl)
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))

    const handler = createShapeRouteHandler({
      table: `users`,
      getWhere: async () => `"id" IN ('user-1','user-2')`,
    })

    const request = new Request(`https://example.com/api/shapes/users`)
    await handler({ request })

    expect(originUrl.searchParams.get(`table`)).toBe(`users`)
    expect(originUrl.searchParams.get(`where`)).toBe(
      `"id" IN ('user-1','user-2')`
    )
    expect(proxyElectricRequest).toHaveBeenCalledWith(originUrl, request.signal)
  })
})

describe(`shape column + trash contracts`, () => {
  beforeEach(() => {
    resolveSession.mockReset()
    prepareElectricUrl.mockReset()
    proxyElectricRequest.mockReset()
    membership.getUserTeamIds.mockReset()
    membership.getReadableUserIdsInTeams.mockReset()
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))
  })

  it(`pins the boards columns and appends the deleted_at filter for members`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getUserTeamIds.mockResolvedValue([`w-2`, `w-1`])

    await shapeHandler(boardsRoute)({
      request: new Request(`https://example.com/api/shapes/boards`, {
        headers: { authorization: `Bearer t` },
      }),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).toContain(`is_protected`)
    expect(columns).toContain(`deleted_at`)
    // The public-board columns are gone (EXP-180) and must never resync.
    expect(columns).not.toContain(`is_public`)
    expect(columns).not.toContain(`public_show_comments`)
    expect(columns).not.toContain(`public_show_activity`)
    expect(columns).not.toContain(`helpdesk_enabled`)
    const where = originUrl.searchParams.get(`where`) ?? ``
    expect(where).toContain(`"deleted_at" IS NULL`)
    // Byte-stable: team ids are sorted regardless of query heap order.
    expect(where).toContain(`"team_id" IN ('w-1','w-2')`)
  })

  it(`anonymous boards requests get the impossible-match sentinel`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue(null)
    prepareElectricUrl.mockReturnValue(originUrl)

    await shapeHandler(boardsRoute)({
      request: new Request(`https://example.com/api/shapes/boards`),
    })

    expect(originUrl.searchParams.get(`where`)).toBe(
      `"id" = '00000000-0000-0000-0000-000000000000'`
    )
  })

  it(`pins the users shape to exactly the 7 client columns`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getReadableUserIdsInTeams.mockResolvedValue([`user-1`])

    await shapeHandler(usersRoute)({
      request: new Request(`https://example.com/api/shapes/users`, {
        headers: { authorization: `Bearer t` },
      }),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).toEqual([
      `id`,
      `name`,
      `email`,
      `image`,
      `created_at`,
      `updated_at`,
    ])
    // is_agent was removed from the users schema + shape entirely.
    expect(columns).not.toContain(`is_agent`)
    // The columns that used to crash native partial-update loops must be gone.
    expect(columns).not.toContain(`onboarding_completed_at`)
    expect(columns).not.toContain(`is_admin`)
    expect(columns).not.toContain(`email_verified`)
  })

  it(`pins the team-invites columns and excludes the invite bearer token`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getUserTeamIds.mockResolvedValue([`w-1`])

    // A client attempting to widen the allowlist back to `token` must be
    // overridden by the server pin — the token is a bearer secret (accept is
    // not recipient-bound; a synced owner-role token would let any member
    // escalate to owner).
    await shapeHandler(teamInvitesRoute)({
      request: new Request(
        `https://example.com/api/shapes/team-invites?columns=token`,
        { headers: { authorization: `Bearer t` } }
      ),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).toEqual([
      `id`,
      `team_id`,
      `invited_by_id`,
      `role`,
      `email`,
      `accepted_at`,
      `expires_at`,
      `created_at`,
      `updated_at`,
    ])
    expect(columns).not.toContain(`token`)
  })
})

describe(`team-stable trash-aware child shapes (REV2-5)`, () => {
  beforeEach(() => {
    resolveSession.mockReset()
    prepareElectricUrl.mockReset()
    proxyElectricRequest.mockReset()
    membership.getUserTeamIds.mockReset()
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))
  })

  // All board-scoped shapes scope members by TEAM (stable across board
  // create/trash/restore) with the static board_deleted_at trash predicate —
  // a board-id list here would rotate the shape identity on every board
  // create/trash in ANY of the user's teams and force full cross-team
  // resyncs. buildWhereClause sorts the id list, so the SQL is byte-stable.
  const childRoutes = [
    [`issues`, issuesRoute],
    [`comments`, commentsRoute],
    [`issue-events`, issueEventsRoute],
    [`issue-labels`, issueLabelsRoute],
    [`issue-subscribers`, issueSubscribersRoute],
    [`attachments`, attachmentsRoute],
    [`coding-sessions`, codingSessionsRoute],
  ] as const

  it.each(childRoutes)(
    `%s member branch is team-scoped, trash-aware, and byte-stable`,
    async (_name, route) => {
      const originUrl = new URL(`https://electric.example/v1/shape`)
      resolveSession.mockResolvedValue({ user: { id: `user-1` } })
      prepareElectricUrl.mockReturnValue(originUrl)
      // Unsorted on purpose — the emitted clause must come out sorted.
      membership.getUserTeamIds.mockResolvedValue([`w-2`, `w-1`])

      await shapeHandler(route)({
        request: new Request(`https://example.com/api/shapes/x`, {
          headers: { authorization: `Bearer t` },
        }),
      })

      expect(originUrl.searchParams.get(`where`)).toBe(
        `("team_id" IN ('w-1','w-2')) AND ("board_deleted_at" IS NULL)`
      )
    }
  )

  it.each(childRoutes)(
    `%s anonymous clause stays byte-identical to the sentinel composite`,
    async (_name, route) => {
      const originUrl = new URL(`https://electric.example/v1/shape`)
      resolveSession.mockResolvedValue(null)
      prepareElectricUrl.mockReturnValue(originUrl)

      await shapeHandler(route)({
        request: new Request(`https://example.com/api/shapes/x`),
      })

      expect(originUrl.searchParams.get(`where`)).toBe(
        `("team_id" = '00000000-0000-0000-0000-000000000000') AND ("board_deleted_at" IS NULL)`
      )
      expect(membership.getUserTeamIds).not.toHaveBeenCalled()
    }
  )

  it.each(childRoutes)(
    `%s pins a columns allowlist that excludes board_deleted_at`,
    async (_name, route) => {
      const originUrl = new URL(`https://electric.example/v1/shape`)
      resolveSession.mockResolvedValue({ user: { id: `user-1` } })
      prepareElectricUrl.mockReturnValue(originUrl)
      membership.getUserTeamIds.mockResolvedValue([`w-1`])

      // A client attempting to widen the allowlist to the server-only trash
      // mirror must be overridden — native schemas don't carry the column.
      await shapeHandler(route)({
        request: new Request(
          `https://example.com/api/shapes/x?columns=board_deleted_at`,
          { headers: { authorization: `Bearer t` } }
        ),
      })

      const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
      expect(columns.length).toBeGreaterThan(0)
      expect(columns).not.toContain(`board_deleted_at`)
    }
  )

  it(`issues excludes the server-only team_id scoping column`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getUserTeamIds.mockResolvedValue([`w-1`])

    await shapeHandler(issuesRoute)({
      request: new Request(`https://example.com/api/shapes/issues`, {
        headers: { authorization: `Bearer t` },
      }),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    // Native issue schemas have no team_id — it must never reach the wire.
    expect(columns).not.toContain(`team_id`)
    expect(columns).toContain(`id`)
    expect(columns).toContain(`board_id`)
    expect(columns).toContain(`identifier`)
  })

  it(`issue-subscribers keeps the email-excluding columns pin on the member path`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getUserTeamIds.mockResolvedValue([`w-1`])

    // A client attempting to widen the allowlist to `email` must be overridden.
    await shapeHandler(issueSubscribersRoute)({
      request: new Request(
        `https://example.com/api/shapes/issue-subscribers?columns=email`,
        { headers: { authorization: `Bearer t` } }
      ),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).not.toContain(`email`)
    expect(columns).toContain(`team_id`)
    expect(originUrl.searchParams.get(`where`)).toBe(
      `("team_id" IN ('w-1')) AND ("board_deleted_at" IS NULL)`
    )
  })

  it(`notifications member clause is fully static per user`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)

    await shapeHandler(notificationsRoute)({
      request: new Request(`https://example.com/api/shapes/notifications`, {
        headers: { authorization: `Bearer t` },
      }),
    })

    // No membership id lists at all — this shape's identity never rotates.
    // Trash-awareness rides the static board_deleted_at predicate; issue-less
    // rows (NULL board_deleted_at) always match.
    expect(originUrl.searchParams.get(`where`)).toBe(
      `("user_id" = 'user-1') AND ("board_deleted_at" IS NULL)`
    )
    expect(membership.getUserTeamIds).not.toHaveBeenCalled()
    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).not.toContain(`board_id`)
    expect(columns).not.toContain(`board_deleted_at`)
    expect(columns).not.toContain(`emailed_at`)
  })
})
