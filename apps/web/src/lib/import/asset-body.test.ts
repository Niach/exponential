import { describe, expect, it } from "vitest"
import { readBodyBounded } from "@/lib/import/asset-body"

// EXP-630: the bounded body reader — a wrong or missing content-length must
// not let a download buffer past the cap.

function response(chunks: Uint8Array[], contentLength?: string) {
  let cancelled = false
  const stream = new ReadableStream<Uint8Array>({
    pull(controller) {
      const next = chunks.shift()
      if (next) controller.enqueue(next)
      else controller.close()
    },
    cancel() {
      cancelled = true
    },
  })
  return {
    response: {
      headers: { get: (name: string) => (name === `content-length` ? (contentLength ?? null) : null) },
      body: stream,
    },
    wasCancelled: () => cancelled,
  }
}

describe(`readBodyBounded`, () => {
  it(`concatenates the chunks of a body under the cap`, async () => {
    const { response: r } = response([new Uint8Array([1, 2]), new Uint8Array([3])])
    expect(Array.from((await readBodyBounded(r, 10))!)).toEqual([1, 2, 3])
  })

  it(`rejects on the declared length before reading a byte`, async () => {
    const { response: r, wasCancelled } = response([new Uint8Array([1])], `11`)
    expect(await readBodyBounded(r, 10)).toBeNull()
    expect(wasCancelled()).toBe(false)
  })

  it(`stops and cancels the stream once the observed bytes pass the cap, whatever the header said`, async () => {
    const { response: r, wasCancelled } = response(
      [new Uint8Array(6), new Uint8Array(6), new Uint8Array(6)],
      `3`
    )
    expect(await readBodyBounded(r, 10)).toBeNull()
    expect(wasCancelled()).toBe(true)
  })

  it(`treats a missing body as empty`, async () => {
    expect((await readBodyBounded({ headers: { get: () => null }, body: null }, 10))!.byteLength).toBe(0)
  })
})
