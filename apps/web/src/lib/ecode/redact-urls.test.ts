import { describe, expect, it } from "vitest"
import { redactUrlQueries } from "@/lib/ecode/redact-urls"

describe(`redactUrlQueries (#EXP-2191)`, () => {
  it(`drops a presigned URL's signature but keeps where it failed`, () => {
    const text = `http 0: error sending request for url (https://s3.example.com/team/packs/c5fd6aa5.pack?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIA%2F20261127%2Feu&X-Amz-Signature=0123abcd)`
    const out = redactUrlQueries(text)
    expect(out).toBe(`http 0: error sending request for url (https://s3.example.com/team/packs/c5fd6aa5.pack)`)
    expect(out).not.toMatch(/X-Amz|0123abcd/)
  })

  it(`handles several URLs, JSON quoting and fragments; leaves other text alone`, () => {
    const json = JSON.stringify({ ok: false, message: `GET https://a.example/x?sig=1 then https://b.example/y#t=2 failed` })
    expect(redactUrlQueries(json)).toBe(JSON.stringify({ ok: false, message: `GET https://a.example/x then https://b.example/y failed` }))
    expect(redactUrlQueries(`no url? fine`)).toBe(`no url? fine`)
    expect(redactUrlQueries(null)).toBeNull()
  })
})
