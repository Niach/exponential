import { describe, expect, it } from "vitest"
import { buildAppleRevokeBody } from "./apple"

// The x-www-form-urlencoded body Apple's /auth/revoke endpoint expects. The
// network call is best-effort/fire-and-forget, but the body shape is exact —
// a wrong field name silently no-ops the revocation, so pin it here.

describe(`buildAppleRevokeBody`, () => {
  it(`encodes all four required fields`, () => {
    const body = buildAppleRevokeBody({
      clientId: `at.exponential`,
      clientSecret: `secret.jwt.value`,
      token: `refresh-token-123`,
      tokenTypeHint: `refresh_token`,
    })
    const params = new URLSearchParams(body)
    expect(params.get(`client_id`)).toBe(`at.exponential`)
    expect(params.get(`client_secret`)).toBe(`secret.jwt.value`)
    expect(params.get(`token`)).toBe(`refresh-token-123`)
    expect(params.get(`token_type_hint`)).toBe(`refresh_token`)
  })

  it(`url-encodes values with reserved characters`, () => {
    const body = buildAppleRevokeBody({
      clientId: `id`,
      clientSecret: `a.b+c/d=`,
      token: `tok en&x`,
      tokenTypeHint: `access_token`,
    })
    // The raw body must escape '+', '/', '&', '=' so Apple parses one field.
    expect(body).not.toContain(`a.b+c/d=`)
    expect(body).not.toContain(`tok en&x`)
    const params = new URLSearchParams(body)
    expect(params.get(`client_secret`)).toBe(`a.b+c/d=`)
    expect(params.get(`token`)).toBe(`tok en&x`)
    expect(params.get(`token_type_hint`)).toBe(`access_token`)
  })
})
