import { describe, expect, it } from "vitest"

// Regression lock for the /auth/login + /auth/register open redirect:
// `?redirect=` flows into `window.location.href` after sign-in, so only
// same-origin absolute paths may survive. Anything scheme-bearing
// (`javascript:`, `https://evil.example`), protocol-relative (`//host`),
// backslash-escaped (`/\host`), or containing ASCII control chars (HTML URL
// parsing strips tab/newline BEFORE parsing) must become undefined so the
// sinks' `|| '/'` fallback takes over.

import { sanitizeRedirectPath } from "@/lib/auth/safe-redirect"

describe(`sanitizeRedirectPath`, () => {
  it.each([
    `/`,
    `/t/acme/boards/exp`,
    // legacy path form — still a valid same-origin redirect target
    `/w/acme/boards/exp`,
    // invite/$token.tsx producer shape
    `/invite/abc123`,
    // MCP OAuth resume-shaped relative URL
    `/api/auth/mcp/authorize?client_id=x&response_type=code&state=a`,
    // consent.tsx beforeLoad producer shape (ParsedLocation.href is relative)
    `/auth/consent?client_id=x&scope=a%20b`,
    // colon only inside the query string is fine — no scheme
    `/path?next=https://ok.example`,
  ])(`accepts same-origin absolute path %j unchanged`, (value) => {
    expect(sanitizeRedirectPath(value)).toBe(value)
  })

  it.each([
    `https://evil.example`,
    `http://evil.example/w/acme`,
    `javascript:alert(1)`,
    `JaVaScRiPt:alert(1)`,
    `mailto:a@b.example`,
    `data:text/html,x`,
    `//evil.example`,
    `//evil.example/path`,
    // browsers normalize backslash to slash in http(s) URLs → //evil.example
    `/\\evil.example`,
    // URL parsing strips embedded tab/newline BEFORE parsing → //evil.example
    `/\t/evil.example`,
    `\t/foo`,
    `/foo\nbar`,
    ``,
  ])(`rejects unsafe value %j`, (value) => {
    expect(sanitizeRedirectPath(value)).toBeUndefined()
  })

  it.each([undefined, null, 42, {}, [`/`], true])(
    `rejects non-string value %j`,
    (value) => {
      expect(sanitizeRedirectPath(value)).toBeUndefined()
    }
  )
})
