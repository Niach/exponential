import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-879: the token-gated screenshot upload. It is anonymous by design — the
// signed token IS the credential — so the 401 has to land BEFORE any database
// work, and the row's STATUS is deliberately not re-checked here (the gate ran
// at mint time; a run may end while curl is in flight).

const h = vi.hoisted(() => {
  const unlocked: { current: unknown[] } = { current: [] }
  const locked: { current: unknown[] } = { current: [] }
  const deletedRows: { current: Array<{ storageKey: string }> } = { current: [] }
  const select = vi.fn()
  const insertValues = vi.fn(async () => undefined)
  const deleteWhere = vi.fn()
  const updateSet = vi.fn()
  const transaction = vi.fn()
  const prepareSessionImage = vi.fn()
  const rollbackSessionImage = vi.fn(async () => undefined)
  const deleteObject = vi.fn(async () => undefined)
  return {
    unlocked,
    locked,
    deletedRows,
    select,
    insertValues,
    deleteWhere,
    updateSet,
    transaction,
    prepareSessionImage,
    rollbackSessionImage,
    deleteObject,
  }
})

/** A chainable, thenable drizzle query stub resolving to `rows`. */
function builder(rows: () => unknown[]) {
  const query: Record<string, unknown> = {}
  for (const method of [`from`, `where`, `limit`, `for`, `orderBy`]) {
    query[method] = vi.fn(() => query)
  }
  ;(query as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) => Promise.resolve(rows()).then(resolve, reject)
  return query
}

vi.mock(`@/db/connection`, () => {
  const tx = {
    select: () => builder(() => h.locked.current),
    insert: () => ({ values: h.insertValues }),
    delete: () => ({
      where: (...args: unknown[]) => {
        h.deleteWhere(...args)
        return { returning: async () => h.deletedRows.current }
      },
    }),
    update: () => ({
      set: (values: unknown) => {
        h.updateSet(values)
        return { where: async () => undefined }
      },
    }),
  }
  return {
    db: {
      select: (...args: unknown[]) => {
        h.select(...args)
        return builder(() => h.unlocked.current)
      },
      transaction: (cb: (tx: unknown) => Promise<unknown>) => {
        h.transaction()
        return cb(tx)
      },
    },
    pool: {},
  }
})

vi.mock(`@/lib/storage`, () => ({ deleteObject: h.deleteObject }))

// The multipart/storage half is exercised by session-attachment-upload.test.ts;
// here it is a seam so the route's transaction is what the test drives.
vi.mock(`@/lib/storage/session-attachment-upload`, () => ({
  prepareSessionImage: h.prepareSessionImage,
  rollbackSessionImage: h.rollbackSessionImage,
  sessionAttachmentValues: (
    prepared: { attachmentId: string; storageKey: string },
    scope: { teamId: string; sessionId: string | null },
    uploaderId: string
  ) => ({
    id: prepared.attachmentId,
    teamId: scope.teamId,
    sessionId: scope.sessionId,
    uploaderId,
    storageKey: prepared.storageKey,
  }),
}))

// Token minting/verification runs for real: the route accepting a signed URL
// as a complete credential IS the security boundary.
vi.stubEnv(`BETTER_AUTH_SECRET`, `session-result-route-test-secret`)

import { Route } from "@/routes/api/session-results/$token"
import { mintSessionResultToken } from "@/lib/storage/session-result-token"

type Handler = (args: {
  params: { token: string }
  request: Request
}) => Promise<Response>

const handler = (
  Route as unknown as {
    options: { server: { handlers: { POST: Handler } } }
  }
).options.server.handlers.POST

const SESSION = `00000000-0000-4000-8000-000000000001`
const TEAM = `00000000-0000-4000-8000-0000000000ff`

function token(over: Partial<Parameters<typeof mintSessionResultToken>[0]> = {}) {
  return mintSessionResultToken({
    sessionId: SESSION,
    topic: `chatui`,
    label: `web`,
    userId: `user-1`,
    ...over,
  }).token
}

function post(value: string) {
  return handler({
    params: { token: value },
    request: new Request(`https://example.com/api/session-results/${value}`, {
      method: `POST`,
    }),
  })
}

const prepared = {
  attachmentId: `aaaaaaaa-0000-4000-8000-000000000001`,
  filename: `shot.png`,
  contentType: `image/png`,
  sizeBytes: 12,
  storageKey: `sessions/${SESSION}/shot.png`,
  url: `/api/attachments/aaaaaaaa-0000-4000-8000-000000000001`,
  width: 1600,
  height: 900,
}

beforeEach(() => {
  vi.clearAllMocks()
  h.unlocked.current = [{ id: SESSION, teamId: TEAM }]
  h.locked.current = [{ id: SESSION, results: null }]
  h.deletedRows.current = []
  h.prepareSessionImage.mockResolvedValue(prepared)
})

describe(`POST /api/session-results/$token`, () => {
  it(`401s a bad or expired token without touching the database`, async () => {
    for (const bad of [`not-a-token`, ``, `${token()}x`]) {
      const response = await post(bad)
      expect(response.status).toBe(401)
    }
    expect(h.select).not.toHaveBeenCalled()
    expect(h.prepareSessionImage).not.toHaveBeenCalled()
  })

  it(`404s once the run's row is gone`, async () => {
    h.unlocked.current = []
    const response = await post(token())
    expect(response.status).toBe(404)
    expect(h.prepareSessionImage).not.toHaveBeenCalled()
  })

  it(`appends the picture and answers with the run's list`, async () => {
    const response = await post(token())
    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({
      ok: true,
      id: prepared.attachmentId,
      url: `/api/attachments/${prepared.attachmentId}`,
      width: 1600,
      height: 900,
      topic: `chatui`,
      label: `web`,
      replaced: false,
      results: [{ topic: `chatui`, label: `web` }],
    })
    // The row is uploaded by the user the TOKEN names, on the run's team.
    expect(h.insertValues).toHaveBeenCalledWith(
      expect.objectContaining({
        id: prepared.attachmentId,
        teamId: TEAM,
        sessionId: SESSION,
        uploaderId: `user-1`,
      })
    )
    expect(h.updateSet).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [
          {
            topic: `chatui`,
            label: `web`,
            attachmentId: prepared.attachmentId,
            width: 1600,
            height: 900,
          },
        ],
      })
    )
    expect(h.deleteObject).not.toHaveBeenCalled()
  })

  it(`replaces the same topic and label, reclaiming the old blob after commit`, async () => {
    h.locked.current = [
      {
        id: SESSION,
        results: [
          { topic: `chatui`, label: `web`, attachmentId: `old`, width: 1, height: 1 },
          { topic: `nav`, label: `web`, attachmentId: `keep`, width: 1, height: 1 },
        ],
      },
    ]
    h.deletedRows.current = [{ storageKey: `sessions/old.png` }]

    const response = await post(token())
    const body = (await response.json()) as { replaced: boolean; results: unknown }
    expect(response.status).toBe(200)
    expect(body.replaced).toBe(true)
    expect(body.results).toEqual([
      { topic: `chatui`, label: `web` },
      { topic: `nav`, label: `web` },
    ])
    // Replaced IN PLACE, and the displaced object only goes after the commit.
    expect(h.updateSet).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [
          expect.objectContaining({ attachmentId: prepared.attachmentId }),
          expect.objectContaining({ attachmentId: `keep` }),
        ],
      })
    )
    expect(h.deleteObject).toHaveBeenCalledWith(`sessions/old.png`)
  })

  it(`409s over the cap and reclaims the object it just uploaded`, async () => {
    h.locked.current = [
      {
        id: SESSION,
        results: Array.from({ length: 60 }, (_, index) => ({
          topic: `t`,
          label: `l${index}`,
          attachmentId: `a${index}`,
          width: 1,
          height: 1,
        })),
      },
    ]
    const response = await post(token())
    expect(response.status).toBe(409)
    expect(h.insertValues).not.toHaveBeenCalled()
    expect(h.rollbackSessionImage).toHaveBeenCalledWith(prepared.storageKey)
  })

  it(`surfaces a rejected file as its own status and stores nothing`, async () => {
    h.prepareSessionImage.mockRejectedValue(
      new TRPCError({ code: `BAD_REQUEST`, message: `Missing file` })
    )
    const response = await post(token())
    expect(response.status).toBe(400)
    expect(h.transaction).not.toHaveBeenCalled()
  })

  it(`does not care that the run already ended (the gate ran at mint time)`, async () => {
    h.unlocked.current = [{ id: SESSION, teamId: TEAM }]
    h.locked.current = [{ id: SESSION, results: [], status: `ended` }]
    const response = await post(token())
    expect(response.status).toBe(200)
  })
})
