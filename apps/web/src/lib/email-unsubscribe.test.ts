import { describe, expect, it, vi } from "vitest"
import { handleUnsubscribe } from "@/lib/email-unsubscribe"

describe(`handleUnsubscribe`, () => {
  it(`400s on a missing token without touching the resolver`, async () => {
    const unsubscribe = vi.fn()
    const res = await handleUnsubscribe(null, unsubscribe)
    expect(res.status).toBe(400)
    expect(res.headers.get(`Content-Type`)).toContain(`text/html`)
    expect(unsubscribe).not.toHaveBeenCalled()
  })

  it(`flips the pref and confirms on a known token (POST)`, async () => {
    const unsubscribe = vi.fn().mockResolvedValue(true)
    const res = await handleUnsubscribe(`tok-1`, unsubscribe, `POST`)
    expect(unsubscribe).toHaveBeenCalledWith(`tok-1`)
    expect(res.status).toBe(200)
    const html = await res.text()
    expect(html).toContain(`unsubscribed`)
  })

  it(`GET never mutates — renders a confirm form instead (scanner prefetch safety)`, async () => {
    const unsubscribe = vi.fn()
    const res = await handleUnsubscribe(`tok-1`, unsubscribe, `GET`)
    expect(unsubscribe).not.toHaveBeenCalled()
    expect(res.status).toBe(200)
    const html = await res.text()
    expect(html).toContain(`method="post"`)
    // The token stays in the query string — never embedded in the markup.
    expect(html).not.toContain(`tok-1`)
  })

  it(`defaults to the mutating path when no method is given (RFC 8058 one-click compat)`, async () => {
    const unsubscribe = vi.fn().mockResolvedValue(true)
    const res = await handleUnsubscribe(`tok-1`, unsubscribe)
    expect(unsubscribe).toHaveBeenCalledWith(`tok-1`)
    expect(res.status).toBe(200)
  })

  it(`404s on an unknown token`, async () => {
    const unsubscribe = vi.fn().mockResolvedValue(false)
    const res = await handleUnsubscribe(`nope`, unsubscribe)
    expect(res.status).toBe(404)
  })

  it(`never throws — a resolver failure becomes a friendly 500`, async () => {
    const unsubscribe = vi.fn().mockRejectedValue(new Error(`db down`))
    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => {})
    const res = await handleUnsubscribe(`tok-1`, unsubscribe)
    expect(res.status).toBe(500)
    errorSpy.mockRestore()
  })
})
