// @vitest-environment node
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { appRecords, apps, attachments, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import type { RecordSchema } from "@exponential_app/eui/host"

// VAPP-221 (eApps R7): files on records, against the real migrated
// Postgres: a record may reference only its own app's and team's files, and
// the sweep reclaims an app file no record of its app (live or trashed)
// names once it is past the grace window.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'attachments' AND column_name = 'app_id') AS ok
`)
if (!ready.ok) console.info(`[app-files.real-db] skipped: ${ready.reason}`)

const { assertRecordAttachments } = await import(`@/lib/apps/app-files`)
const { reclaimUnreferencedAppFilesInTx, reclaimOrphanSessionAttachmentsInTx } = await import(`@/lib/session-attachment-sweep`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const schema: RecordSchema = {
  type: `object`,
  properties: { receipt: { type: `string`, format: `attachment` }, photos: { type: `array`, items: { type: `string`, format: `attachment` } } },
}
const ref = (id: string) => `/api/attachments/${id}`
const old = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000)

async function seed(tx: RealDbTx) {
  const [a, b] = await tx
    .insert(teams)
    .values([
      { name: `A`, slug: `files-a-${id8()}` },
      { name: `B`, slug: `files-b-${id8()}` },
    ])
    .returning({ id: teams.id })
  const [a1, a2, b1] = await tx
    .insert(apps)
    .values([
      { teamId: a!.id, slug: `one`, name: `One` },
      { teamId: a!.id, slug: `two`, name: `Two` },
      { teamId: b!.id, slug: `one`, name: `One` },
    ])
    .returning({ id: apps.id })
  const file = async (teamId: string, appId: string | null, createdAt = old) => {
    const id = crypto.randomUUID()
    await tx.insert(attachments).values({ id, teamId, appId, filename: `f.png`, contentType: `image/png`, sizeBytes: 1, storageKey: `k/${id}`, url: ref(id), createdAt })
    return id
  }
  return { teamA: a!.id, a1: a1!.id, a2: a2!.id, teamB: b!.id, b1: b1!.id, file }
}

const refusal = async (fn: () => Promise<unknown>) => {
  try {
    await fn()
    return null
  } catch (e) {
    return (e as Error).message
  }
}

describe.skipIf(!ready.ok)(`app files (real db)`, () => {
  it(`a record references only its own app's and team's files`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      const own = await w.file(w.teamA, w.a1)
      const otherApp = await w.file(w.teamA, w.a2)
      const otherTeam = await w.file(w.teamB, w.b1)
      const plain = await w.file(w.teamA, null)
      const app = { id: w.a1, teamId: w.teamA }
      expect(await refusal(() => assertRecordAttachments(tx, app, [{ schema, data: { receipt: ref(own), photos: [ref(own)] } }]))).toBeNull()
      for (const id of [otherApp, otherTeam, plain]) {
        expect(await refusal(() => assertRecordAttachments(tx, app, [{ schema, data: { photos: [ref(own), ref(id)] } }]))).toMatch(/files uploaded to this app/)
      }
      // A field that is no attachment field references nothing.
      expect(await refusal(() => assertRecordAttachments(tx, app, [{ schema: { type: `object`, properties: { note: { type: `string` } } }, data: { note: ref(otherTeam) } }]))).toBeNull()
    })
  })

  it(`the sweep keeps files a live or trashed record names and reclaims the rest past the window`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      const live = await w.file(w.teamA, w.a1)
      const trashed = await w.file(w.teamA, w.a1)
      const dropped = await w.file(w.teamA, w.a1)
      const fresh = await w.file(w.teamA, w.a1, new Date())
      const namedByOtherApp = await w.file(w.teamA, w.a1)
      await tx.insert(appRecords).values([
        { appId: w.a1, teamId: w.teamA, kind: `expense`, data: { receipt: ref(live) } },
        { appId: w.a1, teamId: w.teamA, kind: `expense`, data: { photos: [ref(trashed)] }, deletedAt: new Date() },
        { appId: w.a2, teamId: w.teamA, kind: `expense`, data: { receipt: ref(namedByOtherApp) } },
      ])
      const cutoff = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000)
      // The orphan sweep never takes an app's file.
      const orphanKeys = (await reclaimOrphanSessionAttachmentsInTx(tx, cutoff)).storageKeys
      expect(orphanKeys.filter((k) => [live, trashed, dropped, fresh, namedByOtherApp].some((id) => k.endsWith(id)))).toEqual([])
      const { storageKeys } = await reclaimUnreferencedAppFilesInTx(tx, cutoff)
      expect(storageKeys.filter((k) => [live, trashed, dropped, fresh, namedByOtherApp].some((id) => k.endsWith(id))).sort()).toEqual([`k/${dropped}`, `k/${namedByOtherApp}`].sort())
      const left = await tx.execute<{ id: string }>(sql`SELECT id FROM attachments WHERE app_id = ${w.a1}`)
      expect(left.rows.map((r) => r.id).sort()).toEqual([live, trashed, fresh].sort())
    })
  })

  it(`deleting the app hands its files to the orphan sweep`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      const id = await w.file(w.teamA, w.a2)
      await tx.execute(sql`DELETE FROM apps WHERE id = ${w.a2}`)
      const row = await tx.execute<{ app_id: string | null }>(sql`SELECT app_id FROM attachments WHERE id = ${id}`)
      expect(row.rows[0]?.app_id).toBeNull()
    })
  })
})
