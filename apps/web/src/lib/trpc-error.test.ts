import { describe, expect, it } from "vitest"
import { TRPCClientError } from "@trpc/client"
import {
  isTransportError,
  OFFLINE_ERROR_MESSAGE,
  trpcErrorCode,
  trpcErrorMessage,
} from "@/lib/trpc-error"

// A tRPC call that never reached the server: the httpBatchLink has no response
// to shape `data` from, and carries the failed fetch's TypeError as the cause.
function transportError(message = `Failed to fetch`): TRPCClientError<never> {
  const error = new TRPCClientError(message, { cause: new TypeError(message) })
  return error as TRPCClientError<never>
}

// A real server answer: tRPC fills `data` with the code and HTTP status.
function serverError(
  code: string,
  message: string,
  httpStatus = 400
): TRPCClientError<never> {
  const error = new TRPCClientError(message)
  ;(error as { data?: unknown }).data = { code, httpStatus }
  return error as TRPCClientError<never>
}

describe(`isTransportError`, () => {
  it(`recognises a fetch that never reached the server`, () => {
    expect(isTransportError(transportError())).toBe(true)
    // Safari words the same failure differently.
    expect(isTransportError(transportError(`Load failed`))).toBe(true)
  })

  it(`does not treat a server answer as an outage`, () => {
    expect(isTransportError(serverError(`FORBIDDEN`, `Not a member of this team`))).toBe(
      false
    )
  })

  it(`does not treat a deliberate abort as an outage`, () => {
    const aborted = new DOMException(`The user aborted a request.`, `AbortError`)
    const error = new TRPCClientError(`aborted`, { cause: aborted })
    expect(isTransportError(error)).toBe(false)
  })

  it(`ignores anything that is not a TRPCClientError`, () => {
    expect(isTransportError(new TypeError(`Failed to fetch`))).toBe(false)
    expect(isTransportError(undefined)).toBe(false)
  })
})

describe(`trpcErrorMessage`, () => {
  it(`answers the shared offline sentence for a transport failure`, () => {
    // EXP-533: never Chrome's "Failed to fetch" or Safari's "Load failed".
    expect(trpcErrorMessage(transportError(), `fallback`)).toBe(
      OFFLINE_ERROR_MESSAGE
    )
    expect(OFFLINE_ERROR_MESSAGE).toBe(
      `You're offline. Check your connection and try again.`
    )
  })

  it(`surfaces the server's own sentence`, () => {
    expect(
      trpcErrorMessage(
        serverError(`FORBIDDEN`, `Not a member of this team`),
        `fallback`
      )
    ).toBe(`Not a member of this team`)
  })

  it(`falls back for a serialized zod error and for a non-tRPC error`, () => {
    expect(
      trpcErrorMessage(serverError(`BAD_REQUEST`, `[{"path":["title"]}]`), `fallback`)
    ).toBe(`fallback`)
    expect(trpcErrorMessage(new Error(`boom`), `fallback`)).toBe(`fallback`)
  })
})

describe(`trpcErrorCode`, () => {
  it(`reads the server's code and nothing else`, () => {
    expect(trpcErrorCode(serverError(`CONFLICT`, `conflict`, 409))).toBe(`CONFLICT`)
    expect(trpcErrorCode(transportError())).toBeUndefined()
    expect(trpcErrorCode(new Error(`boom`))).toBeUndefined()
  })
})
