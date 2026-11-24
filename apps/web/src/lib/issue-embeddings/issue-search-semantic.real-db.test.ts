// @vitest-environment node
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { boards, comments, issues, teamMembers, teams } from "@/db/schema"
import { issueSearchMatchIds, type IssueSearchScope } from "@/lib/issue-search-sql"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import embeddingModel from "../../../embedder/model.json"
import { stopEmbedder } from "./embedder"
import { runIssueEmbeddingSweep } from "./recompute"
import {
  ISSUE_SEARCH_SEMANTIC_MAX_HITS,
  issueSearchBackend,
  issueSearchIdsSql,
  resetIssueSearchVectors,
  searchIssues,
} from "./search"
import { nearestIssueIds } from "./store"

// #NEW-135 against a REAL migrated Postgres, each case in one rolled-back
// transaction (the boot SQL of db/out/custom/0002 applied inside it), like
// issue-embeddings.real-db.test.ts beside it:
//
//   * a stock Postgres (the gate's `postgres:18`, every install without
//     pgvector): the search is exactly the lexical one, `similar:` is empty;
//   * Postgres + pgvector with the REAL model (the gate's `embeddings` job):
//     `similar:<ID>` and the semantic hits behind the lexical ones, through
//     both entry points (`searchIssues` = tRPC issues.search,
//     `issueSearchIdsSql` = MCP list `search`).

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'bump_issue_updated_at_from_comment') AS ok
`)
if (!ready.ok) console.info(`[issue-search-semantic.real-db] skipped: ${ready.reason}`)

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
  const userId = `search-test-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@search.test`}, true, false, now(), now())
  `)
  const team = async (prefix: string) => {
    const [row] = await tx.insert(teams).values({ name: `Search`, slug: `search-${id8()}` }).returning({ id: teams.id })
    await tx.insert(teamMembers).values({ teamId: row!.id, userId, role: `owner` })
    const [board] = await tx
      .insert(boards)
      .values({ teamId: row!.id, name: `Core`, slug: `core-${id8()}`, prefix })
      .returning({ id: boards.id })
    return { teamId: row!.id, boardId: board!.id }
  }
  const home = await team(`SM`)
  const issue = async (title: string, description: string | null = null, at = home) => {
    const [row] = await tx
      .insert(issues)
      .values({ boardId: at.boardId, teamId: at.teamId, title, description, creatorId: userId })
      .returning({ id: issues.id, identifier: issues.identifier })
    return row!
  }
  const comment = async (issueId: string, body: string) =>
    tx.insert(comments).values({ issueId, teamId: home.teamId, boardId: home.boardId, authorId: userId, body })
  return { userId, ...home, team, issue, comment }
}

/** The tracker every pgvector case searches: the same bug in German and
 *  English twice over, unrelated issues, a trashed twin, another team. */
async function seed(tx: RealDbTx) {
  const w = await world(tx)
  const login = {
    en: await w.issue(`Login button does nothing on Safari`, `Clicking "Sign in" on the login page has no effect in Safari 18. Chrome works.`),
    de: await w.issue(`Anmelde-Knopf reagiert in Safari nicht`, `Ein Klick auf „Anmelden“ bewirkt in Safari 18 nichts, in Chrome funktioniert es.`),
  }
  const push = {
    en: await w.issue(`Push notifications arrive twice on Android`, `Every mention sends two identical notifications on Pixel phones.`),
    de: await w.issue(`Push-Benachrichtigungen kommen auf Android doppelt an`, `Jede Erwähnung löst auf Pixel-Geräten zwei gleiche Benachrichtigungen aus.`),
  }
  const csv = await w.issue(`Export the board as a CSV file`, `A download button in the board menu that writes all issues to CSV.`)
  const dark = await w.issue(`Dunkles Farbschema für die Einstellungen`, `Die Einstellungsseite ignoriert das dunkle Design und bleibt hell.`)
  const vague = await w.issue(`Something is off after the update`)
  await w.comment(vague.id, `The PDF invoice shows the wrong VAT rate for Austrian customers.`)
  const trashed = await w.issue(`Login broken in Safari (duplicate)`, `Sign in does nothing in Safari.`)
  const elsewhere = await w.team(`FX`)
  const foreign = await w.issue(`Login button does nothing on Safari`, `Clicking "Sign in" has no effect in Safari.`, elsewhere)
  const foreignTwin = await w.issue(`Safari: the sign in button is dead`, `Nothing happens when you click it.`, elsewhere)

  // The real model, through the worker.
  expect(await runIssueEmbeddingSweep({ executor: tx, settleSeconds: 0, limit: 50, teamId: w.teamId })).toMatchObject({
    embedded: 8,
    unavailable: false,
  })
  expect(await runIssueEmbeddingSweep({ executor: tx, settleSeconds: 0, teamId: elsewhere.teamId })).toMatchObject({ embedded: 2 })
  await tx.update(issues).set({ deletedAt: new Date() }).where(eq(issues.id, trashed.id))
  // Filed after the sweep: an issue the model has not read yet.
  const fresh = await w.issue(`Safari login button broken again`, `Sign in does nothing.`)

  const names = new Map<string, string>([
    [login.en.id, `login.en`], [login.de.id, `login.de`], [push.en.id, `push.en`], [push.de.id, `push.de`],
    [csv.id, `csv`], [dark.id, `dark`], [vague.id, `vague`], [trashed.id, `trashed`], [fresh.id, `fresh`],
    [foreign.id, `foreign`], [foreignTwin.id, `foreignTwin`],
  ])
  const backend = issueSearchBackend(tx)
  const scope: IssueSearchScope = { teamId: w.teamId }
  const search = async (query: string, at: IssueSearchScope = scope, limit = 20) =>
    (await searchIssues(query, at, limit, { backend, budgetMs: 60_000 })).map((row) => names.get(row.id) ?? row.id)
  /** MCP list `search`: the ids its subquery admits. */
  const listed = async (query: string, at: IssueSearchScope = { boardIds: [w.boardId] }) => {
    const ids = await issueSearchIdsSql(query, at, { backend, budgetMs: 60_000, limit: 5000 })
    const result = await tx.execute<{ id: string }>(sql`select i.id from issues i where i.id in (${ids})`)
    return result.rows.map((row) => names.get(row.id) ?? row.id).sort()
  }
  /** What the search was before #NEW-135: the lexical rows alone. */
  const lexical = async (query: string, at: IssueSearchScope = scope, limit = 20) =>
    (await backend.lexical(query, at, limit)).map((row) => names.get(row.id) ?? row.id)
  return { ...w, login, push, csv, dark, vague, trashed, fresh, elsewhere, foreign, foreignTwin, names, backend, scope, search, listed, lexical }
}

describe.skipIf(!ready.ok || hasVector)(`issue search on a Postgres WITHOUT pgvector`, () => {
  it(`is exactly the lexical search; similar: finds nothing`, async () => {
    await inRollback(async (tx) => {
      await tx.execute(sql.raw(bootSql))
      const w = await world(tx)
      const safari = await w.issue(`Login button does nothing on Safari`, `Clicking "Sign in" has no effect.`)
      const other = await w.issue(`Anmelde-Knopf reagiert in Safari nicht`)
      await w.issue(`Export the board as a CSV file`)
      const backend = issueSearchBackend(tx)
      const scope = { teamId: w.teamId }
      resetIssueSearchVectors()
      for (const query of [`login safari`, `safari`, `can't sign in with Safari`, safari.identifier, `is:open`]) {
        expect(await searchIssues(query, scope, 20, { backend })).toEqual(await backend.lexical(query, scope, 20))
      }
      expect((await searchIssues(`safari`, scope, 20, { backend })).map((row) => row.id).sort()).toEqual([safari.id, other.id].sort())
      expect(await searchIssues(`similar:${safari.identifier}`, scope, 20, { backend })).toEqual([])
      expect(await searchIssues(`safari similar:${safari.identifier}`, scope, 20, { backend })).toEqual([])

      const lexicalIds = await tx.execute<{ id: string }>(sql`select i.id from issues i where i.id in (${issueSearchMatchIds(`safari`, scope)})`)
      const listedIds = await tx.execute<{ id: string }>(sql`select i.id from issues i where i.id in (${await issueSearchIdsSql(`safari`, scope, { backend })})`)
      expect(listedIds.rows.map((row) => row.id).sort()).toEqual(lexicalIds.rows.map((row) => row.id).sort())
      const similar = await tx.execute(sql`select i.id from issues i where i.id in (${await issueSearchIdsSql(`similar:${safari.identifier}`, scope, { backend })})`)
      expect(similar.rows).toEqual([])
    })
  })
})

describe.skipIf(!ready.ok || !hasVector || !hasModel)(`issue search on a Postgres WITH pgvector, the real model`, () => {
  afterAll(() => stopEmbedder())

  it(
    `similar:<ID> lists the issue's German/English twin first, inside the scope, filters and words applied`,
    async () => {
      await inRollback(async (tx) => {
        await tx.execute(sql.raw(bootSql))
        const s = await seed(tx)
        resetIssueSearchVectors()

        // The measured band, for the cutoff's record (issue to issue).
        const source = await s.backend.source(s.login.en.identifier, s.scope)
        expect(source?.issueId).toBe(s.login.en.id)
        const band = await nearestIssueIds(source!.vector, s.scope, { maxDistance: 1, k: 20 }, tx)
        console.info(`[issue-search-semantic.real-db] similar:${s.login.en.identifier} → ${band.map((hit) => `${s.names.get(hit.issueId)} ${hit.distance.toFixed(3)}`).join(`, `)}`)

        // The twin first, either way round and in any spelling of the token;
        // never the issue itself, the trashed duplicate or the other team's.
        const similar = await s.search(`similar:${s.login.en.identifier}`)
        expect(similar[0]).toBe(`login.de`)
        expect(similar).not.toContain(`login.en`)
        expect(similar).not.toContain(`trashed`)
        expect(similar).not.toContain(`foreign`)
        expect(similar).not.toContain(`foreignTwin`)
        expect(similar).not.toContain(`fresh`)
        expect((await s.search(`similar:#${s.login.de.identifier.toLowerCase()}`))[0]).toBe(`login.en`)
        expect((await s.search(`similar:${s.push.de.identifier}`))[0]).toBe(`push.en`)
        expect(await s.search(`similar:${s.login.en.identifier}`, s.scope, 1)).toEqual([`login.de`])
        // The same list through the other scopes a caller may hold.
        expect(await s.search(`similar:${s.login.en.identifier}`, { boardIds: [s.boardId] })).toEqual(similar)
        expect(await s.search(`similar:${s.login.en.identifier}`, { teamIds: [s.teamId, s.elsewhere.teamId] })).toEqual(
          expect.arrayContaining([`login.de`])
        )

        // Unknown, another team's, trashed, not embedded yet: no hits, and
        // nothing tells them apart.
        expect(await s.search(`similar:SM-99999`)).toEqual([])
        expect(await s.search(`similar:${s.foreign.identifier}`)).toEqual([])
        expect(await s.search(`similar:${s.trashed.identifier}`)).toEqual([])
        expect(await s.search(`similar:${s.fresh.identifier}`)).toEqual([])
        expect(await s.search(`similar:${s.login.en.identifier}`, { teamIds: [] })).toEqual([])
        // ...while its own team finds the other team's twin.
        expect(await s.search(`similar:${s.foreign.identifier}`, { teamId: s.elsewhere.teamId })).toEqual([`foreignTwin`])

        // The other tokens narrow: a filter, and words lexically.
        await tx.update(issues).set({ status: `done` }).where(eq(issues.id, s.push.en.id))
        const closed = await s.search(`similar:${s.login.en.identifier} is:closed`)
        expect(closed.every((name) => name === `push.en`)).toBe(true)
        const open = await s.search(`similar:${s.login.en.identifier} is:open`)
        expect(open[0]).toBe(`login.de`)
        expect(open).not.toContain(`push.en`)
        expect(await s.search(`similar:${s.login.en.identifier} safari`)).toEqual([`login.de`])
        expect(await s.search(`similar:${s.login.en.identifier} zzzunknownword`)).toEqual([])

        // MCP list `search`: the same issues.
        expect(await s.listed(`similar:${s.login.en.identifier}`)).toEqual([...similar].sort())
        expect(await s.listed(`similar:${s.login.en.identifier} safari`)).toEqual([`login.de`])
        expect(await s.listed(`similar:${s.foreign.identifier}`)).toEqual([])
        expect(await s.listed(`similar:SM-99999`)).toEqual([])
      })
    },
    180_000
  )

  it(
    `a worded query keeps its lexical hits and their order, then appends the paraphrase the words miss`,
    async () => {
      await inRollback(async (tx) => {
        await tx.execute(sql.raw(bootSql))
        const s = await seed(tx)
        resetIssueSearchVectors()

        for (const query of [`login safari`, `Anmeldung in Safari funktioniert nicht`, `doppelte Mitteilungen auf dem Handy`, `Rechnung mit falscher Mehrwertsteuer`, `push`]) {
          const lexical = await s.lexical(query)
          const found = await s.search(query)
          console.info(`[issue-search-semantic.real-db] ${JSON.stringify(query)} → lexical [${lexical.join(`, `)}] then [${found.slice(lexical.length).join(`, `)}]`)
          // Lexical first, untouched; at most the cap behind it; no repeats.
          expect(found.slice(0, lexical.length)).toEqual(lexical)
          expect(found.length - lexical.length).toBeLessThanOrEqual(ISSUE_SEARCH_SEMANTIC_MAX_HITS)
          expect(new Set(found).size).toBe(found.length)
          expect(found).not.toContain(`trashed`)
          expect(found).not.toContain(`foreign`)
          // MCP list `search` admits exactly those issues.
          expect(await s.listed(query)).toEqual([...found].sort())
        }

        // "login safari": the English issue (and the one filed after the
        // sweep) match the words; the German twin matches neither word pair
        // and comes in behind them, by meaning.
        const mixed = await s.search(`login safari`)
        const lexical = await s.lexical(`login safari`)
        expect(lexical).toContain(`login.en`)
        expect(lexical).not.toContain(`login.de`)
        expect(mixed.indexOf(`login.de`)).toBeGreaterThanOrEqual(lexical.length)
        // No word matches at all: the list is the meaning's, the pair on top.
        expect(await s.lexical(`Anmeldung in Safari funktioniert nicht`)).toEqual([])
        expect((await s.search(`Anmeldung in Safari funktioniert nicht`)).slice(0, 2).sort()).toEqual([`login.de`, `login.en`])
        expect((await s.search(`doppelte Mitteilungen auf dem Handy`)).slice(0, 2).sort()).toEqual([`push.de`, `push.en`])
        // Found through its COMMENT, in another language than the query.
        expect((await s.search(`Rechnung mit falscher Mehrwertsteuer`))[0]).toBe(`vague`)

        // The limit spans both halves; a full lexical page takes no neighbour.
        expect(await s.search(`login safari`, s.scope, lexical.length)).toEqual(lexical)
        expect(await s.search(`login safari`, s.scope, lexical.length + 1)).toEqual([...lexical, mixed[lexical.length]])

        // The filter tokens hold for the neighbours too.
        await tx.update(issues).set({ status: `done` }).where(eq(issues.id, s.login.de.id))
        expect(await s.search(`Anmeldung in Safari funktioniert nicht is:closed`)).toEqual([`login.de`])
        expect(await s.search(`Anmeldung in Safari funktioniert nicht is:open`)).not.toContain(`login.de`)
        expect(await s.listed(`Anmeldung in Safari funktioniert nicht is:closed`)).toEqual([`login.de`])

        // A filter-only, a short and an identifier query never reach the
        // model: exactly the lexical answer.
        for (const query of [`is:open`, `sa`, s.csv.identifier]) {
          expect(await s.search(query)).toEqual(await s.lexical(query))
        }

        // The embedder off (or late): today's lexical result, nothing else.
        const off = issueSearchBackend(tx, async () => null)
        resetIssueSearchVectors()
        expect((await searchIssues(`login safari`, s.scope, 20, { backend: off })).map((row) => s.names.get(row.id))).toEqual(lexical)
      })
    },
    180_000
  )
})
