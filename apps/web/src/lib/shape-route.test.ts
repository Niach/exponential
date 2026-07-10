import { beforeEach, describe, expect, it, vi } from "vitest"
import { createShapeRouteHandler } from "@/lib/shape-route"
import { Route as projectsRoute } from "@/routes/api/shapes/projects"
import { Route as usersRoute } from "@/routes/api/shapes/users"
import { Route as workspaceInvitesRoute } from "@/routes/api/shapes/workspace-invites"

const { resolveSession, prepareElectricUrl, proxyElectricRequest } = vi.hoisted(
  () => ({
    resolveSession: vi.fn(),
    prepareElectricUrl: vi.fn(),
    proxyElectricRequest: vi.fn(),
  })
)

// The real shape proxies resolve their scope through workspace-membership; keep
// the pure clause builders (andClauses/buildWhereClause) real and only stub the
// DB-touching scope resolvers.
const membership = vi.hoisted(() => ({
  getUserWorkspaceIds: vi.fn(),
  getPublicProjectScope: vi.fn(),
  getReadableUserIdsInWorkspaces: vi.fn(),
}))

vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSession,
}))

vi.mock(`@/lib/electric-proxy`, () => ({
  prepareElectricUrl,
  proxyElectricRequest,
}))

vi.mock(`@/lib/workspace-membership`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/workspace-membership")>()
  return {
    ...actual,
    getUserWorkspaceIds: membership.getUserWorkspaceIds,
    getPublicProjectScope: membership.getPublicProjectScope,
    getReadableUserIdsInWorkspaces: membership.getReadableUserIdsInWorkspaces,
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
      getWhere: async () => `"project_id" = 'p-1'`,
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
      getWhere: async () => `"project_id" = 'p-1'`,
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
      table: `workspaces`,
      getWhere,
    })

    // The web collection layer has no 401 recovery, so an expired session
    // cookie falls back to the anonymous where clause instead of erroring;
    // the router auth guard re-authenticates on next navigation.
    const response = await handler({
      request: new Request(`https://example.com/api/shapes/workspaces`, {
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
      table: `workspaces`,
      getWhere,
    })

    await handler({
      request: new Request(`https://example.com/api/shapes/workspaces`),
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
      columns: [`id`, `issue_id`, `workspace_id`],
    })

    // A client-supplied columns param must not widen the allowlist.
    await handler({
      request: new Request(
        `https://example.com/api/shapes/issue-subscribers?columns=id,email`
      ),
    })

    expect(originUrl.searchParams.get(`columns`)).toBe(
      `id,issue_id,workspace_id`
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
    membership.getUserWorkspaceIds.mockReset()
    membership.getPublicProjectScope.mockReset()
    membership.getReadableUserIdsInWorkspaces.mockReset()
    proxyElectricRequest.mockResolvedValue(new Response(`ok`))
  })

  it(`pins the projects columns and appends the deleted_at filter for members`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getUserWorkspaceIds.mockResolvedValue([`w-2`, `w-1`])

    await shapeHandler(projectsRoute)({
      request: new Request(`https://example.com/api/shapes/projects`, {
        headers: { authorization: `Bearer t` },
      }),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).toContain(`is_protected`)
    expect(columns).toContain(`deleted_at`)
    const where = originUrl.searchParams.get(`where`) ?? ``
    expect(where).toContain(`"deleted_at" IS NULL`)
    // Byte-stable: workspace ids are sorted regardless of query heap order.
    expect(where).toContain(`"workspace_id" IN ('w-1','w-2')`)
  })

  it(`scopes anonymous projects to the public ids with no deleted_at suffix`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue(null)
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getPublicProjectScope.mockResolvedValue({ projectIds: [`p-1`] })

    await shapeHandler(projectsRoute)({
      request: new Request(`https://example.com/api/shapes/projects`),
    })

    expect(originUrl.searchParams.get(`where`)).toBe(`"id" IN ('p-1')`)
    expect(originUrl.searchParams.get(`columns`)?.split(`,`)).toContain(
      `is_protected`
    )
  })

  it(`pins the users shape to exactly the 7 client columns`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getReadableUserIdsInWorkspaces.mockResolvedValue([`user-1`])

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
      `is_agent`,
      `created_at`,
      `updated_at`,
    ])
    // The columns that used to crash native partial-update loops must be gone.
    expect(columns).not.toContain(`onboarding_completed_at`)
    expect(columns).not.toContain(`is_admin`)
    expect(columns).not.toContain(`email_verified`)
  })

  it(`pins the workspace-invites columns and excludes the invite bearer token`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    resolveSession.mockResolvedValue({ user: { id: `user-1` } })
    prepareElectricUrl.mockReturnValue(originUrl)
    membership.getUserWorkspaceIds.mockResolvedValue([`w-1`])

    // A client attempting to widen the allowlist back to `token` must be
    // overridden by the server pin — the token is a bearer secret (accept is
    // not recipient-bound; a synced owner-role token would let any member
    // escalate to owner).
    await shapeHandler(workspaceInvitesRoute)({
      request: new Request(
        `https://example.com/api/shapes/workspace-invites?columns=token`,
        { headers: { authorization: `Bearer t` } }
      ),
    })

    const columns = originUrl.searchParams.get(`columns`)?.split(`,`) ?? []
    expect(columns).toEqual([
      `id`,
      `workspace_id`,
      `invited_by_id`,
      `role`,
      `accepted_at`,
      `expires_at`,
      `created_at`,
      `updated_at`,
    ])
    expect(columns).not.toContain(`token`)
  })
})
