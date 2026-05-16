import { describe, expect, it } from "vitest"
import { parseSessionTokenFromSetCookie } from "./session-cookie"

describe(`parseSessionTokenFromSetCookie`, () => {
  it(`returns null when no Set-Cookie header is present`, () => {
    const headers = new Headers()
    expect(parseSessionTokenFromSetCookie(headers)).toBeNull()
  })

  it(`extracts a session token from a single Set-Cookie header`, () => {
    const headers = new Headers()
    headers.append(
      `set-cookie`,
      `session_token=abc123; Path=/; HttpOnly; Secure; SameSite=Lax`
    )
    expect(parseSessionTokenFromSetCookie(headers)).toBe(`abc123`)
  })

  it(`finds the session token when multiple Set-Cookie headers are present`, () => {
    const headers = new Headers()
    headers.append(`set-cookie`, `other=foo; Path=/`)
    headers.append(
      `set-cookie`,
      `session_token=token-xyz; Path=/; HttpOnly`
    )
    headers.append(`set-cookie`, `csrf=bar; Path=/`)
    expect(parseSessionTokenFromSetCookie(headers)).toBe(`token-xyz`)
  })

  it(`returns null when no Set-Cookie entry carries session_token`, () => {
    const headers = new Headers()
    headers.append(`set-cookie`, `other=foo; Path=/`)
    headers.append(`set-cookie`, `csrf=bar; Path=/`)
    expect(parseSessionTokenFromSetCookie(headers)).toBeNull()
  })

  it(`decodes URL-encoded session token values`, () => {
    const headers = new Headers()
    headers.append(
      `set-cookie`,
      `session_token=abc%2F123; Path=/; HttpOnly`
    )
    expect(parseSessionTokenFromSetCookie(headers)).toBe(`abc/123`)
  })
})
