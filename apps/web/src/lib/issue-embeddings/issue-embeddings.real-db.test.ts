// @vitest-environment node
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { boards, comments, issues, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import embeddingModel from "../../../embedder/model.json"
import { embedQuery, embedText, stopEmbedder } from "./embedder"
import { ISSUE_EMBEDDING_MODEL } from "./input"
import { recomputeIssueEmbedding, runIssueEmbeddingSweep, staleIssueEmbeddingIds } from "./recompute"
import { issueEmbeddingsTableExists, nearestIssueIds } from "./store"

// #NEW-134 against a REAL migrated Postgres, each case in one rolled-back
// transaction. The boot SQL (db/out/custom/0002) is applied INSIDE the case,
// so the suite proves both databases:
//
//   * a stock Postgres (the gate's `postgres:18`, every existing install): the
//     boot SQL is a no-op and every entry point answers "nothing";
//   * a Postgres with pgvector (the derived image): table + HNSW index,
//     recompute, the sweep, kNN.
//
// PGVECTOR_REQUIRED (the CI job on the derived image) FAILS the suite when the
// extension is missing instead of running the stock half; EMBEDDING_MODEL_REQUIRED
// does the same for the weights the kNN cases need.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'bump_issue_updated_at_from_comment') AS ok
`)
if (!ready.ok) console.info(`[issue-embeddings.real-db] skipped: ${ready.reason}`)

const bootSql = readFileSync(join(process.cwd(), `src`, `db`, `out`, `custom`, `0002_issue_embeddings.sql`), `utf8`)

const hasVector = ready.ok
  ? (await probeRealDb(`SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'vector') AS ok`).catch(() => ({ ok: false }))).ok
  : false
if (ready.ok && !hasVector && process.env.PGVECTOR_REQUIRED) {
  throw new Error(`pgvector is required but this Postgres does not ship it`)
}
const modelDir = join(process.cwd(), `embedder`, `models`, `multilingual-e5-small`)
const hasModel = embeddingModel.source.files.every((file) => existsSync(join(modelDir, file.name)))
if (!hasModel && process.env.EMBEDDING_MODEL_REQUIRED) {
  throw new Error(`the embedding model is required but not in ${modelDir}`)
}

const id8 = () => crypto.randomUUID().slice(0, 8)

async function world(tx: RealDbTx) {
  const userId = `embed-test-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@embed.test`}, true, false, now(), now())
  `)
  const team = async () => {
    const [row] = await tx.insert(teams).values({ name: `Embed`, slug: `embed-${id8()}` }).returning({ id: teams.id })
    await tx.insert(teamMembers).values({ teamId: row!.id, userId, role: `owner` })
    const [board] = await tx
      .insert(boards)
      .values({ teamId: row!.id, name: `Core`, slug: `core-${id8()}`, prefix: `EM` })
      .returning({ id: boards.id })
    return { teamId: row!.id, boardId: board!.id }
  }
  const home = await team()
  const issue = async (title: string, description: string | null = null, at = home) =>
    (await tx.insert(issues).values({ boardId: at.boardId, teamId: at.teamId, title, description, creatorId: userId }).returning({ id: issues.id }))[0]!.id
  const comment = async (issueId: string, body: string) =>
    (await tx.insert(comments).values({ issueId, teamId: home.teamId, boardId: home.boardId, authorId: userId, body }).returning({ id: comments.id }))[0]!.id
  return { userId, ...home, team, issue, comment }
}

/** A deterministic stand-in for the model: counts its calls. */
function fakeEmbedder() {
  const inputs: string[] = []
  const embed = async (input: string) => {
    inputs.push(input)
    const vector = new Float32Array(embeddingModel.dimensions)
    vector[inputs.length % embeddingModel.dimensions] = 1
    return vector
  }
  return { inputs, embed }
}

const row = async (tx: RealDbTx, issueId: string) =>
  (
    await tx.execute<{ model: string; content_hash: string; team_id: string; fresh: boolean }>(sql`
      select e.model, e.content_hash, e.team_id, e.updated_at = i.updated_at as fresh
      from issue_embeddings e join issues i on i.id = e.issue_id where e.issue_id = ${issueId}::uuid`)
  ).rows[0]

/** "The issue was written again after it was embedded", inside one
 *  transaction (where now() stands still): move the row's stamp back. */
const makeStale = (tx: RealDbTx, issueId: string) =>
  tx.execute(sql`update issue_embeddings set updated_at = updated_at - interval '1 minute' where issue_id = ${issueId}::uuid`)

describe.skipIf(!ready.ok || hasVector)(`issue embeddings on a Postgres WITHOUT pgvector`, () => {
  it(`the boot SQL is a no-op and every entry point answers "nothing"`, async () => {
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(bootSql))
      await tx.execute(sql.raw(bootSql))
      expect(await issueEmbeddingsTableExists(tx)).toBe(false)
      const { teamId, issue } = await world(tx)
      await issue(`Login button does nothing`)
      const fake = fakeEmbedder()
      expect(await runIssueEmbeddingSweep({ executor: tx, embed: fake.embed, settleSeconds: 0 })).toEqual({
        candidates: 0,
        embedded: 0,
        unchanged: 0,
        skipped: 0,
        unavailable: true,
      })
      expect(fake.inputs).toEqual([])
      expect(await nearestIssueIds(new Float32Array(embeddingModel.dimensions), { teamId }, {}, tx)).toEqual([])
    })
  })
})

describe.skipIf(!ready.ok || !hasVector)(`issue embeddings on a Postgres WITH pgvector`, () => {
  afterAll(() => stopEmbedder())

  it(`the boot SQL creates the extension, the table and the HNSW index, idempotently`, async () => {
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(bootSql))
      await tx.execute(sql.raw(bootSql))
      expect(await issueEmbeddingsTableExists(tx)).toBe(true)
      const indexes = await tx.execute<{ indexdef: string }>(
        sql`select indexdef from pg_indexes where tablename = 'issue_embeddings' order by indexname`
      )
      const defs = indexes.rows.map((r) => r.indexdef).join(`\n`)
      expect(defs).toContain(`USING hnsw (embedding vector_cosine_ops)`)
      expect(defs).toContain(`(team_id)`)
      const column = await tx.execute<{ type: string }>(sql`
        select format_type(atttypid, atttypmod) as type from pg_attribute
        where attrelid = 'issue_embeddings'::regclass and attname = 'embedding'`)
      expect(column.rows[0]?.type).toBe(`vector(${embeddingModel.dimensions})`)
      // Server-only: in no publication (Electric never sees it).
      const published = await tx.execute(sql`select 1 from pg_publication_tables where tablename = 'issue_embeddings'`)
      expect(published.rows).toHaveLength(0)
    })
  })

  it(`recompute embeds once, skips an unchanged hash, follows title, description and comments`, async () => {
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(bootSql))
      const { teamId, issue, comment } = await world(tx)
      const id = await issue(`Login button does nothing`, `Clicking **Sign in** has no effect.`)
      const fake = fakeEmbedder()
      const options = { executor: tx, embed: fake.embed }

      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      expect(fake.inputs).toEqual([`Login button does nothing\nClicking **Sign in** has no effect.`])
      const first = await row(tx, id)
      expect(first).toMatchObject({ model: ISSUE_EMBEDDING_MODEL, team_id: teamId, fresh: true })

      // Nothing changed: no inference.
      expect(await recomputeIssueEmbedding(id, options)).toBe(`unchanged`)
      expect(fake.inputs).toHaveLength(1)

      // A change that is not text (the sweep's common case): no inference,
      // and the row is fresh again.
      await tx.update(issues).set({ priority: `high` }).where(eq(issues.id, id))
      await makeStale(tx, id)
      expect((await row(tx, id))?.fresh).toBe(false)
      expect(await recomputeIssueEmbedding(id, options)).toBe(`unchanged`)
      expect(fake.inputs).toHaveLength(1)
      expect((await row(tx, id))?.fresh).toBe(true)

      await tx.update(issues).set({ title: `Sign-in button does nothing` }).where(eq(issues.id, id))
      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      await tx.update(issues).set({ description: null }).where(eq(issues.id, id))
      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      expect(fake.inputs.at(-1)).toBe(`Sign-in button does nothing`)
      const beforeComments = (await row(tx, id))!.content_hash

      const first_comment = await comment(id, `Happens on Safari 18 only`)
      await comment(id, `Fixed by #EM-2`)
      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      expect(fake.inputs.at(-1)).toBe(`Sign-in button does nothing\nHappens on Safari 18 only\nFixed by #EM-2`)

      await tx.update(comments).set({ body: `Happens on Safari 18 and 17` }).where(eq(comments.id, first_comment))
      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      expect(fake.inputs.at(-1)).toContain(`Safari 18 and 17`)

      // Deleting the comments brings the old hash back.
      await tx.delete(comments).where(eq(comments.issueId, id))
      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      expect((await row(tx, id))!.content_hash).toBe(beforeComments)
      expect(fake.inputs).toHaveLength(6)
    })
  })

  it(`a trashed or missing issue is skipped; a hard delete takes the row along`, async () => {
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(bootSql))
      const { issue } = await world(tx)
      const fake = fakeEmbedder()
      const options = { executor: tx, embed: fake.embed }
      const id = await issue(`Doomed`)
      expect(await recomputeIssueEmbedding(id, options)).toBe(`embedded`)
      await tx.update(issues).set({ deletedAt: new Date() }).where(eq(issues.id, id))
      expect(await recomputeIssueEmbedding(id, options)).toBe(`skipped`)
      expect(await recomputeIssueEmbedding(crypto.randomUUID(), options)).toBe(`skipped`)
      await tx.delete(issues).where(eq(issues.id, id))
      expect(await row(tx, id)).toBeUndefined()
      // An embedder that answers null changes nothing.
      const other = await issue(`Other`)
      expect(await recomputeIssueEmbedding(other, { executor: tx, embed: async () => null })).toBe(`unavailable`)
      expect(await row(tx, other)).toBeUndefined()
    })
  })

  it(`the sweep (and the backfill on top of it) finds what is missing or stale, in batches, and resumes`, async () => {
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(bootSql))
      const { teamId, issue, comment, team } = await world(tx)
      const ids = [await issue(`One`), await issue(`Two`), await issue(`Three`), await issue(`Four`), await issue(`Five`)]
      const trashed = await issue(`Trashed`)
      await tx.update(issues).set({ deletedAt: new Date() }).where(eq(issues.id, trashed))
      const elsewhere = await team()
      await issue(`Another team's`, null, elsewhere)
      const fake = fakeEmbedder()
      const options = { executor: tx, embed: fake.embed, settleSeconds: 0, teamId }

      // Fresh edits are left to the debounce.
      expect(await staleIssueEmbeddingIds({ executor: tx, limit: 50, teamId, settleSeconds: 30 })).toEqual([])
      expect((await staleIssueEmbeddingIds({ executor: tx, limit: 50, teamId, settleSeconds: 0 })).sort()).toEqual([...ids].sort())

      // Batch one, an "interrupted" run: the embedder goes away after one.
      let calls = 0
      const flaky = async (input: string) => (calls++ === 0 ? fake.embed(input) : null)
      expect(await runIssueEmbeddingSweep({ ...options, embed: flaky, limit: 2 })).toMatchObject({ candidates: 2, embedded: 1, unavailable: true })
      // The resumed run takes the remaining four, two at a time.
      expect(await runIssueEmbeddingSweep({ ...options, limit: 2 })).toMatchObject({ candidates: 2, embedded: 2, unavailable: false })
      expect(await runIssueEmbeddingSweep({ ...options, limit: 2 })).toMatchObject({ candidates: 2, embedded: 2 })
      expect(await runIssueEmbeddingSweep({ ...options, limit: 2 })).toMatchObject({ candidates: 0, embedded: 0 })
      const count = await tx.execute<{ n: number }>(sql`select count(*)::int as n from issue_embeddings where team_id = ${teamId}::uuid`)
      expect(count.rows[0]?.n).toBe(5)

      // A comment bumps its issue (the trigger); once the stamp is behind,
      // the sweep re-embeds exactly that one.
      await comment(ids[0]!, `new detail`)
      await makeStale(tx, ids[0]!)
      await makeStale(tx, ids[1]!)
      const before = fake.inputs.length
      expect(await runIssueEmbeddingSweep(options)).toMatchObject({ candidates: 2, embedded: 1, unchanged: 1 })
      expect(fake.inputs.slice(before)).toEqual([`One\nnew detail`])

      // A row of another model is stale too.
      await tx.execute(sql`update issue_embeddings set model = 'old-model/0' where issue_id = ${ids[2]!}::uuid`)
      expect(await runIssueEmbeddingSweep(options)).toMatchObject({ candidates: 1, embedded: 1 })
      expect((await row(tx, ids[2]!))?.model).toBe(ISSUE_EMBEDDING_MODEL)
    })
  })

  it.skipIf(!hasModel)(
    `kNN with the real model: German and English issues about the same bug are neighbours, inside the scope only`,
    async () => {
      await inRollback(async (tx) => {
        await tx.execute(sql.raw(bootSql))
        const { teamId, boardId, issue, comment, team } = await world(tx)
        const login = {
          en: await issue(`Login button does nothing on Safari`, `Clicking "Sign in" on the login page has no effect in Safari 18. Chrome works.`),
          de: await issue(`Anmelde-Knopf reagiert in Safari nicht`, `Ein Klick auf „Anmelden“ bewirkt in Safari 18 nichts, in Chrome funktioniert es.`),
        }
        const push = {
          en: await issue(`Push notifications arrive twice on Android`, `Every mention sends two identical notifications on Pixel phones.`),
          de: await issue(`Push-Benachrichtigungen kommen auf Android doppelt an`, `Jede Erwähnung löst auf Pixel-Geräten zwei gleiche Benachrichtigungen aus.`),
        }
        const csv = await issue(`Export the board as a CSV file`, `A download button in the board menu that writes all issues to CSV.`)
        const dark = await issue(`Dunkles Farbschema für die Einstellungen`, `Die Einstellungsseite ignoriert das dunkle Design und bleibt hell.`)
        // Found through its COMMENT only.
        const vague = await issue(`Something is off after the update`)
        await comment(vague, `The PDF invoice shows the wrong VAT rate for Austrian customers.`)
        const trashed = await issue(`Login broken in Safari (duplicate)`, `Sign in does nothing in Safari.`)
        const elsewhere = await team()
        const foreign = await issue(`Login button does nothing on Safari`, `Clicking "Sign in" has no effect in Safari.`, elsewhere)

        // The real model, through the worker (no `embed` override).
        const sweep = await runIssueEmbeddingSweep({ executor: tx, settleSeconds: 0, limit: 50, teamId })
        expect(sweep).toMatchObject({ candidates: 8, embedded: 8, unavailable: false })
        expect(await runIssueEmbeddingSweep({ executor: tx, settleSeconds: 0, teamId: elsewhere.teamId })).toMatchObject({ embedded: 1 })
        await tx.update(issues).set({ deletedAt: new Date() }).where(eq(issues.id, trashed))

        const names = new Map<string, string>([
          [login.en, `login.en`], [login.de, `login.de`], [push.en, `push.en`], [push.de, `push.de`],
          [csv, `csv`], [dark, `dark`], [vague, `vague`], [trashed, `trashed`], [foreign, `foreign`],
        ])
        const search = async (query: string, scope: Parameters<typeof nearestIssueIds>[1] = { teamId }, options = {}) => {
          const vector = await embedQuery(query)
          expect(vector).toHaveLength(embeddingModel.dimensions)
          const hits = await nearestIssueIds(vector!, scope, { maxDistance: 1, ...options }, tx)
          console.info(`[issue-embeddings.real-db] ${JSON.stringify(query)} → ${hits.map((hit) => `${names.get(hit.issueId)} ${hit.distance.toFixed(3)}`).join(`, `)}`)
          return hits.map((hit) => names.get(hit.issueId))
        }

        // The same bug in two languages: both on top, either way round.
        expect((await search(`Anmeldung in Safari funktioniert nicht`)).slice(0, 2).sort()).toEqual([`login.de`, `login.en`])
        expect((await search(`can't sign in with Safari`)).slice(0, 2).sort()).toEqual([`login.de`, `login.en`])
        expect((await search(`duplicate push notification on Android`)).slice(0, 2).sort()).toEqual([`push.de`, `push.en`])
        expect((await search(`doppelte Mitteilungen auf dem Handy`)).slice(0, 2).sort()).toEqual([`push.de`, `push.en`])
        expect((await search(`dark mode in settings`))[0]).toBe(`dark`)
        expect((await search(`Rechnung mit falscher Mehrwertsteuer`))[0]).toBe(`vague`)

        // An issue's own vector finds its translation first (similar issues).
        const own = await embedText(`passage`, `Login button does nothing on Safari\nClicking "Sign in" on the login page has no effect in Safari 18. Chrome works.`)
        const similar = await nearestIssueIds(own!, { teamId }, { maxDistance: 1, k: 3 }, tx)
        expect(similar.map((hit) => names.get(hit.issueId))).toEqual([`login.en`, `login.de`, expect.any(String)])
        expect(similar[0]!.distance).toBeLessThan(0.001)

        // The scope: this team's live issues only; the trashed one and the
        // other team's never appear, whatever the cutoff.
        const all = await search(`login safari`, { teamId }, { k: 50 })
        expect(all).toHaveLength(7)
        expect(all).not.toContain(`trashed`)
        expect(all).not.toContain(`foreign`)
        expect(await search(`login safari`, { teamIds: [elsewhere.teamId] })).toEqual([`foreign`])
        expect((await search(`login safari`, { boardIds: [boardId] }, { k: 50 }))).toHaveLength(7)
        expect(await search(`login safari`, { teamIds: [] })).toEqual([])
        // k and the distance cutoff.
        expect(await search(`login safari`, { teamId }, { k: 2 })).toHaveLength(2)
        expect(await search(`login safari`, { teamId }, { maxDistance: 0 })).toEqual([])

        // The same answer through the HNSW index (the planner prefers a
        // sequential scan on nine rows).
        await tx.execute(sql`set local enable_seqscan = off`)
        await tx.execute(sql`set local enable_bitmapscan = off`)
        expect((await search(`Anmeldung in Safari funktioniert nicht`, { boardIds: [boardId] })).slice(0, 2).sort()).toEqual([`login.de`, `login.en`])
      })
    },
    180_000
  )
})
