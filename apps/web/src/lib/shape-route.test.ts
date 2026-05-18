import { beforeEach, describe, expect, it, vi } from "vitest"
import { createShapeRouteHandler } from "@/lib/shape-route"

const { getSession, prepareElectricUrl, proxyElectricRequest } = vi.hoisted(
  () => ({
    getSession: vi.fn(),
    prepareElectricUrl: vi.fn(),
    proxyElectricRequest: vi.fn(),
  })
)

vi.mock(`@/lib/auth`, () => ({
  auth: {
    api: {
      getSession,
    },
  },
}))

vi.mock(`@/lib/electric-proxy`, () => ({
  prepareElectricUrl,
  proxyElectricRequest,
}))

describe(`shape route handler`, () => {
  beforeEach(() => {
    getSession.mockReset()
    prepareElectricUrl.mockReset()
    proxyElectricRequest.mockReset()
  })

  it(`returns 401 for unauthenticated requests when requireAuth is true`, async () => {
    getSession.mockResolvedValue(null)

    const handler = createShapeRouteHandler({
      table: `users`,
      requireAuth: true,
    })

    const response = await handler({
      request: new Request(`https://example.com/api/shapes/users`),
    })

    expect(response.status).toBe(401)
  })

  it(`forwards anonymous requests to getWhere with a null userId`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)
    getSession.mockResolvedValue(null)
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

  it(`applies the scoped where clause before proxying`, async () => {
    const originUrl = new URL(`https://electric.example/v1/shape`)

    getSession.mockResolvedValue({
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
