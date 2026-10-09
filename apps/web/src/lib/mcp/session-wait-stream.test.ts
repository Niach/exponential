import { afterEach, describe, expect, it, vi } from "vitest"
import { isSessionWait, withKeepalive } from "./session-wait-stream"

const call = (args: Record<string, unknown>, name = `exponential_sessions_get`) => ({
  jsonrpc: `2.0`,
  id: 1,
  method: `tools/call`,
  params: { name, arguments: args },
})

describe(`isSessionWait (FEED-83)`, () => {
  it(`matches only a waiting sessions_get call`, () => {
    expect(isSessionWait(call({ id: `x`, waitForIdle: true }))).toBe(true)
    expect(isSessionWait([call({ id: `x`, waitForIdle: true })])).toBe(true)
    expect(isSessionWait(call({ id: `x` }))).toBe(false)
    expect(isSessionWait(call({ waitForIdle: true }, `exponential_issues_get`))).toBe(false)
    expect(isSessionWait(undefined)).toBe(false)
    expect(isSessionWait({ method: `initialize` })).toBe(false)
  })
})

describe(`withKeepalive (FEED-83)`, () => {
  afterEach(() => vi.useRealTimers())

  it(`pings while the body is quiet and finishes once it ends`, async () => {
    vi.useFakeTimers()
    const encoder = new TextEncoder()
    let source!: ReadableStreamDefaultController<Uint8Array>
    const body = new ReadableStream<Uint8Array>({
      start: (controller) => {
        source = controller
      },
    })
    const onDone = vi.fn().mockResolvedValue(undefined)
    const text = new Response(withKeepalive(body, onDone, 1_000)).text()
    await vi.advanceTimersByTimeAsync(2_500)
    source.enqueue(encoder.encode(`event: message\ndata: {}\n\n`))
    source.close()
    await vi.advanceTimersByTimeAsync(0)
    expect(await text).toBe(
      `: keepalive\n\n: keepalive\n\nevent: message\ndata: {}\n\n`
    )
    expect(onDone).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(5_000)
    expect(onDone).toHaveBeenCalledTimes(1)
  })

  it(`finishes when the client leaves`, async () => {
    const body = new ReadableStream<Uint8Array>()
    const onDone = vi.fn().mockResolvedValue(undefined)
    await withKeepalive(body, onDone, 1_000).cancel()
    expect(onDone).toHaveBeenCalledTimes(1)
  })
})
