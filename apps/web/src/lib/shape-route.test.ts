import { beforeEach, describe, expect, it, vi } from "vitest"
import { createShapeRouteHandler } from "@/lib/shape-route"

const { resolveSession, prepareElectricUrl, proxyElectricRequest } = vi.hoisted(
  () => ({
    resolveSession: vi.fn(),
    prepareElectricUrl: vi.fn(),
    proxyElectricRequest: vi.fn(),
  })
)

vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSession,
}))

vi.mock(`@/lib/electric-proxy`, () => ({
  prepareElectricUrl,
  proxyElectricRequest,
}))

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
