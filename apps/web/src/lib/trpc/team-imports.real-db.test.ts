// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, comments, conversations, importIdMap, importRows, issueLabels, issueStatuses, issues, labels, messages, sessions, teamImports, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1713: the team import against the REAL migrated Postgres: the team
// is invisible until the commit, a chunk is idempotent per seq, a commit
// without the attachments' bytes fails and keeps the import open, the
// commit keeps ids (remapping a collision), maps statuses onto the team's
// builtins, rewrites the identity and its mentions, lands sessions ended and
// private with their conversations and messages, and flips the team live.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'import_rows') AS ok
`)
if (!ready.ok) console.info(`[team-imports.real-db] skipped: ${ready.reason}`)

const { importsRouter } = await import(`@/lib/trpc/imports`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<{ id: string; email: string }> {
  const id = `imp-test-${id8()}`
  const email = `${id}@imports.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${email}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email }
}

const caller = (u: { id: string; email: string }) =>
  importsRouter.createCaller({ session: { user: { id: u.id, email: u.email } }, db, request: new Request(`http://x.test`) } as never)

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const LOCAL_USER = `local-user-${id8()}`
const LOCAL_EMAIL = `danny@local.desktop`
const T0 = `2026-10-01T10:00:00.000Z`
const uuid = () => crypto.randomUUID()

describe.skipIf(!ready.ok)(`team import (real db)`, () => {
  it(`create, chunk, commit`, async () => {
    const owner = await user()
    // A label elsewhere whose id the local file reuses: the commit remaps it.
    const [other] = await db.insert(teams).values({ name: `Other`, slug: `other-${id8()}` }).returning({ id: teams.id })
    made.teams.push(other!.id)
    const takenLabel = uuid()
    await db.insert(labels).values({ id: takenLabel, teamId: other!.id, name: `Taken` })

    const ids = { b1: uuid(), i1: uuid(), c1: uuid(), c2: uuid(), ag: uuid(), s1: uuid(), s2: uuid(), cv1: uuid(), m1: uuid(), a1: uuid(), stBacklog: uuid(), stProgress: uuid(), stBlocked: uuid() }
    const created = await caller(owner).create({
      name: `Local`,
      manifest: { version: 1, desktopId: uuid(), name: `Local`, identity: { userId: LOCAL_USER, email: LOCAL_EMAIL }, deviceId: `dev-${id8()}`, counts: { issues: 1 }, attachmentBytes: 3 },
    })
    made.teams.push(created.teamId)
    expect(created.slug).toMatch(/^local/)
    // Invisible: no member row yet.
    expect(await db.select().from(teamMembers).where(eq(teamMembers.teamId, created.teamId))).toHaveLength(0)

    const rows = [
      { table: `boards`, id: ids.b1, row: { id: ids.b1, team_id: `t-local`, name: `Board`, slug: `board`, prefix: `B`, repository_id: `r-local`, created_at: T0, updated_at: T0 } },
      { table: `issue_statuses`, id: ids.stBacklog, row: { id: ids.stBacklog, team_id: `t-local`, category: `backlog`, name: `Backlog`, color: `#A1A1AA`, sort_order: 1, builtin_key: `backlog` } },
      { table: `issue_statuses`, id: ids.stProgress, row: { id: ids.stProgress, team_id: `t-local`, category: `started`, name: `Doing`, color: `#EAB308`, sort_order: 1, builtin_key: `in_progress` } },
      { table: `issue_statuses`, id: ids.stBlocked, row: { id: ids.stBlocked, team_id: `t-local`, category: `started`, name: `Blocked`, color: `#FF0000`, sort_order: 3, builtin_key: null } },
      { table: `labels`, id: takenLabel, row: { id: takenLabel, team_id: `t-local`, name: `Bug`, color: `#FF0000` } },
      { table: `agents`, id: ids.ag, row: { id: ids.ag, team_id: `t-local`, name: `Assistant`, device_id: `dev-local`, owner_id: LOCAL_USER, role: `assistant`, instructions: `ask @${LOCAL_EMAIL}` } },
      {
        table: `issues`,
        id: ids.i1,
        row: { id: ids.i1, board_id: ids.b1, team_id: `t-local`, number: 7, identifier: `B-7`, title: `First`, description: `by @${LOCAL_EMAIL} and @sam@example.com`, status: `in_progress`, status_id: ids.stProgress, priority: `high`, creator_id: LOCAL_USER, assignee_id: `someone-else`, created_at: T0, updated_at: T0 },
      },
      { table: `issue_labels`, id: `${ids.i1}/${takenLabel}`, row: { issue_id: ids.i1, label_id: takenLabel, team_id: `t-local` } },
      { table: `comments`, id: ids.c1, row: { id: ids.c1, issue_id: ids.i1, team_id: `t-local`, author_id: LOCAL_USER, body: `one`, created_at: T0, updated_at: T0 } },
      { table: `comments`, id: ids.c2, row: { id: ids.c2, issue_id: ids.i1, team_id: `t-local`, author_id: LOCAL_USER, parent_id: ids.c1, body: `two`, created_at: T0, updated_at: T0 } },
      { table: `sessions`, id: ids.s1, row: { id: ids.s1, team_id: `t-local`, root_id: ids.s1, depth: 0, user_id: LOCAL_USER, device_id: `dev-local`, status: `running`, busy: true, shared: true, title: `Root`, agent_id: ids.ag } },
      { table: `sessions`, id: ids.s2, row: { id: ids.s2, team_id: `t-local`, root_id: ids.s1, parent_id: ids.s1, depth: 1, user_id: LOCAL_USER, device_id: `dev-local`, status: `ended`, ended_at: T0, title: `Child` } },
      { table: `conversations`, id: ids.cv1, row: { id: ids.cv1, team_id: `t-local`, kind: `session`, session_id: ids.s1 } },
      { table: `messages`, id: ids.m1, row: { id: ids.m1, conversation_id: ids.cv1, root_id: ids.s1, team_id: `t-local`, author_kind: `user`, author_id: LOCAL_USER, body: `hello @${LOCAL_EMAIL}`, attachment_ids: [ids.a1], mention_user_ids: [LOCAL_USER] } },
      { table: `attachments`, id: ids.a1, row: { id: ids.a1, team_id: `t-local`, issue_id: ids.i1, uploader_id: LOCAL_USER, filename: `note.txt`, content_type: `text/plain`, size_bytes: 3, storage_key: `local/a1`, url: `/api/attachments/${ids.a1}`, created_at: T0, updated_at: T0 } },
    ]
    const first = await caller(owner).chunk({ importId: created.importId, seq: 0, rows })
    expect(first.counts).toMatchObject({ boards: 1, issues: 1, attachments: 1 })
    expect(first.uploads).toHaveLength(1)
    expect(first.uploads[0]!.uploadPath).toMatch(/^\/api\/attachment-uploads\//)
    // The same seq again: nothing changes.
    const again = await caller(owner).chunk({ importId: created.importId, seq: 0, rows })
    expect(again.counts).toEqual(first.counts)
    expect(await db.select().from(importRows).where(eq(importRows.importId, created.importId))).toHaveLength(rows.length)
    await expect(caller(owner).chunk({ importId: created.importId, seq: 1, rows: [{ table: `users`, id: `x`, row: { id: `x` } }] })).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // No bytes landed yet: the commit refuses and the import stays open.
    await expect(caller(owner).commit({ importId: created.importId })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    const [open] = await db.select({ status: teamImports.status, error: teamImports.error }).from(teamImports).where(eq(teamImports.id, created.importId))
    expect(open).toMatchObject({ status: `importing` })
    expect(open?.error).toContain(`never landed`)

    // What the signed upload lands.
    await db.insert(attachments).values({ id: first.uploads[0]!.serverId, teamId: created.teamId, uploaderId: owner.id, filename: `note.txt`, contentType: `text/plain`, sizeBytes: 3, storageKey: `pending/${ids.a1}`, url: `/api/attachments/${ids.a1}` })

    const result = await caller(owner).commit({ importId: created.importId })
    expect(result.team.id).toBe(created.teamId)
    expect(result.remapped).toBeGreaterThanOrEqual(1)
    const labelMap = result.idMap.find((e) => e.tableName === `labels` && e.localId === takenLabel)
    expect(labelMap?.serverId).toBeDefined()
    expect(labelMap?.serverId).not.toBe(takenLabel)

    // Live: the caller owns it, the import is committed and its rows gone.
    expect(await db.select().from(teamMembers).where(eq(teamMembers.teamId, created.teamId))).toMatchObject([{ userId: owner.id, role: `owner` }])
    const [imp] = await db.select({ status: teamImports.status, error: teamImports.error }).from(teamImports).where(eq(teamImports.id, created.importId))
    expect(imp).toMatchObject({ status: `committed`, error: null })
    expect(await db.select().from(importRows).where(eq(importRows.importId, created.importId))).toHaveLength(0)
    const [team] = await db.select({ importedFrom: teams.importedFrom }).from(teams).where(eq(teams.id, created.teamId))
    expect(team?.importedFrom).toMatchObject({ importId: created.importId, name: `Local` })

    // Statuses: the builtins stay single, Doing maps onto In Progress by key, Blocked is new.
    const statuses = await db.select({ id: issueStatuses.id, name: issueStatuses.name, builtinKey: issueStatuses.builtinKey, category: issueStatuses.category }).from(issueStatuses).where(eq(issueStatuses.teamId, created.teamId))
    expect(statuses.filter((s) => s.builtinKey === `in_progress`)).toHaveLength(1)
    expect(statuses.find((s) => s.name === `Blocked`)).toMatchObject({ category: `started`, builtinKey: null })
    expect(statuses.find((s) => s.name === `Doing`)).toBeUndefined()
    const inProgress = statuses.find((s) => s.builtinKey === `in_progress`)!

    // The issue: id and number kept, identifier from the prefix, identity and mentions rewritten.
    const [issue] = await db.select().from(issues).where(eq(issues.id, ids.i1))
    expect(issue).toMatchObject({ teamId: created.teamId, boardId: ids.b1, number: 7, identifier: `B-7`, statusId: inProgress.id, creatorId: owner.id, assigneeId: null, description: `by @${owner.email} and @sam@example.com` })
    expect(issue?.createdAt.toISOString()).toBe(T0)
    expect(await db.select().from(issueLabels).where(and(eq(issueLabels.issueId, ids.i1), eq(issueLabels.labelId, labelMap!.serverId)))).toHaveLength(1)
    const [reply] = await db.select({ parentId: comments.parentId, boardId: comments.boardId, authorId: comments.authorId }).from(comments).where(eq(comments.id, ids.c2))
    expect(reply).toEqual({ parentId: ids.c1, boardId: ids.b1, authorId: owner.id })

    // Sessions: ended and private, the tree kept, the conversation the server made holds the message.
    const [root] = await db.select().from(sessions).where(eq(sessions.id, ids.s1))
    expect(root).toMatchObject({ status: `ended`, shared: false, busy: false, userId: owner.id, agentId: ids.ag, teamId: created.teamId })
    expect(root?.endedAt).not.toBeNull()
    const [child] = await db.select({ rootId: sessions.rootId, parentId: sessions.parentId, endedAt: sessions.endedAt }).from(sessions).where(eq(sessions.id, ids.s2))
    expect(child).toMatchObject({ rootId: ids.s1, parentId: ids.s1 })
    expect(child?.endedAt?.toISOString()).toBe(T0)
    const [conversation] = await db.select({ id: conversations.id }).from(conversations).where(eq(conversations.sessionId, ids.s1))
    const [message] = await db.select().from(messages).where(eq(messages.id, ids.m1))
    expect(message).toMatchObject({ conversationId: conversation!.id, rootId: ids.s1, authorId: owner.id, body: `hello @${owner.email}`, attachmentIds: [first.uploads[0]!.serverId], mentionUserIds: [owner.id] })
    expect(await db.select().from(importIdMap).where(and(eq(importIdMap.importId, created.importId), eq(importIdMap.tableName, `conversations`)))).toHaveLength(1)

    // The attachment the upload landed now belongs to the issue.
    const [att] = await db.select({ issueId: attachments.issueId, boardId: attachments.boardId, sizeBytes: attachments.sizeBytes }).from(attachments).where(eq(attachments.id, first.uploads[0]!.serverId))
    expect(att).toEqual({ issueId: ids.i1, boardId: ids.b1, sizeBytes: 3 })

    // Done is done.
    await expect(caller(owner).chunk({ importId: created.importId, seq: 2, rows: rows.slice(0, 1) })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    // Someone else never sees it.
    const stranger = await user()
    await expect(caller(stranger).commit({ importId: created.importId })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})
