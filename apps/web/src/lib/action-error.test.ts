import { describe, expect, it, vi } from "vitest"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"

const toastError = vi.hoisted(() => vi.fn())
vi.mock(`sonner`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("sonner")>()
  return { ...actual, toast: Object.assign(() => {}, actual.toast, { error: toastError }) }
})

import { actionErrorTitle, errorSurface } from "@exp/ui"
import { actionErrorDescription, failedAction } from "./action-error"

// FEED-79: the fixture's action cases are toasts titled per `titlePattern`.
describe(`feedback-states errors`, () => {
  const actionCases = FEEDBACK_STATES.errors.cases.filter((c) => c.kind === `action`)

  it.each(actionCases)(`$name`, (c) => {
    expect(actionErrorTitle(c.verb!, c.thing!)).toBe(c.title)
    toastError.mockClear()
    const retry = c.retry ? () => {} : undefined
    failedAction(c.verb!, c.thing!, new Error(`Server said no`), { retry })
    expect(toastError).toHaveBeenCalledTimes(1)
    const [title, options] = toastError.mock.calls[0]
    expect(title).toBe(c.title)
    expect(options.description).toBe(`Server said no`)
    expect(options.action?.label).toBe(c.retry ? FEEDBACK_STATES.errors.retry : undefined)
  })

  // FEED-79: automatic or background work (sync, polling, a load on open or
  // focus) never reaches `failedAction`; it keeps its quiet surface.
  it.each(FEEDBACK_STATES.errors.cases.filter((c) => c.kind === `background`))(`$name`, (c) => {
    expect(c.render).toBe(`quiet`)
    expect(errorSurface(`background`)).toBe(`quiet`)
  })

  it(`drops JSON-shaped messages and keeps plain ones`, () => {
    expect(actionErrorDescription(new Error(`[{"code":"too_small"}]`))).toBeUndefined()
    expect(actionErrorDescription({ message: `Invalid code` })).toBe(`Invalid code`)
    expect(actionErrorDescription(undefined)).toBeUndefined()
  })
})
