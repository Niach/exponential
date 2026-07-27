import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  SupportConversationView,
  retryAfterSeconds,
} from "@/routes/support/$token"

// Lives under a `-` prefix so the route generator ignores it (src/routes/**
// is scanned for routes; `-` is the documented escape hatch).

const thread = {
  subject: `Login is broken`,
  boardName: null,
  teamName: `Acme`,
  closed: false,
  reporterName: `Ada`,
  messages: [
    {
      id: `m1`,
      direction: `inbound` as const,
      body: `Hello`,
      createdAt: new Date().toISOString(),
    },
  ],
}

const jsonResponse = (status: number, body: unknown, headers?: HeadersInit) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": `application/json`, ...headers },
  })

function mockFetch(handlers: {
  thread?: () => Response
  reply?: () => Response
}) {
  // `_init` is unused, but declaring it is what types `mock.calls` entries as
  // 2-tuples so assertions can read the request body off `[1]`.
  const fetchMock = vi.fn(
    async (input: RequestInfo | URL, _init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith(`/api/support/thread`)) {
        return handlers.thread?.() ?? jsonResponse(200, thread)
      }
      if (url.endsWith(`/api/support/reply`)) {
        return handlers.reply?.() ?? jsonResponse(200, { ok: true })
      }
      // The live poll — never the subject of these tests.
      return jsonResponse(200, { closed: false, messages: [] })
    }
  )
  vi.stubGlobal(`fetch`, fetchMock)
  return fetchMock
}

const replyCalls = (fetchMock: ReturnType<typeof mockFetch>) =>
  fetchMock.mock.calls.filter(([input]) =>
    String(input).endsWith(`/api/support/reply`)
  )

describe(`retryAfterSeconds`, () => {
  it(`falls back and clamps`, () => {
    expect(retryAfterSeconds(`12`)).toBe(12)
    expect(retryAfterSeconds(null)).toBe(5)
    expect(retryAfterSeconds(`nonsense`)).toBe(5)
    expect(retryAfterSeconds(`0`)).toBe(5)
    expect(retryAfterSeconds(`-3`)).toBe(5)
    expect(retryAfterSeconds(`99999`)).toBe(60)
  })
})

describe(`SupportConversationView`, () => {
  beforeEach(() => {
    vi.unstubAllGlobals()
    Element.prototype.scrollIntoView = vi.fn()
  })

  it(`lets Enter insert a newline instead of sending`, async () => {
    const fetchMock = mockFetch({})
    render(<SupportConversationView token="tok" />)
    const composer = await screen.findByPlaceholderText(`Write a reply…`)

    fireEvent.change(composer, { target: { value: `first line` } })
    const notPrevented = fireEvent.keyDown(composer, {
      key: `Enter`,
      shiftKey: false,
    })

    // The textarea's own default (newline insertion) must survive.
    expect(notPrevented).toBe(true)
    expect(replyCalls(fetchMock)).toHaveLength(0)
  })

  it(`sends on ⌘/Ctrl+Enter but not mid-IME-composition`, async () => {
    const fetchMock = mockFetch({})
    render(<SupportConversationView token="tok" />)
    const composer = await screen.findByPlaceholderText(`Write a reply…`)
    fireEvent.change(composer, { target: { value: `ready to send` } })

    fireEvent.keyDown(composer, {
      key: `Enter`,
      metaKey: true,
      isComposing: true,
    })
    expect(replyCalls(fetchMock)).toHaveLength(0)

    fireEvent.keyDown(composer, { key: `Enter`, metaKey: true })
    await waitFor(() => expect(replyCalls(fetchMock)).toHaveLength(1))
    expect(JSON.parse(String(replyCalls(fetchMock)[0]?.[1]?.body))).toEqual({
      token: `tok`,
      body: `ready to send`,
    })

    fireEvent.change(composer, { target: { value: `and again` } })
    fireEvent.keyDown(composer, { key: `Enter`, ctrlKey: true })
    await waitFor(() => expect(replyCalls(fetchMock)).toHaveLength(2))
  })

  it(`offers a retry instead of a dead end when the load is throttled`, async () => {
    let throttle = true
    const fetchMock = mockFetch({
      thread: () =>
        throttle
          ? jsonResponse(
              429,
              { error: `Too many requests` },
              { "Retry-After": `30` }
            )
          : jsonResponse(200, thread),
    })
    render(<SupportConversationView token="tok" />)

    expect(await screen.findByText(`Support is busy right now`)).toBeTruthy()
    expect(screen.getByText(/Retrying in 30s/)).toBeTruthy()

    throttle = false
    fireEvent.click(screen.getByRole(`button`, { name: `Try again` }))

    expect(await screen.findByText(`Hello`)).toBeTruthy()
    expect(fetchMock).toHaveBeenCalled()
  })

  it(`keeps the transcript when the post-send refresh is rejected`, async () => {
    let loads = 0
    mockFetch({
      thread: () => {
        loads += 1
        return loads === 1
          ? jsonResponse(200, thread)
          : jsonResponse(429, { error: `Too many requests` })
      },
    })
    render(<SupportConversationView token="tok" />)
    const composer = await screen.findByPlaceholderText(`Write a reply…`)
    fireEvent.change(composer, { target: { value: `hi again` } })
    fireEvent.keyDown(composer, { key: `Enter`, metaKey: true })

    await waitFor(() => expect(loads).toBe(2))
    expect(screen.getByText(`Hello`)).toBeTruthy()
    expect(screen.queryByText(`Support is busy right now`)).toBeNull()
  })
})
