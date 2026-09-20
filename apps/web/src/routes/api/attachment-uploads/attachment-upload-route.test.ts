import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-929: the token-gated issue-attachment upload. Anonymous by design — the
// signed token IS the credential — so the 401 lands BEFORE any database work,
// and the row is inserted HERE from the token's scope once the bytes land
// (never at mint time), with the real size.

const h = vi.hoisted(() => {
  // One result set per db.select(), in call order.
  const selects: { queue: unknown[][] } = { queue: [] }
  const select = vi.fn()
  const insertValues = vi.fn(async () => undefined)
  const uploadObject = vi.fn(async () => undefined)
  const deleteObject = vi.fn(async () => undefined)
  const assertWithinStorageLimit = vi.fn(async () => undefined)
  const getImageDimensions = vi.fn(() => ({ width: 640, height: 480 }))
  return {
    selects,
    select,
    insertValues,
    uploadObject,
    deleteObject,
    assertWithinStorageLimit,
    getImageDimensions,
  }
})

function builder(rows: () => unknown[]) {
  const query: Record<string, unknown> = {}
  for (const method of [`from`, `where`, `limit`]) {
    query[method] = vi.fn(() => query)
  }
  ;(query as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) => Promise.resolve(rows()).then(resolve, reject)
  return query
}

vi.mock(`@/db/connection`, () => ({
  db: {
    select: (...args: unknown[]) => {
      h.select(...args)
      const rows = h.selects.queue.shift() ?? []
      return builder(() => rows)
    },
    insert: () => ({ values: h.insertValues }),
  },
  pool: {},
}))

vi.mock(`@/lib/storage`, () => ({
  uploadObject: h.uploadObject,
  deleteObject: h.deleteObject,
}))
vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))
vi.mock(`@/lib/storage/image-dimensions`, () => ({
  getImageDimensions: h.getImageDimensions,
}))

// Token minting/verification runs for real: the route accepting a signed URL
// as a complete credential IS the security boundary.
vi.stubEnv(`BETTER_AUTH_SECRET`, `attachment-upload-route-test-secret`)

import { Route } from "@/routes/api/attachment-uploads/$token"
import { mintAttachmentUploadToken } from "@/lib/storage/attachment-upload-token"

type Handler = (args: {
  params: { token: string }
  request: Request
}) => Promise<Response>

const handlers = (
  Route as unknown as {
    options: { server: { handlers: { PUT: Handler; POST: Handler } } }
  }
).options.server.handlers

const ATTACHMENT = `aaaaaaaa-0000-4000-8000-000000000001`
const ISSUE = `00000000-0000-4000-8000-000000000001`
const BOARD = `00000000-0000-4000-8000-0000000000b0`
const TEAM = `00000000-0000-4000-8000-0000000000ff`

function token(
  over: Partial<Parameters<typeof mintAttachmentUploadToken>[0]> = {}
) {
  return mintAttachmentUploadToken({
    attachmentId: ATTACHMENT,
    issueId: ISSUE,
    boardId: BOARD,
    teamId: TEAM,
    userId: `user-1`,
    filename: `shot.png`,
    contentType: `image/png`,
    ...over,
  }).token
}

function put(value: string, body?: BodyInit, headers?: Record<string, string>) {
  return handlers.PUT({
    params: { token: value },
    request: new Request(`https://example.com/api/attachment-uploads/${value}`, {
      method: `PUT`,
      body,
      headers,
      // undici requires it for a stream body; harmless for bytes.
      ...(body instanceof ReadableStream ? { duplex: `half` } : {}),
    } as RequestInit),
  })
}

const BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5, 6, 7, 8])

beforeEach(() => {
  vi.clearAllMocks()
  // Default: no row for the id yet, the issue exists.
  h.selects.queue = [[], [{ id: ISSUE }]]
})

describe(`PUT /api/attachment-uploads/$token`, () => {
  it(`401s a bad or expired token without touching the database or storage`, async () => {
    for (const bad of [`not-a-token`, ``, `${token()}x`]) {
      const response = await put(bad, BYTES)
      expect(response.status).toBe(401)
    }
    expect(h.select).not.toHaveBeenCalled()
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`stores the bytes under the token's scope and inserts the row with the real size`, async () => {
    const response = await put(token(), BYTES)
    expect(response.status).toBe(200)
    const url = `/api/attachments/${ATTACHMENT}`
    expect(await response.json()).toEqual({
      ok: true,
      id: ATTACHMENT,
      url,
      filename: `shot.png`,
      contentType: `image/png`,
      sizeBytes: BYTES.byteLength,
      width: 640,
      height: 480,
      durationMs: null,
      next: `exponential_attachments_upload({ attachmentId: "${ATTACHMENT}" })`,
    })
    expect(h.assertWithinStorageLimit).toHaveBeenCalledWith(TEAM, BYTES.byteLength)
    expect(h.uploadObject).toHaveBeenCalledWith(
      expect.objectContaining({
        key: `issues/${ISSUE}/${ATTACHMENT}-shot.png`,
        contentType: `image/png`,
        contentLength: BYTES.byteLength,
      })
    )
    // Every column comes from the TOKEN, not the request: the id the tool
    // announced, the issue/board/team it checked, the user it authorized.
    expect(h.insertValues).toHaveBeenCalledWith({
      id: ATTACHMENT,
      teamId: TEAM,
      boardId: BOARD,
      issueId: ISSUE,
      commentId: null,
      uploaderId: `user-1`,
      filename: `shot.png`,
      contentType: `image/png`,
      sizeBytes: BYTES.byteLength,
      storageKey: `issues/${ISSUE}/${ATTACHMENT}-shot.png`,
      url,
      width: 640,
      height: 480,
      durationMs: null,
    })
  })

  it(`ignores the request's own Content-Type in favour of the token's`, async () => {
    const response = await put(token(), BYTES, {
      "content-type": `application/octet-stream`,
    })
    expect(response.status).toBe(200)
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({ contentType: `image/png` })
    )
  })

  it(`writes the comment link the token carries`, async () => {
    const response = await put(token({ commentId: `comment-1` }), BYTES)
    expect(response.status).toBe(200)
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({ commentId: `comment-1` })
    )
  })

  it(`also takes a multipart "file" part (the sessions_results curl shape)`, async () => {
    // A real multipart Request cannot be built here: jsdom's `File` is not
    // undici's and its parser asserts on it. The route only reads the header
    // and `formData()`, so a stub carries the part.
    const form = new FormData()
    const file = new File([BYTES], `shot.png`, { type: `image/png` })
    // jsdom's File has no arrayBuffer(); the route reads bytes through it.
    Object.defineProperty(file, `arrayBuffer`, {
      value: async () => BYTES.buffer.slice(0),
    })
    form.append(`file`, file)
    const response = await handlers.POST({
      params: { token: token() },
      request: {
        headers: new Headers({
          "content-type": `multipart/form-data; boundary=x`,
        }),
        formData: async () => form,
      } as unknown as Request,
    })
    expect(response.status).toBe(200)
    expect(h.uploadObject).toHaveBeenCalledWith(
      expect.objectContaining({ contentLength: BYTES.byteLength })
    )
  })

  it(`rejects an empty body`, async () => {
    const response = await put(token(), new Uint8Array(0))
    expect(response.status).toBe(400)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`caps images at 10 MB by declared length and by streamed bytes`, async () => {
    const declared = await put(token(), BYTES, {
      "content-length": String(10 * 1024 * 1024 + 1),
    })
    expect(declared.status).toBe(400)
    expect((await declared.json()).error).toMatch(/10 MB/)
    h.selects.queue = [[], [{ id: ISSUE }]]
    // A chunked body that lies by omission: the running total catches it.
    const streamed = await put(
      token(),
      new ReadableStream<Uint8Array>({
        start(controller) {
          const chunk = new Uint8Array(1024 * 1024)
          for (let i = 0; i < 11; i++) controller.enqueue(chunk)
          controller.close()
        },
      })
    )
    expect(streamed.status).toBe(400)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`409s a retry whose first PUT already landed, naming the finalize call`, async () => {
    h.selects.queue = [[{ id: ATTACHMENT }]]
    const response = await put(token(), BYTES)
    expect(response.status).toBe(409)
    expect((await response.json()).error).toContain(`attachmentId: "${ATTACHMENT}"`)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`404s once the issue is gone`, async () => {
    h.selects.queue = [[], []]
    const response = await put(token(), BYTES)
    expect(response.status).toBe(404)
    expect(h.uploadObject).not.toHaveBeenCalled()
  })

  it(`rolls the object back when the row insert fails`, async () => {
    h.insertValues.mockRejectedValueOnce(new Error(`boom`))
    const response = await put(token(), BYTES)
    expect(response.status).toBe(500)
    expect(h.deleteObject).toHaveBeenCalledWith(
      `issues/${ISSUE}/${ATTACHMENT}-shot.png`
    )
  })
})
