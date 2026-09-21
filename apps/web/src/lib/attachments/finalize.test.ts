import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Attachment } from "@/db/schema"

// EXP-988 contract tests for `finalizeAttachmentUpload` (lib/attachments/
// finalize.ts), un-skipped by EXP-955 with the implementation. They run the
// injectable core (finalize-core.ts) against an in-memory `attachments`
// table and a fake object store; finalize.ts only binds the aws-sdk probe.

const h = vi.hoisted(() => ({
  rows: new Map<string, Record<string, unknown>>(),
  assertWithinStorageLimit: vi.fn(),
}))

// A minimal drizzle-shaped fake: every chain this module uses resolves
// against `h.rows` keyed by id. The where clause is opaque, so the fake
// reads the id out of the drizzle `eq()` object it is handed.
vi.mock(`@/db/connection`, () => {
  const idOf = (where: unknown) => {
    // drizzle's eq() builds a SQL chunk whose queryChunks hold the column
    // then the bound value; the last string chunk is the id.
    const chunks = (where as { queryChunks?: unknown[] })?.queryChunks ?? []
    for (let index = chunks.length - 1; index >= 0; index -= 1) {
      const chunk = chunks[index] as { value?: unknown }
      if (typeof chunk?.value === `string`) return chunk.value
    }
    throw new Error(`fake db: no id in where clause`)
  }
  const db = {
    select: () => ({
      from: () => ({
        where: (where: unknown) => ({
          limit: async () => {
            const row = h.rows.get(idOf(where))
            return row ? [{ ...row }] : []
          },
        }),
      }),
    }),
    update: () => ({
      set: (patch: Record<string, unknown>) => ({
        where: (where: unknown) => ({
          returning: async () => {
            const id = idOf(where)
            const row = h.rows.get(id)
            if (!row) return []
            const next = { ...row, ...patch }
            h.rows.set(id, next)
            return [{ ...next }]
          },
        }),
      }),
    }),
    delete: () => ({
      where: async (where: unknown) => {
        h.rows.delete(idOf(where))
      },
    }),
  }
  return { db, pool: {} }
})

vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))

import { TRPCError } from "@trpc/server"
import {
  finalizeAttachmentUploadWith,
  type AttachmentObjectProbe,
} from "@/lib/attachments/finalize-core"
import { getImageDimensions } from "@/lib/storage/image-dimensions"

const ID = `00000000-0000-4000-8000-00000000a001`

function reservedRow(overrides: Partial<Attachment> = {}): Attachment {
  return {
    id: ID,
    teamId: `t-1`,
    boardId: `b-1`,
    issueId: `i-1`,
    draftId: null,
    commentId: null,
    uploaderId: `u-1`,
    filename: `notes.md`,
    contentType: `text/markdown`,
    sizeBytes: 0,
    storageKey: `issues/i-1/${ID}/notes.md`,
    url: `/api/attachments/${ID}`,
    width: null,
    height: null,
    durationMs: null,
    posterStorageKey: null,
    posterSizeBytes: null,
    boardDeletedAt: null,
    boardArchivedAt: null,
    createdAt: new Date(`2026-09-20T10:00:00Z`),
    updatedAt: new Date(`2026-09-20T10:00:00Z`),
    ...overrides,
  } as Attachment
}

/** An in-memory object store keyed by storage key. */
function fakeStore(
  objects: Record<string, { bytes: Uint8Array; contentType?: string | null }>
) {
  const store = new Map(Object.entries(objects))
  const probe: AttachmentObjectProbe = {
    head: vi.fn(async (key: string) => {
      const object = store.get(key)
      return object
        ? {
            sizeBytes: object.bytes.byteLength,
            contentType: object.contentType ?? null,
          }
        : null
    }),
    read: vi.fn(async (key: string) => store.get(key)?.bytes ?? null),
    remove: vi.fn(async (key: string) => {
      store.delete(key)
    }),
  }
  return { store, probe }
}

// A 1x1 PNG header (IHDR width/height = 1) — enough for the dimension probe.
function tinyPng(width: number, height: number) {
  const bytes = new Uint8Array(33)
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a], 0)
  bytes.set([0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52], 8)
  new DataView(bytes.buffer).setUint32(16, width)
  new DataView(bytes.buffer).setUint32(20, height)
  return bytes
}

beforeEach(() => {
  h.rows.clear()
  h.assertWithinStorageLimit.mockReset()
  h.assertWithinStorageLimit.mockResolvedValue(undefined)
})

describe(`finalizeAttachmentUpload (EXP-955)`, () => {
  it(`a signed-URL upload of N bytes reports sizeBytes = N`, async () => {
    // Arrange: a reserved row (sizeBytes 0) whose object holds N bytes.
    const row = reservedRow()
    h.rows.set(ID, row)
    const bytes = new TextEncoder().encode(`# hello\n`.repeat(3000))
    const { probe } = fakeStore({
      [row.storageKey]: { bytes, contentType: `text/markdown; charset=utf-8` },
    })

    // Act: finalizeAttachmentUpload(row.id).
    const finished = await finalizeAttachmentUploadWith(ID, probe)

    // Assert: the returned row and the stored row both read sizeBytes === N,
    // and contentType is the stored object's (canonicalized).
    expect(finished.sizeBytes).toBe(bytes.byteLength)
    expect(finished.contentType).toBe(`text/markdown`)
    expect(h.rows.get(ID)?.sizeBytes).toBe(bytes.byteLength)
    expect(h.rows.get(ID)?.contentType).toBe(`text/markdown`)
    // The team budget was charged exactly the new bytes.
    expect(h.assertWithinStorageLimit).toHaveBeenCalledWith(
      `t-1`,
      bytes.byteLength
    )
    // Nothing was deleted on the happy path.
    expect(probe.remove).not.toHaveBeenCalled()
  })

  it(`refuses a row whose object is not in storage yet`, async () => {
    // A HEAD miss throws; the row stays at 0 so a retry can finalize later.
    const row = reservedRow()
    h.rows.set(ID, row)
    const { probe } = fakeStore({})

    await expect(finalizeAttachmentUploadWith(ID, probe)).rejects.toMatchObject(
      { code: `PRECONDITION_FAILED` }
    )
    expect(h.rows.get(ID)?.sizeBytes).toBe(0)
    expect(probe.remove).not.toHaveBeenCalled()
    expect(h.assertWithinStorageLimit).not.toHaveBeenCalled()
  })

  it(`is idempotent: finalizing twice writes the same numbers`, async () => {
    const row = reservedRow({
      filename: `shot.png`,
      contentType: `image/png`,
      storageKey: `issues/i-1/${ID}/shot.png`,
    })
    h.rows.set(ID, row)
    const bytes = tinyPng(640, 480)
    expect(getImageDimensions(bytes)).toEqual({ width: 640, height: 480 })
    const { probe } = fakeStore({
      [row.storageKey]: { bytes, contentType: `image/png` },
    })

    const first = await finalizeAttachmentUploadWith(ID, probe)
    const second = await finalizeAttachmentUploadWith(ID, probe)

    expect(first.sizeBytes).toBe(bytes.byteLength)
    expect(first.width).toBe(640)
    expect(first.height).toBe(480)
    expect(second).toMatchObject({
      sizeBytes: first.sizeBytes,
      contentType: first.contentType,
      width: 640,
      height: 480,
    })
    // The dimensions were probed ONCE — a finished row is not re-read.
    expect(probe.read).toHaveBeenCalledTimes(1)
    // The second pass adds no bytes, so the budget is not charged again.
    expect(h.assertWithinStorageLimit).toHaveBeenCalledTimes(1)
  })

  it(`backfills a legacy row with sizeBytes = 0 through the same function`, async () => {
    // A pre-EXP-955 row: real object in storage, size never recorded, the
    // store carrying a type the row never had.
    const row = reservedRow({
      filename: `spec.pdf`,
      contentType: `application/octet-stream`,
      storageKey: `issues/i-1/${ID}/spec.pdf`,
      createdAt: new Date(`2025-01-01T00:00:00Z`),
    })
    h.rows.set(ID, row)
    const bytes = new Uint8Array(70 * 1024 * 1024)
    const { probe } = fakeStore({
      [row.storageKey]: { bytes, contentType: `application/pdf` },
    })

    const healed = await finalizeAttachmentUploadWith(ID, probe, {
      mode: `backfill`,
    })

    expect(healed.sizeBytes).toBe(bytes.byteLength)
    expect(healed.contentType).toBe(`application/pdf`)
    expect(h.rows.get(ID)?.sizeBytes).toBe(bytes.byteLength)
    // Backfill mode never refuses (this one is over the 50 MB file cap and
    // would have been refused as an upload), never deletes and never gates
    // on the team budget: the bytes are already there.
    expect(probe.remove).not.toHaveBeenCalled()
    expect(h.rows.has(ID)).toBe(true)
    expect(h.assertWithinStorageLimit).not.toHaveBeenCalled()
  })

  it(`upload mode refuses an over-cap object and drops object + row`, async () => {
    const row = reservedRow()
    h.rows.set(ID, row)
    const { store, probe } = fakeStore({
      [row.storageKey]: {
        bytes: new Uint8Array(50 * 1024 * 1024 + 1),
        contentType: `text/markdown`,
      },
    })

    await expect(finalizeAttachmentUploadWith(ID, probe)).rejects.toMatchObject(
      { code: `BAD_REQUEST`, message: `Files must be 50 MB or smaller` }
    )
    expect(store.has(row.storageKey)).toBe(false)
    expect(h.rows.has(ID)).toBe(false)
  })

  it(`upload mode refuses an empty object`, async () => {
    const row = reservedRow()
    h.rows.set(ID, row)
    const { store, probe } = fakeStore({
      [row.storageKey]: { bytes: new Uint8Array(0), contentType: null },
    })

    await expect(finalizeAttachmentUploadWith(ID, probe)).rejects.toMatchObject(
      { code: `BAD_REQUEST`, message: `File is empty` }
    )
    expect(store.has(row.storageKey)).toBe(false)
    expect(h.rows.has(ID)).toBe(false)
  })

  it(`upload mode forwards a team storage refusal and drops object + row`, async () => {
    const row = reservedRow()
    h.rows.set(ID, row)
    const { store, probe } = fakeStore({
      [row.storageKey]: { bytes: new Uint8Array(1024), contentType: null },
    })
    h.assertWithinStorageLimit.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `Plan limit` })
    )

    await expect(finalizeAttachmentUploadWith(ID, probe)).rejects.toMatchObject(
      { code: `FORBIDDEN` }
    )
    expect(store.has(row.storageKey)).toBe(false)
    expect(h.rows.has(ID)).toBe(false)
  })

  it(`keeps the reserved type when the store recorded none`, async () => {
    const row = reservedRow()
    h.rows.set(ID, row)
    const { probe } = fakeStore({
      [row.storageKey]: { bytes: new Uint8Array(12), contentType: null },
    })

    const finished = await finalizeAttachmentUploadWith(ID, probe)
    expect(finished.contentType).toBe(`text/markdown`)
    expect(finished.sizeBytes).toBe(12)
  })

  it(`is a NOT_FOUND for an unknown row`, async () => {
    const { probe } = fakeStore({})
    await expect(
      finalizeAttachmentUploadWith(`missing`, probe)
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})
