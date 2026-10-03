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
  teamName: `Acme`,
  status: `open` as `open` | `resolved`,
  reporterName: `Ada`,
  report: {
    title: `Login is broken`,
    // Stored as server-escaped GFM; the page shows the plain text.
    description: `It loops back \\#3 times\nand \\*never\\* signs in\\.`,
    createdAt: new Date().toISOString(),
  },
  attachments: [
    {
      id: `a1`,
      filename: `shot.png`,
      contentType: `image/png`,
      width: 800,
      height: 600,
    },
  ],
  messages: [
    {
      id: `m1`,
      direction: `inbound` as const,
      body: `Hello`,
      createdAt: new Date().toISOString(),
    },
    {
      id: `m2`,
      direction: `outbound` as const,
      body: `On it.`,
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
  attachment?: () => Response
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
      if (url.endsWith(`/api/support/attachment`)) {
        return (
          handlers.attachment?.() ??
          new Response(new Blob([new Uint8Array([1])], { type: `image/png` }))
        )
      }
      return jsonResponse(404, { error: `nope` })
    }
  )
  vi.stubGlobal(`fetch`, fetchMock)
  return fetchMock
}

const replyCalls = (fetchMock: ReturnType<typeof mockFetch>) =>
  fetchMock.mock.calls.filter(([input]) =>
    String(input).endsWith(`/api/support/reply`)
  )
const attachmentCalls = (fetchMock: ReturnType<typeof mockFetch>) =>
  fetchMock.mock.calls.filter(([input]) =>
    String(input).endsWith(`/api/support/attachment`)
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
    vi.stubGlobal(`URL`, {
      ...URL,
      createObjectURL: vi.fn(() => `blob:shot`),
      revokeObjectURL: vi.fn(),
    })
  })

  // SLOP-4: the report block — plain text (escapes dropped), the pictures
  // fetched through the token-gated POST route, the status pill, and the
  // reply box that stays even when resolved (a reply reopens).
  it(`shows the report as plain text with its pictures and status`, async () => {
    const fetchMock = mockFetch({})
    render(<SupportConversationView token="tok" />)

    expect(await screen.findByText(`It loops back #3 times`, { exact: false })).toBeTruthy()
    expect(screen.getByText(/and \*never\* signs in\./)).toBeTruthy()
    expect(screen.queryByText(/\\#3/)).toBeNull()
    expect(screen.getByTestId(`report-status`).textContent).toBe(`Open`)
    expect(screen.getByText(`Hello`)).toBeTruthy()
    expect(screen.getByText(`On it.`)).toBeTruthy()

    await waitFor(() => expect(attachmentCalls(fetchMock)).toHaveLength(1))
    expect(
      JSON.parse(String(attachmentCalls(fetchMock)[0]?.[1]?.body))
    ).toEqual({ token: `tok`, id: `a1` })
    await waitFor(() =>
      expect(screen.getByAltText(`shot.png`).getAttribute(`src`)).toBe(
        `blob:shot`
      )
    )
  })

  it(`keeps the reply box on a resolved report`, async () => {
    mockFetch({
      thread: () => jsonResponse(200, { ...thread, status: `resolved` }),
    })
    render(<SupportConversationView token="tok" />)
    expect(await screen.findByPlaceholderText(`Write a reply…`)).toBeTruthy()
    expect(screen.getByTestId(`report-status`).textContent).toBe(`Resolved`)
    expect(screen.getByText(/Replying reopens it/)).toBeTruthy()
  })

  it(`tells a forged or retired link that the conversation moved`, async () => {
    mockFetch({ thread: () => jsonResponse(404, { error: `nope` }) })
    render(<SupportConversationView token="tok" />)
    expect(
      await screen.findByText(
        `This conversation moved. Please write to us again from where you first reached out.`
      )
    ).toBeTruthy()
    expect(screen.queryByPlaceholderText(`Write a reply…`)).toBeNull()
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

    expect(await screen.findByText(`We're busy right now`)).toBeTruthy()
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
    expect(screen.queryByText(`We're busy right now`)).toBeNull()
  })
})
