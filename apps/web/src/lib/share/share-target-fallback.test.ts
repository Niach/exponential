// @vitest-environment node
// EXP-1635: `/api/share-target` with no service worker: never a 500, text
// survives, files do not.
import { describe, expect, it } from "vitest"
import { shareTargetFallback } from "@/lib/share/share-target-fallback"

const post = (body: BodyInit, headers: Record<string, string>) =>
  new Request(`https://app.test/api/share-target`, { method: `POST`, body, headers })

async function multipart(fields: Record<string, string | File>) {
  const form = new FormData()
  for (const [name, value] of Object.entries(fields)) form.append(name, value)
  const probe = new Response(form)
  const bytes = await probe.arrayBuffer()
  return post(bytes, {
    "content-type": probe.headers.get(`content-type`)!,
    "content-length": String(bytes.byteLength),
  })
}

describe(`shareTargetFallback`, () => {
  it(`text fields ride /share's search params`, async () => {
    const response = await shareTargetFallback(
      await multipart({ title: `T`, text: `a b`, url: `https://x.test/?q=1` })
    )
    expect(response.status).toBe(303)
    const location = new URL(response.headers.get(`location`)!, `https://app.test`)
    expect(location.pathname).toBe(`/share`)
    expect(Object.fromEntries(location.searchParams)).toEqual({
      title: `T`,
      text: `a b`,
      url: `https://x.test/?q=1`,
    })
  })

  it(`files are dropped, fields kept`, async () => {
    const response = await shareTargetFallback(
      await multipart({ text: `hi`, files: new File([`x`], `a.png`, { type: `image/png` }) })
    )
    expect(response.headers.get(`location`)).toBe(`/share?text=hi`)
  })

  it(`a long field is cut to fit a redirect`, async () => {
    const response = await shareTargetFallback(await multipart({ text: `x`.repeat(5_000) }))
    expect(new URL(response.headers.get(`location`)!, `https://app.test`).searchParams.get(`text`)).toHaveLength(2_000)
  })

  it(`a body too big to be text goes unread`, async () => {
    const response = await shareTargetFallback(post(`x`, { "content-length": String(10 * 1024 * 1024) }))
    expect(response.status).toBe(303)
    expect(response.headers.get(`location`)).toBe(`/share`)
  })

  it(`an unparsable body never throws`, async () => {
    const response = await shareTargetFallback(post(`junk`, { "content-type": `multipart/form-data; boundary=x`, "content-length": `4` }))
    expect(response.status).toBe(303)
    expect(response.headers.get(`location`)).toBe(`/share`)
  })
})
