// @vitest-environment node
// #EXP-1990 (agent-settings.json `groupChat`, phase 6 of #EXP-1984): team
// agents as group chats, against the REAL migrated Postgres. Locks:
//   * off: one conversation per (agent, member), `group_chat_id` stays null;
//   * on: ONE root per team agent, named by `agents.group_chat_id`, started
//     by the agent's owner, whoever writes first; two members write into it
//     and read it; a KEYED group conversation: both members' installs hold
//     the tree key, and a teammate who joins later gets the wrap from a
//     holder and opens the history;
//   * an agent on its owner's unshared machine talks to its owner only; the
//     root turns shared once the machine is;
//   * a trigger firing and an assignment nudge land in the group chat;
//   * a question's push goes to the asker, else the agent's owner (`askCases`);
//   * THE MOVE: a private conversation ends, is detached and turns private
//     (a teammate's wrap of it dropped), readable only by its member; an
//     Assistant's conversation and an app's editor root stay (`moveCases`).
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm"
import { GROUP_CHAT, groupChatMoveOf } from "@exp/domain-contract/agent-settings"
import {
  b64urlEncode,
  generateTreeKey,
  openBody,
  pushPublicKey,
  sealBody,
  unwrapTreeKey,
  wrapTreeKey,
} from "@exp/domain-contract/sealed-content"
import { db } from "@/db/connection"
import { agents, apps, devices, installs, messages, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => null,
    relayPostNudge: async () => {},
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'agents' AND column_name = 'group_chat_id') AS ok
`)
if (!ready.ok) console.info(`[group-chat.real-db] skipped: ${ready.reason}`)

const { treeReaders } = await import(`@/lib/sessions/tree-keys`)
const { sessionVisibleTo, loadSessionForUser } = await import(`@/lib/sessions/access`)
const { sendSession } = await import(`@/lib/sessions/send`)
const { sendTarget } = await import(`@/lib/sessions/send-target`)
const { inSession } = await import(`@/lib/sessions/messages`)
const { postAgentInputInTx } = await import(`@/lib/agents/triggers`)
const { endPrivateConversationsInTx, groupAskRecipients, runGroupChatMove } = await import(`@/lib/agents/group-chat`)
const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `group-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@group.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(members: Array<[string, `owner` | `member`]>): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Group`, slug: `group-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(members.map(([userId, role]) => ({ teamId: row!.id, userId, role })))
  return row!.id
}

async function device(userId: string, sharedTeamIds: string[] = []) {
  const deviceId = `dev-${id8()}`
  const [row] = await db
    .insert(devices)
    .values({ userId, deviceId, label: `box`, kind: `server`, lastSeenAt: new Date(), sharedTeamIds })
    .returning({ id: devices.id })
  return { deviceId, rowId: row!.id }
}

async function install(userId: string, deviceRowId: string | null = null) {
  const secret = crypto.getRandomValues(new Uint8Array(32))
  const x25519Public = b64urlEncode(pushPublicKey(secret))
  const [row] = await db
    .insert(installs)
    .values({ userId, kind: deviceRowId ? `desktop` : `phone`, deviceId: deviceRowId, endpointId: `ep-${id8()}${id8()}`, x25519Public } as never)
    .returning({ id: installs.id })
  return { id: row!.id, secret, x25519Public }
}

type Runner = Pick<typeof db, `insert`>
const agent = async (runner: Runner, teamId: string, ownerId: string, deviceId: string, role: `agent` | `assistant` | `coordinator` = `agent`) =>
  (await runner.insert(agents).values({ teamId, ownerId, deviceId, name: `Triage`, role } as never).returning())[0]!

const agentRow = async (id: string) => (await db.select().from(agents).where(eq(agents.id, id)).limit(1))[0]!
const sessionRow = async (id: string) => (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!
const linesOf = (sessionId: string) =>
  db.select().from(messages).where(inSession(sessionId)).orderBy(asc(messages.createdAt), asc(messages.id))
const keys = (userId: string) =>
  sessionKeysRouter.createCaller({ session: { user: { id: userId }, session: { id: `s` } }, db, request: new Request(`http://x.test`) } as never)

beforeEach(() => {
  process.env.AGENT_GROUP_CHATS = `on`
})
afterEach(() => {
  delete process.env.AGENT_GROUP_CHATS
})
afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`team agents as group chats (#EXP-1990)`, () => {
  it(`off: one conversation per (agent, member), no group chat`, async () => {
    delete process.env.AGENT_GROUP_CHATS
    const owner = await user()
    const mate = await user()
    const teamId = await team([[owner, `owner`], [mate, `member`]])
    const box = await device(owner, [teamId])
    const a = await agent(db, teamId, owner, box.deviceId)
    const mine = await sendSession({ userId: owner, input: { agentId: a.id, text: `hi` } })
    const theirs = await sendSession({ userId: mate, input: { agentId: a.id, text: `hello` } })
    expect(theirs.sessionId).not.toBe(mine.sessionId)
    expect((await sessionRow(theirs.sessionId)).userId).toBe(mate)
    expect((await agentRow(a.id)).groupChatId).toBeNull()
    expect(await runGroupChatMove()).toEqual([])
    expect((await sessionRow(mine.sessionId)).agentId).toBe(a.id)
  }, 60_000)

  it(`on: ONE keyed group chat; two members' installs write and read it; a later joiner gets the wrap and the history`, async () => {
    const owner = await user()
    const mate = await user()
    const late = await user()
    const teamId = await team([[owner, `owner`], [mate, `member`]])
    const box = await device(owner, [teamId])
    const ownerPhone = await install(owner)
    const matePhone = await install(mate)
    const a = await agent(db, teamId, owner, box.deviceId)

    // The teammate writes first: no chat yet, so their send carries the key,
    // wrapped for every reader (both members' installs).
    expect(await sendTarget(mate, { agentId: a.id })).toMatchObject({ rootId: null, publicKey: null })
    const tree = generateTreeKey()
    const first = await sendSession({
      userId: mate,
      input: {
        agentId: a.id,
        text: sealBody(tree.publicKey, 1, `can you triage EXP-1?`),
        seal: {
          installId: matePhone.id,
          publicKey: tree.publicKey,
          epoch: 1,
          wraps: [matePhone, ownerPhone].map((reader) => ({ installId: reader.id, wrapped: wrapTreeKey(reader.x25519Public, tree.secret) })),
        },
      },
    })
    const chat = await sessionRow(first.sessionId)
    // The agent's ONE chat: started by its owner, shared, on its machine.
    expect(chat).toMatchObject({ agentId: a.id, parentId: null, userId: owner, shared: true, deviceId: box.deviceId, teamId, sealEpoch: 1, sealPublicKey: tree.publicKey })
    expect((await agentRow(a.id)).groupChatId).toBe(chat.id)

    // The owner finds the same chat and writes into it under the same key.
    const target = await sendTarget(owner, { agentId: a.id })
    expect(target).toMatchObject({ rootId: chat.id, epoch: 1, publicKey: tree.publicKey })
    const second = await sendSession({ userId: owner, input: { agentId: a.id, text: sealBody(tree.publicKey, 1, `and EXP-2 while you are at it`) } })
    expect(second.sessionId).toBe(chat.id)
    const roots = await db.select({ id: sessions.id }).from(sessions).where(and(eq(sessions.agentId, a.id), isNull(sessions.parentId)))
    expect(roots).toEqual([{ id: chat.id }])

    // Each line keeps its author; each member's install opens both.
    const lines = (await linesOf(chat.id)).filter((line) => line.authorKind === `user`)
    expect(lines.map((line) => line.authorId)).toEqual([mate, owner])
    for (const [userId, reader] of [[owner, ownerPhone], [mate, matePhone]] as const) {
      const [held] = await keys(userId).mine({ installId: reader.id, rootIds: [chat.id] })
      const secret = unwrapTreeKey(reader.secret, held!.wrapped)
      expect(lines.map((line) => openBody(line.body, new Map([[1, secret]])))).toMatchObject([
        { status: `opened`, text: `can you triage EXP-1?` },
        { status: `opened`, text: `and EXP-2 while you are at it` },
      ])
      await expect(loadSessionForUser(userId, chat.id)).resolves.toMatchObject({ id: chat.id })
    }

    // A teammate joins later: not a reader before, a reader after; a holder's
    // install wraps the key for theirs and they open the whole history.
    const latePhone = await install(late)
    await expect(loadSessionForUser(late, chat.id)).rejects.toThrow()
    expect(await keys(mate).pending({ installId: matePhone.id, rootIds: [chat.id] })).toEqual([])
    await db.insert(teamMembers).values({ teamId, userId: late, role: `member` })
    await expect(loadSessionForUser(late, chat.id)).resolves.toMatchObject({ id: chat.id })
    const pending = await keys(mate).pending({ installId: matePhone.id, rootIds: [chat.id] })
    expect(pending).toMatchObject([{ rootId: chat.id, epoch: 1, targetInstallId: latePhone.id, targetX25519Public: latePhone.x25519Public }])
    const [mateHeld] = await keys(mate).mine({ installId: matePhone.id, rootIds: [chat.id] })
    await keys(mate).wrap({
      installId: matePhone.id,
      wraps: [{ rootId: chat.id, epoch: 1, targetInstallId: latePhone.id, wrapped: wrapTreeKey(latePhone.x25519Public, unwrapTreeKey(matePhone.secret, mateHeld!.wrapped)) }],
    })
    const [lateHeld] = await keys(late).mine({ installId: latePhone.id, rootIds: [chat.id] })
    const lateSecret = unwrapTreeKey(latePhone.secret, lateHeld!.wrapped)
    expect(lines.map((line) => openBody(line.body, new Map([[1, lateSecret]])).status)).toEqual([`opened`, `opened`])
    // And writes into the same chat.
    const third = await sendSession({ userId: late, input: { agentId: a.id, text: sealBody(tree.publicKey, 1, `joining in`) } })
    expect(third.sessionId).toBe(chat.id)
  }, 60_000)

  it(`an agent on its owner's unshared machine talks to its owner only; shared later, the same chat is everyone's`, async () => {
    const owner = await user()
    const mate = await user()
    const teamId = await team([[owner, `owner`], [mate, `member`]])
    const box = await device(owner)
    const a = await agent(db, teamId, owner, box.deviceId)
    await expect(sendSession({ userId: mate, input: { agentId: a.id, text: `hello?` } })).rejects.toThrow(/not yours and not shared/)
    expect((await agentRow(a.id)).groupChatId).toBeNull()
    const mine = await sendSession({ userId: owner, input: { agentId: a.id, text: `just us` } })
    expect(await sessionRow(mine.sessionId)).toMatchObject({ userId: owner, shared: false })
    expect((await agentRow(a.id)).groupChatId).toBe(mine.sessionId)
    // Still private: the teammate neither finds nor writes it.
    expect(await sendTarget(mate, { agentId: a.id })).toMatchObject({ rootId: null })
    await expect(sendSession({ userId: mate, input: { agentId: a.id, text: `me too` } })).rejects.toThrow(/not yours and not shared/)
    expect((await linesOf(mine.sessionId)).map((line) => line.body)).toEqual([`just us`])
    // The owner shares the machine: the next use re-stamps the same root.
    await db.update(devices).set({ sharedTeamIds: [teamId] }).where(eq(devices.id, box.rowId))
    const theirs = await sendSession({ userId: mate, input: { agentId: a.id, text: `me too` } })
    expect(theirs.sessionId).toBe(mine.sessionId)
    expect(await sessionRow(mine.sessionId)).toMatchObject({ shared: true })
    expect((await linesOf(mine.sessionId)).every((line) => line.shared)).toBe(true)
    // Unshared again: private at the owner's next use, the teammate out.
    await db.update(devices).set({ sharedTeamIds: [] }).where(eq(devices.id, box.rowId))
    await expect(sendSession({ userId: mate, input: { agentId: a.id, text: `still here?` } })).rejects.toThrow(/not yours and not shared/)
    expect(await sessionRow(mine.sessionId)).toMatchObject({ shared: true })
    await sendSession({ userId: owner, input: { agentId: a.id, text: `back to us` } })
    expect(await sessionRow(mine.sessionId)).toMatchObject({ shared: false })
  }, 60_000)

  it(`a trigger firing and a nudge land in the group chat; a question's push goes to the asker, else the owner`, async () => {
    const owner = await user()
    const mate = await user()
    const teamId = await team([[owner, `owner`], [mate, `member`]])
    const box = await device(owner, [teamId])
    const a = await agent(db, teamId, owner, box.deviceId)
    await inRollback(async (tx) => {
      // A firing opens the chat (its owner's line), a nudge is the assigner's.
      await postAgentInputInTx(tx, a, owner, `Schedule fired.`, { triggerId: `t1`, eventId: null, name: null })
      const [fresh] = await tx.select().from(agents).where(eq(agents.id, a.id))
      const chatId = fresh!.groupChatId!
      expect(chatId).toBeTruthy()
      const [chat] = await tx.select().from(sessions).where(eq(sessions.id, chatId))
      expect(chat).toMatchObject({ userId: owner, shared: true, agentId: a.id })
      await postAgentInputInTx(tx, fresh!, mate, `EXP-3 is yours.`, { triggerId: null, eventId: null, name: `issue.assigned` })
      const lines = await tx.select().from(messages).where(eq(messages.sessionId, chatId)).orderBy(asc(messages.createdAt), asc(messages.id))
      expect(lines.map((line) => [line.authorId, (line.card as { event?: string } | null)?.event])).toEqual([
        [owner, `trigger_fired`],
        [mate, `trigger_fired`],
      ])
      // A turn only a trigger started: nobody asked, the owner hears it.
      const consume = (at: Date, ids: string[]) => tx.update(messages).set({ consumedAt: at }).where(inArray(messages.id, ids))
      await consume(new Date(`2026-11-23T10:00:00Z`), lines.map((line) => line.id))
      expect(await groupAskRecipients(tx, chat!)).toEqual([owner])
      // The teammate writes; the turn that takes their message asks THEM.
      const [said] = await tx
        .insert(messages)
        .values({ conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${chatId})`, sessionId: chatId, rootId: chatId, teamId, authorKind: `user`, authorId: mate, body: `which one first?`, shared: true } as never)
        .returning({ id: messages.id })
      await consume(new Date(`2026-11-23T10:05:00Z`), [said!.id])
      expect(await groupAskRecipients(tx, chat!)).toEqual([mate])
      // An asker who left the team: the owner.
      await tx.delete(teamMembers).where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, mate)))
      expect(await groupAskRecipients(tx, chat!)).toEqual([owner])
      // Not a group chat (a child, a plain chat): the usual rule.
      expect(await groupAskRecipients(tx, { ...chat!, agentId: null })).toBeNull()
      expect(await groupAskRecipients(tx, { ...chat!, parentId: chatId })).toBeNull()
    })
  }, 60_000)

  it(`the move: a private conversation ends, detached and private with its teammates' wraps gone; the group chat, an Assistant's and an app's root stay`, async () => {
    const owner = await user()
    const mate = await user()
    const teamId = await team([[owner, `owner`], [mate, `member`]])
    const box = await device(owner, [teamId])
    const ownerPhone = await install(owner)
    const ownerBox = await install(owner, box.rowId)
    const matePhone = await install(mate)
    await inRollback(async (tx: RealDbTx) => {
      const a = await agent(tx, teamId, owner, box.deviceId)
      const coordinator = await agent(tx, teamId, owner, box.deviceId, `coordinator`)
      const assistant = await agent(tx, teamId, owner, box.deviceId, `assistant`)
      const appAgent = await agent(tx, teamId, owner, box.deviceId)
      const [app] = await tx
        .insert(apps)
        .values({ teamId, slug: `app-${id8()}`, name: `App`, agentId: appAgent.id, createdBy: owner } as never)
        .returning({ id: apps.id })
      const tree = generateTreeKey()
      const root = async (input: { agentId: string; userId: string; shared?: boolean; appId?: string; keyed?: boolean }) => {
        const id = crypto.randomUUID()
        await tx.insert(sessions).values({
          id,
          rootId: id,
          teamId,
          agentId: input.agentId,
          appId: input.appId ?? null,
          userId: input.userId,
          deviceId: box.deviceId,
          shared: input.shared ?? true,
          status: `running`,
          busy: true,
          needsInput: true,
          ...(input.keyed ? { sealPublicKey: tree.publicKey, sealEpoch: 1 } : {}),
        } as never)
        return id
      }
      // Today's rows: one conversation per member, `shared` (a shared machine).
      const mine = await root({ agentId: a.id, userId: owner, keyed: true })
      const theirs = await root({ agentId: a.id, userId: mate })
      const withCoordinator = await root({ agentId: coordinator.id, userId: mate })
      const withAssistant = await root({ agentId: assistant.id, userId: owner, shared: false })
      const editor = await root({ agentId: appAgent.id, userId: mate, appId: app!.id })
      const child = crypto.randomUUID()
      await tx.insert(sessions).values({ id: child, rootId: mine, parentId: mine, depth: 1, teamId, agentId: a.id, userId: owner, deviceId: box.deviceId, shared: true, status: `running` } as never)
      for (const reader of [ownerPhone, ownerBox, matePhone]) {
        await tx.insert(sessionKeys).values({ rootId: mine, epoch: 1, installId: reader.id, wrapped: wrapTreeKey(reader.x25519Public, tree.secret) })
      }
      const sealed = sealBody(tree.publicKey, 1, `between the two of us`)
      await tx.insert(messages).values({ conversationId: sql`(SELECT id FROM conversations WHERE session_id = ${mine})`, sessionId: mine, rootId: mine, teamId, authorKind: `user`, authorId: owner, body: sealed, shared: true } as never)
      const [before] = await tx.select().from(sessions).where(eq(sessions.id, mine))
      expect(sessionVisibleTo(before!, mate)).toBe(true)
      expect([...(await treeReaders(tx, before!)).keys()]).toContain(matePhone.id)

      // The pick, as the TS reference reads it.
      const pick = groupChatMoveOf(
        [a, coordinator, assistant, appAgent].map((row) => ({ id: row.id, role: row.role, groupChatId: null })),
        [
          { id: mine, agentId: a.id, appId: null },
          { id: theirs, agentId: a.id, appId: null },
          { id: withCoordinator, agentId: coordinator.id, appId: null },
          { id: withAssistant, agentId: assistant.id, appId: null },
          { id: editor, agentId: appAgent.id, appId: app!.id },
        ]
      )
      expect(pick).toEqual([mine, theirs, withCoordinator].sort())
      const moved = (await endPrivateConversationsInTx(tx)).filter((id) => pick.includes(id) || ([withAssistant, editor] as string[]).includes(id))
      expect(moved.sort()).toEqual(pick)

      // Ended, detached, private: readable only by the member who had it.
      const after = async (id: string) => (await tx.select().from(sessions).where(eq(sessions.id, id)))[0]!
      for (const id of [mine, theirs, withCoordinator, child]) {
        expect(await after(id)).toMatchObject({ status: `ended`, agentId: null, shared: false, busy: false, needsInput: false })
      }
      const ended = await after(mine)
      expect(ended).toMatchObject({ teamId, userId: owner, sealPublicKey: tree.publicKey, sealEpoch: 1, sealRotateEpoch: 1 })
      expect(sessionVisibleTo(ended, owner)).toBe(true)
      expect(sessionVisibleTo(ended, mate)).toBe(false)
      expect(sessionVisibleTo(await after(theirs), owner)).toBe(false)
      expect(sessionVisibleTo(await after(theirs), mate)).toBe(true)
      const wraps = await tx.select().from(sessionKeys).where(eq(sessionKeys.rootId, mine))
      expect(wraps.map((wrap) => wrap.installId).sort()).toEqual([ownerPhone.id, ownerBox.id].sort())
      const [line] = await tx.select().from(messages).where(eq(messages.sessionId, mine))
      expect(line).toMatchObject({ shared: false, body: sealed })
      const secret = unwrapTreeKey(ownerPhone.secret, wraps.find((wrap) => wrap.installId === ownerPhone.id)!.wrapped)
      expect(openBody(line!.body, new Map([[1, secret]]))).toMatchObject({ status: `opened`, text: `between the two of us` })
      // The Assistant's conversation and the app's editor root are untouched.
      expect(await after(withAssistant)).toMatchObject({ status: `running`, agentId: assistant.id })
      expect(await after(editor)).toMatchObject({ status: `running`, agentId: appAgent.id, shared: true })

      // A second pass finds nobody; a group chat started since stays.
      const groupId = await root({ agentId: a.id, userId: owner })
      await tx.update(agents).set({ groupChatId: groupId }).where(eq(agents.id, a.id))
      const again = await endPrivateConversationsInTx(tx)
      expect(again.filter((id) => ([mine, theirs, withCoordinator, groupId, withAssistant, editor] as string[]).includes(id))).toEqual([])
      expect(await after(groupId)).toMatchObject({ status: `running`, agentId: a.id, shared: true })
    })
  }, 60_000)

  it(`replays moveCases on real rows`, async () => {
    const owner = await user()
    // One conversation per (agent, member): each root its own member.
    const others = [await user(), await user()]
    const teamId = await team([[owner, `owner`], ...others.map((id): [string, `member`] => [id, `member`])])
    const box = await device(owner, [teamId])
    for (const c of GROUP_CHAT.moveCases) {
      await inRollback(async (tx: RealDbTx) => {
        const uuidOf = new Map<string, string>()
        const uuid = (fid: string): string => uuidOf.get(fid) ?? (uuidOf.set(fid, crypto.randomUUID()), uuidOf.get(fid)!)
        for (const row of c.agents) {
          await tx.insert(agents).values({ id: uuid(row.id), teamId, ownerId: owner, deviceId: box.deviceId, name: row.id, role: row.role } as never)
        }
        const appOf = new Map<string, string>()
        for (const [index, row] of c.roots.entries()) {
          if (row.appId && !appOf.has(row.appId)) {
            const [app] = await tx
              .insert(apps)
              .values({ teamId, slug: `app-${id8()}`, name: `App`, agentId: uuid(row.agentId), createdBy: owner } as never)
              .returning({ id: apps.id })
            appOf.set(row.appId, app!.id)
          }
          const id = uuid(row.id)
          await tx.insert(sessions).values({ id, rootId: id, teamId, agentId: uuid(row.agentId), appId: row.appId ? appOf.get(row.appId)! : null, userId: [owner, ...others][index]!, deviceId: box.deviceId, shared: true, status: `idle` } as never)
        }
        for (const row of c.agents) {
          if (row.groupChatId) await tx.update(agents).set({ groupChatId: uuid(row.groupChatId) }).where(eq(agents.id, uuid(row.id)))
        }
        const mine = new Set(c.roots.map((row) => uuid(row.id)))
        const ended = (await endPrivateConversationsInTx(tx)).filter((id) => mine.has(id))
        expect(ended.sort(), c.name).toEqual(c.expected.map(uuid).sort())
      })
    }
  }, 120_000)
})
