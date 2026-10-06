import { actionsFromBridge, createGate } from "./actions"

describe(`createGate`, () => {
  it(`never runs more than the limit at once`, async () => {
    const gate = createGate(2)
    let active = 0
    let peak = 0
    const task = () =>
      gate(async () => {
        active += 1
        peak = Math.max(peak, active)
        await new Promise((resolve) => setTimeout(resolve, 5))
        active -= 1
      })
    await Promise.all([task(), task(), task(), task(), task()])
    expect(peak).toBe(2)
  })
})

describe(`actionsFromBridge`, () => {
  it(`retries a host refusal for concurrency, then succeeds`, async () => {
    let calls = 0
    const actions = actionsFromBridge(
      async () => {
        calls += 1
        if (calls === 1) throw new Error(`MCP App request concurrency limit reached`)
        return { content: [{ type: `text`, text: `{"ok":true}` }] }
      },
      () => {},
      { retryDelayMs: 1 }
    )
    expect(await actions.call(`exponential_issues_get`, { id: `x` })).toEqual({
      kind: `ok`,
      data: { ok: true },
    })
    expect(calls).toBe(2)
  })

  it(`does not retry an ordinary error`, async () => {
    let calls = 0
    const actions = actionsFromBridge(
      async () => {
        calls += 1
        throw new Error(`Issue not found`)
      },
      () => {},
      { retryDelayMs: 1 }
    )
    expect((await actions.call(`x`, {})).kind).toBe(`error`)
    expect(calls).toBe(1)
  })
})
