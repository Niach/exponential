import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { AppBridge, HostRefusal } from "./bridge"
import { actionsFromBridge } from "./actions"
import { EXPIRED_LINE, isLeaseRefusal, loadScreen, saveScreen, screenKey, useViewLease } from "./lease"

describe(`view lease (#NEW-24)`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
    window.sessionStorage.clear()
  })
  afterEach(() => vi.useRealTimers())

  it(`keeps a screen under the host's tool-call id`, () => {
    expect(screenKey(`issues`, `call-7`)).toBe(`exp-view:issues:call-7`)
    expect(screenKey(`issues`, 7)).toBe(`exp-view:issues:7`)
    expect(screenKey(`issues`, undefined)).toBe(`exp-view:issues:`)
    saveScreen(screenKey(`issues`, `a`), { kind: `issues`, issues: [1] })
    expect(loadScreen(screenKey(`issues`, `a`))).toEqual({ kind: `issues`, issues: [1] })
    expect(loadScreen(screenKey(`issues`, `b`))).toBeNull()
    // A sandbox without storage keeps nothing and throws nothing.
    saveScreen(`k`, { a: 1 }, null)
    expect(loadScreen(`k`, null)).toBeNull()
  })

  it(`a throttle is not a lease refusal, nor is a failed tool`, () => {
    expect(isLeaseRefusal(new HostRefusal(`App interaction authority expired`))).toBe(true)
    expect(isLeaseRefusal(new HostRefusal(`concurrency limit reached`))).toBe(false)
    expect(isLeaseRefusal(new Error(`View closed`))).toBe(false)
  })

  it(`expires when the host answers but sends neither input nor result`, () => {
    const { result } = renderHook(() => useViewLease<{ kind: string }>(`issues`))
    act(() => result.current.onContext({ toolInfo: { id: `call-1` } }))
    act(() => void vi.advanceTimersByTime(4_999))
    expect(result.current.expired).toBe(false)
    act(() => void vi.advanceTimersByTime(1))
    expect(result.current.expired).toBe(true)
    expect(result.current.restored).toBeNull()
  })

  it(`a running call gets the long wait, and its result settles the lease`, () => {
    const { result } = renderHook(() => useViewLease<{ kind: string }>(`issues`))
    act(() => result.current.onContext({}))
    act(() => result.current.onToolInput())
    act(() => void vi.advanceTimersByTime(30_000))
    expect(result.current.expired).toBe(false)
    act(() => result.current.onToolResult())
    act(() => void vi.advanceTimersByTime(120_000))
    expect(result.current.expired).toBe(false)
  })

  it(`draws the call's last screen again after a reconstruction`, () => {
    const first = renderHook(() => useViewLease<{ kind: string; n: number }>(`issues`))
    act(() => first.result.current.onContext({ toolInfo: { id: `call-9` } }))
    act(() => first.result.current.onToolResult())
    act(() => first.result.current.keep({ kind: `issues`, n: 3 }))
    first.unmount()
    // The reconstructed card: same call id, no result.
    const again = renderHook(() => useViewLease<{ kind: string; n: number }>(`issues`))
    act(() => again.result.current.onContext({ toolInfo: { id: `call-9` } }))
    act(() => void vi.advanceTimersByTime(5_000))
    expect(again.result.current.expired).toBe(true)
    expect(again.result.current.restored).toEqual({ kind: `issues`, n: 3 })
    // Another call's card has nothing to restore.
    const other = renderHook(() => useViewLease<{ kind: string; n: number }>(`issues`))
    act(() => other.result.current.onContext({ toolInfo: { id: `call-10` } }))
    act(() => void vi.advanceTimersByTime(5_000))
    expect(other.result.current.restored).toBeNull()
  })

  it(`a refused first call expires the view: no retry, no further call`, async () => {
    vi.useRealTimers()
    const { result } = renderHook(() => useViewLease<{ kind: string }>(`issues`))
    const host = vi.fn(async () => {
      throw new HostRefusal(`This view can no longer call tools`)
    })
    const actions = actionsFromBridge(result.current.guard(host), () => {}, { retryDelayMs: 1 })
    let answer: Awaited<ReturnType<typeof actions.call>> | undefined
    await act(async () => {
      answer = await actions.call(`exponential_issues_get`, { id: `x` })
    })
    expect(answer).toEqual({ kind: `error`, message: `This view can no longer call tools` })
    expect(host).toHaveBeenCalledTimes(1)
    expect(result.current.expired).toBe(true)
    await act(async () => {
      answer = await actions.call(`exponential_issues_get`, { id: `x` })
    })
    expect(answer).toEqual({ kind: `error`, message: EXPIRED_LINE })
    expect(host).toHaveBeenCalledTimes(1)
  })

  it(`a refusal after a call that worked, a throttle or a tool error leave the view live`, async () => {
    vi.useRealTimers()
    const { result } = renderHook(() => useViewLease<{ kind: string }>(`issues`))
    const answers: Array<() => Promise<{ isError?: boolean; content: Array<{ type: string; text: string }> }>> = [
      async () => ({ isError: true, content: [{ type: `text`, text: `Issue not found` }] }),
      async () => {
        throw new HostRefusal(`Tool not allowed`)
      },
    ]
    const guarded = result.current.guard(() => answers.shift()!())
    await act(async () => {
      await guarded(`a`, {})
      await guarded(`b`, {}).catch(() => undefined)
    })
    expect(result.current.expired).toBe(false)
    const throttled = renderHook(() => useViewLease<{ kind: string }>(`issues`))
    await act(async () => {
      await throttled.result.current.guard(async () => {
        throw new HostRefusal(`concurrency limit reached`)
      })(`a`, {}).catch(() => undefined)
    })
    expect(throttled.result.current.expired).toBe(false)
  })

  it(`the bridge tells a host refusal from a result`, async () => {
    vi.useRealTimers()
    const parent = { postMessage: vi.fn() }
    const listeners: Array<(event: MessageEvent) => void> = []
    const self = { parent, addEventListener: (_: string, fn: (event: MessageEvent) => void) => listeners.push(fn), removeEventListener: () => {}, open: () => null } as unknown as Window
    const bridge = new AppBridge({}, self)
    const call = bridge.callTool(`exponential_issues`, { op: `list` })
    const sent = parent.postMessage.mock.calls[0][0] as { id: number }
    listeners[0]({ source: parent, data: { jsonrpc: `2.0`, id: sent.id, error: { code: -32000, message: `expired` } } } as unknown as MessageEvent)
    await expect(call).rejects.toBeInstanceOf(HostRefusal)
  })
})
