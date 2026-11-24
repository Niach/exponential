// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { probeRealDb } from "@/test/real-db"

// #NEW-23 against the real migrated Postgres: an issue's reply lists the
// pull requests that close it, newest first, a testing one with its batch.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'pull_requests' AND column_name = 'merge_train_id') AS ok
`)
if (!ready.ok) console.info(`[issue-pull-requests.real-db] skipped: ${ready.reason}`)

const { loadIssuePullRequests, loadIssueSessions } = await import(`./issue-pull-requests`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  for (const id of made.teams) await db.execute(sql`DELETE FROM teams WHERE id = ${id}`)
  for (const id of made.users) await db.execute(sql`DELETE FROM users WHERE id = ${id}`)
})

describe.skipIf(!ready.ok)(`#NEW-23: an issue's pull requests`, () => {
  it(`lists the closing PRs with the pill's words and skips a reference`, async () => {
    const team = await db.execute<{ id: string }>(sql`
      INSERT INTO teams (name, slug) VALUES ('Pulls', ${`pulls-${id8()}`}) RETURNING id`)
    const teamId = team.rows[0]!.id
    made.teams.push(teamId)
    const board = await db.execute<{ id: string }>(sql`
      INSERT INTO boards (team_id, name, slug, prefix) VALUES (${teamId}, 'Board', ${`b-${id8()}`}, ${`P${id8().slice(0, 3).toUpperCase()}`}) RETURNING id`)
    const boardId = board.rows[0]!.id
    const issue = await db.execute<{ id: string }>(sql`
      INSERT INTO issues (board_id, team_id, number, identifier, title) VALUES (${boardId}, ${teamId}, 1, ${`PUL-${id8()}`}, 'An issue') RETURNING id`)
    const issueId = issue.rows[0]!.id
    const repo = await db.execute<{ id: string }>(sql`
      INSERT INTO repositories (team_id, full_name, default_branch) VALUES (${teamId}, ${`acme/${id8()}`}, 'main') RETURNING id`)
    const repositoryId = repo.rows[0]!.id
    const train = await db.execute<{ id: string }>(sql`
      INSERT INTO merge_trains (repository_id, team_id, base, units) VALUES (${repositoryId}, ${teamId}, 'main', '[]'::jsonb) RETURNING id`)
    const trainId = train.rows[0]!.id
    const pr = async (number: number, columns: { state?: string; mergeState?: string; mergeWait?: string | null; position?: number | null; trainId?: string | null; age: number }) => {
      const row = await db.execute<{ id: string }>(sql`
        INSERT INTO pull_requests (team_id, repository_id, number, url, head, base, state, merge_state, merge_wait, merge_position, merge_train_id, created_at)
        VALUES (${teamId}, ${repositoryId}, ${number}, ${`https://github.com/acme/app/pull/${number}`}, ${`exp/${number}`}, 'main',
          ${columns.state ?? `open`}, ${columns.mergeState ?? `checks`}, ${columns.mergeWait ?? null}, ${columns.position ?? null}, ${columns.trainId ?? null},
          now() - ${`${columns.age} minutes`}::interval)
        RETURNING id`)
      return row.rows[0]!.id
    }
    const link = (pullRequestId: string, closes = true) => db.execute(sql`
      INSERT INTO pull_request_issues (pull_request_id, issue_id, team_id, board_id, closes) VALUES (${pullRequestId}, ${issueId}, ${teamId}, ${boardId}, ${closes})`)
    await link(await pr(1, { state: `merged`, mergeState: `merged`, age: 30 }))
    await link(await pr(2, { mergeState: `testing`, trainId, age: 20 }))
    // Two more PRs ride the same run; they close other issues.
    await pr(3, { mergeState: `testing`, trainId, age: 19 })
    await pr(4, { mergeState: `testing`, trainId, age: 18 })
    await link(await pr(5, { mergeState: `queued`, position: 2, age: 10 }))
    await link(await pr(6, { mergeState: `checks`, mergeWait: `checks`, age: 5 }), false)

    const rows = await loadIssuePullRequests(issueId)
    expect(rows.map((row) => [row.number, row.state, row.status])).toEqual([
      [5, `open`, `Queued · 2nd`],
      [2, `open`, `Testing with 2`],
      [1, `merged`, `Merged`],
    ])
    expect(rows[0]).toMatchObject({ url: `https://github.com/acme/app/pull/5`, checks: `none` })
  })

  it(`lists the live sessions the caller may see`, async () => {
    const user = async () => {
      const id = `pulls-${id8()}`
      await db.execute(sql`
        INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
        VALUES (${id}, 'Tester', ${`${id}@pulls.test`}, true, false, now(), now())`)
      made.users.push(id)
      return id
    }
    const [me, mate] = [await user(), await user()]
    const team = await db.execute<{ id: string }>(sql`
      INSERT INTO teams (name, slug) VALUES ('Pulls', ${`pulls-${id8()}`}) RETURNING id`)
    const teamId = team.rows[0]!.id
    made.teams.push(teamId)
    const board = await db.execute<{ id: string }>(sql`
      INSERT INTO boards (team_id, name, slug, prefix) VALUES (${teamId}, 'Board', ${`b-${id8()}`}, ${`P${id8().slice(0, 3).toUpperCase()}`}) RETURNING id`)
    const boardId = board.rows[0]!.id
    const issue = await db.execute<{ id: string }>(sql`
      INSERT INTO issues (board_id, team_id, number, identifier, title) VALUES (${boardId}, ${teamId}, 1, ${`PUL-${id8()}`}, 'An issue') RETURNING id`)
    const issueId = issue.rows[0]!.id
    const session = async (owner: string, columns: { status?: string; shared?: boolean; busy?: boolean; branch?: string }) => {
      const id = crypto.randomUUID()
      await db.execute(sql`
        INSERT INTO sessions (id, team_id, root_id, user_id, device_id, status, shared, busy, branch)
        VALUES (${id}, ${teamId}, ${id}, ${owner}, 'dev', ${columns.status ?? `running`}, ${columns.shared ?? false}, ${columns.busy ?? false}, ${columns.branch ?? null})`)
      await db.execute(sql`
        INSERT INTO session_issues (session_id, issue_id, team_id, board_id) VALUES (${id}, ${issueId}, ${teamId}, ${boardId})`)
      return id
    }
    const mine = await session(me, { busy: true, branch: `exp/session-mine` })
    await session(me, { status: `ended` })
    await session(mate, {})
    const shared = await session(mate, { shared: true, status: `idle` })

    const rows = await loadIssueSessions(issueId, me)
    expect(rows.map((row) => row.sessionId).sort()).toEqual([mine, shared].sort())
    expect(rows.find((row) => row.sessionId === mine)).toMatchObject({ rootId: mine, status: `running`, busy: true, needsInput: false, branch: `exp/session-mine` })
  })
})
