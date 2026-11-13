import { afterEach, describe, expect, it, vi } from "vitest"
import {
  CLIENT_CLOSED_MESSAGE,
  CLIENT_CLOSED_STATUS,
  isClientAbort,
  isClientAbortEvent,
  logRequestFailure,
  markRequestStart,
  routeLabel,
} from "./client-abort"
import { errorToResponse } from "./http-errors"

/** Exactly what Bun hands a handler whose client hung up (prod, sha-4a1fffa). */
const bunClientClosed = () => new DOMException(CLIENT_CLOSED_MESSAGE, `AbortError`)

function requestWithSignal(url: string, signal?: AbortSignal, method = `POST`): Request {
  return new Request(url, { method, signal })
}

afterEach(() => vi.restoreAllMocks())

describe(`isClientAbort`, () => {
  it(`is Bun's connection-closed AbortError, with or without the request`, () => {
    expect(isClientAbort(bunClientClosed())).toBe(true)
    expect(isClientAbort(bunClientClosed(), requestWithSignal(`http://x/api/a`))).toBe(true)
  })

  it(`is any AbortError once the request's own signal fired`, () => {
    const client = new AbortController()
    const request = requestWithSignal(`http://x/api/a`, client.signal)
    client.abort()
    expect(isClientAbort(new DOMException(`aborted`, `AbortError`), request)).toBe(true)
  })

  it(`is never our own abort or timeout while the client is connected`, () => {
    const ours = new AbortController()
    ours.abort()
    const request = requestWithSignal(`http://x/api/a`)
    expect(isClientAbort(ours.signal.reason, request)).toBe(false)
    expect(isClientAbort(new DOMException(`timed out`, `TimeoutError`), request)).toBe(false)
    expect(isClientAbort(new DOMException(`timed out`, `TimeoutError`))).toBe(false)
    expect(isClientAbort(new Error(`boom`), request)).toBe(false)
    expect(isClientAbort(undefined)).toBe(false)
  })
})

describe(`routeLabel`, () => {
  it(`masks capability tokens and ids, keeps the route`, () => {
    expect(routeLabel(`/api/attachment-uploads/eyJhbGciOiJIUzI1NiJ9.eyJhIjoiYiJ9.sig`)).toBe(`/api/attachment-uploads/:id`)
    expect(routeLabel(`/api/issues/0f9c2b1e-6a1d-4f7e-9d1a-3c2b1a0f9e8d/files`)).toBe(`/api/issues/:id/files`)
    expect(routeLabel(`/api/teams/abc/icon`)).toBe(`/api/teams/abc/icon`)
  })
})

describe(`logRequestFailure`, () => {
  it(`logs a client abort as one debug line with the route, never an error`, () => {
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    const debug = vi.spyOn(console, `debug`).mockImplementation(() => {})
    const request = requestWithSignal(`http://x/api/issues/abc/files?secret=1`)
    expect(logRequestFailure(bunClientClosed(), request)).toBe(true)
    expect(error).not.toHaveBeenCalled()
    expect(debug).toHaveBeenCalledWith(`[client-abort] POST /api/issues/abc/files`)
  })

  it(`logs our own abort as an error with method, route and duration`, () => {
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    const request = requestWithSignal(`http://x/api/share/abc`, undefined, `GET`)
    markRequestStart(request, performance.now() - 1500)
    const ours = new DOMException(`timed out`, `TimeoutError`)
    expect(logRequestFailure(ours, request)).toBe(false)
    expect(error).toHaveBeenCalledTimes(1)
    const [line, logged] = error.mock.calls[0]!
    expect(line).toMatch(/^\[request aborted\] GET \/api\/share\/abc after 1[5-9]\d\dms$/)
    expect(logged).toBe(ours)
  })

  it(`logs any other failure as a request error with context`, () => {
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    const boom = new Error(`boom`)
    logRequestFailure(boom, requestWithSignal(`http://x/api/a`))
    expect(error).toHaveBeenCalledWith(`[request error] POST /api/a`, boom)
  })
})

describe(`errorToResponse`, () => {
  it(`answers a client abort with 499 and no error log`, () => {
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    vi.spyOn(console, `debug`).mockImplementation(() => {})
    const response = errorToResponse(bunClientClosed(), requestWithSignal(`http://x/api/issues/abc/files`))
    expect(response.status).toBe(CLIENT_CLOSED_STATUS)
    expect(error).not.toHaveBeenCalled()
  })

  it(`keeps a real failure a logged 500`, () => {
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    const response = errorToResponse(new Error(`boom`), requestWithSignal(`http://x/api/a`))
    expect(response.status).toBe(500)
    expect(error).toHaveBeenCalledTimes(1)
  })
})

describe(`isClientAbortEvent (GlitchTip beforeSend)`, () => {
  it(`drops Bun's connection-closed AbortError`, () => {
    expect(isClientAbortEvent({ exception: { values: [{ type: `AbortError`, value: CLIENT_CLOSED_MESSAGE }] } })).toBe(true)
  })

  it(`keeps our own aborts, timeouts and everything else`, () => {
    expect(isClientAbortEvent({ exception: { values: [{ type: `AbortError`, value: `This operation was aborted` }] } })).toBe(false)
    expect(isClientAbortEvent({ exception: { values: [{ type: `TimeoutError`, value: `The operation timed out.` }] } })).toBe(false)
    expect(isClientAbortEvent({ exception: { values: [{ type: `Error`, value: `boom` }] } })).toBe(false)
    expect(isClientAbortEvent({})).toBe(false)
    expect(isClientAbortEvent(null)).toBe(false)
  })
})
