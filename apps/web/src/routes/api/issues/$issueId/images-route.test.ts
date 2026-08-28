import { describe, expect, it } from "vitest"
import { Route } from "@/routes/api/issues/$issueId/images"

type Handler = () => Response
const handler = (
  Route as unknown as { options: { server: { handlers: { POST: Handler } } } }
).options.server.handlers.POST

// EXP-639: pre-/files clients must get a readable 410, never a TanStack
// HTML 404 (self-hosted instances often run with no version floor at all).
describe(`POST /api/issues/$issueId/images`, () => {
  it(`answers 410 with a reason and no upload work`, async () => {
    const response = handler()
    expect(response.status).toBe(410)
    expect(await response.json()).toEqual({
      error: `Inline image upload moved. Update the Exponential app to attach images.`,
    })
  })
})
