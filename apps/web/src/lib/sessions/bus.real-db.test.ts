// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, devices, messages, repositories, sessionReads, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession, messageWithSession } from "@/lib/sessions/messages"

// NEW-1: THE server bus against the REAL migrated Postgres. Every
// transition of a session reaches its PARENT's conversation and every live
// session whose `wake_on` names it: a turn's reply, an open card (the `ask`
// mirror), a wall, a PR, its claim and its end (the `report` card). Waking
// rows are authored BY the child (`author_session_id`) with `consumed_at`
// NULL; informational ones are pre-consumed; with nobody listening an event
// line lands on the child itself. Also `sessions.send`'s `wakeOn` and
// `answer`, and End's deepest-first reports. Each case builds a fresh team;
// the suite deletes every team (and user) it made (cascade).

const notifyBlocked = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => {}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: notifyBlocked,
}))
// The relay frames only kill latency: never reached from here.
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'wake_on')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[bus.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { endSession } = await import(`@/lib/sessions/lifecycle`)
const { BUS_COPY } = await import(`@/lib/sessions/bus`)
const { buildQuestionCard, postAskCard } = await import(`@/lib/sessions/plan`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const { sessionsRouter } = await import(`@/lib/trpc/sessions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `bus-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@bus.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Bus`, slug: `bus-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `BU` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const root = async (text = `root`) =>
    (await sendSession({ userId: owner, input: { teamId, deviceId, text } })).sessionId
  const child = async (parentId: string, text = `child`) =>
    (await sendSession({ userId: owner, input: { parentId, text } })).sessionId
  return { owner, teamId, deviceId, root, child, host: caller(owner) }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

const caller = (userId: string) =>
  sessionsRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`),
  } as never)

const row = async (id: string) =>
  (await db.select().from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

/** The rows `from` posted into `sessionId` (the bus's), oldest first. */
const fromChild = (sessionId: string, from: string) =>
  db
    .select()
    .from(messages)
    .where(and(inSession(sessionId), eq(messages.authorSessionId, from)))
    .orderBy(asc(messages.createdAt), asc(messages.id))

const cardKind = (m: { card: unknown }) => (m.card as { kind?: string } | null)?.kind ?? null

describe.skipIf(!ready.ok)(`the bus: a child's transitions reach its parent`, () => {
  it(`a turn's reply lands in the parent as the child's own waking message`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    const reply = await w.host.reply({ sessionId: childId, body: `  Found the bug.  ` })
    // The child keeps its own row (no author_session_id: never input to it).
    const [own] = await db.select(messageWithSession()).from(messages).where(eq(messages.id, reply.messageId))
    expect(own).toMatchObject({ sessionId: childId, authorKind: `agent`, authorSessionId: null, body: `  Found the bug.  ` })
    const relayed = (await fromChild(rootId, childId)).filter((m) => m.card === null)
    expect(relayed).toMatchObject([
      { authorKind: `agent`, authorSessionId: childId, body: `Found the bug.`, consumedAt: null },
    ])
    // The parent's device consumes it like a person's message.
    await w.host.consume({ sessionId: rootId, messageIds: [relayed[0]!.id] })
    expect((await db.select().from(messages).where(eq(messages.id, relayed[0]!.id)))[0]!.consumedAt).not.toBeNull()

    // A root's reply with nobody listening stays on the root only.
    await w.host.claim({ sessionId: rootId, deviceId: w.deviceId })
    await w.host.reply({ sessionId: rootId, body: `Root speaking.` })
    expect(await db.select().from(messages).where(eq(messages.authorSessionId, rootId))).toEqual([])
  })

  it(`an open question reaches the parent as an ask mirror; the child's card stays ONE row`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    const asked = await w.host.reply({
      sessionId: childId,
      body: ``,
      card: { kind: `question`, question: `Which DB?`, options: [`pg`, `sqlite`] },
    })
    expect((await row(childId)).needsInput).toBe(true)
    const cards = await db
      .select()
      .from(messages)
      .where(sql`${messages.card}->>'kind' = 'question' AND ${messages.rootId} = ${rootId}`)
    expect(cards.map((m) => m.id)).toEqual([asked.messageId])
    const [ask] = (await fromChild(rootId, childId)).filter((m) => cardKind(m) === `ask`)
    expect(ask).toMatchObject({
      authorKind: `agent`,
      authorSessionId: childId,
      consumedAt: null,
      card: { kind: `ask`, sessionId: childId, messageId: asked.messageId },
    })
    expect(ask!.body).toContain(`Asks: Which DB?`)
    expect(ask!.body).toContain(`Options: pg | sqlite`)

    // The MCP path (sessions_ask / plan_propose) mirrors the same way.
    const own = await postAskCard({
      userId: w.owner,
      sessionId: childId,
      card: buildQuestionCard({ question: `Ship it?`, options: [{ label: `yes` }, { label: `no` }] }),
    })
    const asks = (await fromChild(rootId, childId)).filter((m) => cardKind(m) === `ask`)
    expect(asks.map((m) => (m.card as { messageId: string }).messageId)).toEqual([asked.messageId, own.messageId])
    expect(asks[1]!.consumedAt).toBeNull()
  })

  it(`a first wall posts ONE waking session_blocked line and pushes; needs_input holds while blocked`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    const wall = {
      kind: `rate_limit`,
      agent: `claude`,
      window: `session`,
      resetsAt: `2026-10-21T12:00:00.000Z`,
      since: `2026-10-21T10:00:00.000Z`,
    } as const
    notifyBlocked.mockClear()
    await w.host.report({ sessionId: childId, blocked: wall })
    expect(await row(childId)).toMatchObject({ blocked: wall, needsInput: true })
    // Still blocked: said once.
    await w.host.report({ sessionId: childId, blocked: wall, busy: false })
    await w.host.report({ sessionId: childId, needsInput: false })
    expect((await row(childId)).needsInput).toBe(true)
    const lines = (await fromChild(rootId, childId)).filter(
      (m) => (m.card as { event?: string } | null)?.event === `session_blocked`
    )
    expect(lines).toMatchObject([
      {
        authorKind: `system`,
        authorSessionId: childId,
        consumedAt: null,
        body: BUS_COPY.session_blocked(wall),
        card: { kind: `event`, event: `session_blocked`, sessionId: childId },
      },
    ])
    expect(notifyBlocked).toHaveBeenCalledTimes(1)
    // The wall clears: so does needs_input (no card waits).
    await w.host.report({ sessionId: childId, blocked: null })
    expect(await row(childId)).toMatchObject({ blocked: null, needsInput: false })

    // With a card waiting, clearing the wall keeps it under Needs you: the
    // session's own MCP card (sessions_ask)…
    await w.host.report({ sessionId: childId, blocked: wall })
    const own = await postAskCard({
      userId: w.owner,
      sessionId: childId,
      card: buildQuestionCard({ question: `Own card?`, options: [{ label: `a` }, { label: `b` }] }),
    })
    await w.host.report({ sessionId: childId, blocked: null })
    expect(await row(childId)).toMatchObject({ blocked: null, needsInput: true })
    await caller(w.owner).answer({ messageId: own.messageId, answers: [`a`] })
    expect((await row(childId)).needsInput).toBe(false)
    // …and the device's card (`reply` with a question) alike.
    await w.host.report({ sessionId: childId, blocked: wall })
    await w.host.reply({ sessionId: childId, body: ``, card: { kind: `question`, question: `Wait?` } })
    await w.host.report({ sessionId: childId, blocked: null })
    expect(await row(childId)).toMatchObject({ blocked: null, needsInput: true })
  })

  it(`a PR opened by the child wakes its parent with a pr_opened line`, async () => {
    const w = await world()
    const fullName = `o/bus-${id8()}`
    const [repo] = await db
      .insert(repositories)
      .values({ teamId: w.teamId, fullName, defaultBranch: `main` })
      .returning({ id: repositories.id })
    const rootId = await w.root()
    const childId = await w.child(rootId)
    const { pullRequest } = await recordPullRequestOpened({
      teamId: w.teamId,
      rootSessionId: rootId,
      sessionId: childId,
      repositoryId: repo!.id,
      repoFullName: fullName,
      number: 7,
      url: `https://github.com/${fullName}/pull/7`,
      title: `PR 7`,
      head: `exp/bus`,
      base: `main`,
      issueIds: [],
      actorUserId: w.owner,
    })
    const lines = (await fromChild(rootId, childId)).filter(
      (m) => (m.card as { event?: string } | null)?.event === `pr_opened`
    )
    expect(lines).toMatchObject([
      {
        authorKind: `system`,
        consumedAt: null,
        body: BUS_COPY.pr_opened(7, pullRequest.url),
        card: { kind: `event`, event: `pr_opened`, sessionId: childId, pullRequestId: pullRequest.id },
      },
    ])
  })

  it(`an ended child posts its final report card, waking; into an ended parent it is pre-consumed`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    await endSession({ userId: w.owner, sessionId: childId, summary: `Shipped in PR #3.` })
    const [report] = (await fromChild(rootId, childId)).filter((m) => cardKind(m) === `report`)
    expect(report).toMatchObject({
      authorKind: `agent`,
      body: `Shipped in PR #3.`,
      consumedAt: null,
      card: { kind: `report`, sessionId: childId, summary: `Shipped in PR #3.`, final: true },
    })

    // FEED-134: the device reporting a ROOT's process gone leaves it idle
    // (the thread survives an app quit or a crash); a CHILD's process gone
    // ends that one row (endDeadSessionInTx), and its own live child
    // reports later into an ended reader.
    const live = await w.child(rootId, `still working`)
    await w.host.claim({ sessionId: rootId, deviceId: w.deviceId })
    // Release train 2026-10-23: only a root that finished a turn survives;
    // one that never did (a refused launch) ends.
    await w.host.report({ sessionId: rootId, status: `idle` })
    await w.host.report({ sessionId: rootId, status: `ended` })
    expect(await row(rootId)).toMatchObject({ status: `idle`, busy: false })
    const mid = await w.child(rootId, `a worker`)
    const grand = await w.child(mid, `its helper`)
    await w.host.claim({ sessionId: mid, deviceId: w.deviceId })
    await w.host.report({ sessionId: mid, status: `ended` })
    expect(await row(mid)).toMatchObject({ status: `ended` })
    expect(await row(grand)).toMatchObject({ status: `queued` })
    await w.host.claim({ sessionId: grand, deviceId: w.deviceId })
    await w.host.report({ sessionId: grand, status: `ended` })
    const [orphan] = (await fromChild(mid, grand)).filter((m) => cardKind(m) === `report`)
    expect(orphan!.consumedAt).not.toBeNull()
    // A person's End still ends the root, its live child with it.
    await endSession({ userId: w.owner, sessionId: rootId })
    expect(await row(rootId)).toMatchObject({ status: `ended` })
    expect(await row(live)).toMatchObject({ status: `ended` })
  })

  it(`a first claim posts an informational session_claimed line; a root's own event line lands on itself`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    // Idempotent: a second claim says nothing.
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    expect(await fromChild(rootId, childId)).toMatchObject([
      {
        authorKind: `system`,
        body: BUS_COPY.session_claimed,
        card: { kind: `event`, event: `session_claimed`, sessionId: childId },
      },
    ])
    expect((await fromChild(rootId, childId))[0]!.consumedAt).not.toBeNull()

    // A root nobody watches: its claim needs no line at all…
    await w.host.claim({ sessionId: rootId, deviceId: w.deviceId })
    const ownLines = async (event: string) =>
      (await db.select().from(messages).where(inSession(rootId))).filter(
        (m) => (m.card as { event?: string } | null)?.event === event && m.authorSessionId !== childId
      )
    expect(await ownLines(`session_claimed`)).toEqual([])
    // …while its other event lines land on the root itself, pre-consumed.
    await w.host.report({
      sessionId: rootId,
      blocked: { kind: `rate_limit`, agent: `claude`, window: `weekly`, resetsAt: null, since: `2026-10-21T10:00:00.000Z` },
    })
    const own = await ownLines(`session_blocked`)
    expect(own).toMatchObject([{ authorKind: `system`, authorSessionId: null }])
    expect(own[0]!.consumedAt).not.toBeNull()
  })

  it(`a root's End ends its tree deepest first, every child reporting into its parent first`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    const grandId = await w.child(childId, `grand`)
    const ended = await endSession({ userId: w.owner, sessionId: rootId })
    expect(ended.endedIds).toEqual([grandId, childId, rootId])
    // The grandchild reported while its parent was still live: waking.
    const [grandReport] = (await fromChild(childId, grandId)).filter((m) => cardKind(m) === `report`)
    expect(grandReport).toMatchObject({ consumedAt: null, card: { final: true, sessionId: grandId } })
    const [childReport] = (await fromChild(rootId, childId)).filter((m) => cardKind(m) === `report`)
    expect(childReport).toMatchObject({ consumedAt: null, card: { final: true, sessionId: childId } })
    expect(childReport!.createdAt.getTime()).toBeGreaterThanOrEqual(grandReport!.createdAt.getTime())
    // The root reports nowhere (no parent, no watcher).
    expect(await db.select().from(messages).where(eq(messages.authorSessionId, rootId))).toEqual([])
  })
})

describe.skipIf(!ready.ok)(`the bus: watchers (wakeOn) and answers`, () => {
  it(`wakeOn makes the CALLER a listener until the watched session ends`, async () => {
    const w = await world()
    const watcher = await w.root(`watcher`)
    const other = await w.root(`watched`)
    const gone = await w.root(`gone`)
    await endSession({ userId: w.owner, sessionId: gone })
    // A caller session is required.
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: watcher, wakeOn: [other] } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // Idempotent; ended sessions and the caller itself are skipped.
    for (let i = 0; i < 2; i++) {
      await sendSession({
        userId: w.owner,
        callerSessionId: watcher,
        viaMcp: true,
        input: { sessionId: watcher, wakeOn: [other, gone, watcher] },
      })
    }
    expect((await row(watcher)).wakeOn).toEqual([other])

    // The watched root's turn wakes the watcher (it has no parent).
    await w.host.claim({ sessionId: other, deviceId: w.deviceId })
    await w.host.reply({ sessionId: other, body: `Halfway there.` })
    const relayed = (await fromChild(watcher, other)).filter((m) => m.card === null)
    expect(relayed).toMatchObject([{ authorKind: `agent`, body: `Halfway there.`, consumedAt: null }])
    // Its claim line went to the watcher (not onto itself).
    expect(
      (await fromChild(watcher, other)).some(
        (m) => (m.card as { event?: string } | null)?.event === `session_claimed`
      )
    ).toBe(true)

    // Its end reports to the watcher, then the watch is gone.
    await endSession({ userId: w.owner, sessionId: other, summary: `Done there.` })
    const [report] = (await fromChild(watcher, other)).filter((m) => cardKind(m) === `report`)
    expect(report).toMatchObject({ body: `Done there.`, consumedAt: null, card: { final: true } })
    expect((await row(watcher)).wakeOn).toEqual([])
  })

  it(`a root started from inside a session is watched by it`, async () => {
    const w = await world()
    const starter = await w.root(`starter`)
    const spawned = await sendSession({
      userId: w.owner,
      callerSessionId: starter,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `look into it` },
    })
    expect(await row(spawned.sessionId)).toMatchObject({ parentId: null, rootId: spawned.sessionId })
    expect((await row(starter)).wakeOn).toEqual([spawned.sessionId])
    const cards = (await db.select().from(messages).where(inSession(starter))).map((m) => m.card)
    expect(cards).toContainEqual({ kind: `thread_started`, sessionId: spawned.sessionId })
    await endSession({ userId: w.owner, sessionId: spawned.sessionId, summary: `Nothing there.` })
    expect((await fromChild(starter, spawned.sessionId)).filter((m) => cardKind(m) === `report`)).toHaveLength(1)
    expect((await row(starter)).wakeOn).toEqual([])
  })

  it(`sessions.send({answer}) answers the target's open card; the first answer wins`, async () => {
    const w = await world()
    const rootId = await w.root()
    const childId = await w.child(rootId)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    // Nothing open yet.
    await expect(
      sendSession({ userId: w.owner, callerSessionId: rootId, viaMcp: true, input: { sessionId: childId, answer: { answer: `x` } } })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    const asked = await w.host.reply({
      sessionId: childId,
      body: ``,
      card: { kind: `question`, question: `Which DB?`, options: [`pg`, `sqlite`] },
    })
    // The parent answers over its MCP key.
    const sent = await sendSession({
      userId: w.owner,
      callerSessionId: rootId,
      viaMcp: true,
      input: { sessionId: childId, answer: { answers: [`pg`] } },
    })
    expect(sent).toMatchObject({ sessionId: childId, messageId: null })
    const [card] = await db.select().from(messages).where(eq(messages.id, asked.messageId))
    expect(card!.answeredAt).not.toBeNull()
    expect(card!.card).toMatchObject({ kind: `question`, answer: `pg`, answers: [`pg`] })
    expect((await row(childId)).needsInput).toBe(false)
    // A second answer (a person in the card, or by id) is CONFLICT.
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: childId, answer: { messageId: asked.messageId, answer: `sqlite` } } })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    await expect(caller(w.owner).answer({ messageId: asked.messageId, answer: `sqlite` })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    // An ended session has nothing to answer.
    await endSession({ userId: w.owner, sessionId: childId })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: childId, answer: { answer: `late` }, text: `late` } })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})

const readAt = async (userId: string, sessionId: string) =>
  (
    await db
      .select({ readAt: sessionReads.readAt })
      .from(sessionReads)
      .where(and(eq(sessionReads.userId, userId), eq(sessionReads.sessionId, sessionId)))
  )[0]?.readAt ?? null

describe.skipIf(!ready.ok)(`#EXP-1452: an agent's start and read are its member's`, () => {
  it(`a session an agent starts opens with the agent's message, delivered by the claim`, async () => {
    const w = await world()
    const starter = await w.root(`starter`)
    const spawned = await sendSession({
      userId: w.owner,
      callerSessionId: starter,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `look into it` },
    })
    const [first] = await db.select().from(messages).where(eq(messages.id, spawned.messageId!))
    expect(first).toMatchObject({ authorKind: `agent`, authorSessionId: starter, body: `look into it`, consumedAt: null })
    // What the member's agent wrote, the member read.
    expect(await readAt(w.owner, spawned.sessionId)).not.toBeNull()
    const { brief } = await w.host.brief({ sessionId: spawned.sessionId })
    expect(brief).toContain(`look into it`)
    await w.host.claim({ sessionId: spawned.sessionId, deviceId: w.deviceId })
    expect((await db.select().from(messages).where(eq(messages.id, first!.id)))[0]!.consumedAt).not.toBeNull()

    // A child started over the parent's MCP key is the parent's message too.
    const child = await sendSession({
      userId: w.owner,
      callerSessionId: starter,
      viaMcp: true,
      input: { parentId: starter, text: `split this` },
    })
    const [childFirst] = await db.select().from(messages).where(eq(messages.id, child.messageId!))
    expect(childFirst).toMatchObject({ authorKind: `agent`, authorSessionId: starter })
    // Over tRPC a person's start stays a person's.
    const own = await sendSession({ userId: w.owner, input: { teamId: w.teamId, deviceId: w.deviceId, text: `mine` } })
    const [ownFirst] = await db.select().from(messages).where(eq(messages.id, own.messageId!))
    expect(ownFirst).toMatchObject({ authorKind: `user`, authorSessionId: null })
  })

  it(`a line of another chat the agent consumes marks that chat read up to it; its own tree never`, async () => {
    const w = await world()
    const starter = await w.root(`starter`)
    await w.host.claim({ sessionId: starter, deviceId: w.deviceId })
    const spawned = await sendSession({
      userId: w.owner,
      callerSessionId: starter,
      viaMcp: true,
      input: { teamId: w.teamId, deviceId: w.deviceId, text: `look into it` },
    })
    await w.host.setRead({ sessionIds: [spawned.sessionId], read: false })
    await w.host.claim({ sessionId: spawned.sessionId, deviceId: w.deviceId })
    await w.host.reply({ sessionId: spawned.sessionId, body: `Found it.` })
    const relayed = (await fromChild(starter, spawned.sessionId)).filter((m) => m.card === null)
    expect(relayed).toHaveLength(1)
    const before = await readAt(w.owner, spawned.sessionId)
    expect(before!.getTime()).toBeLessThan(relayed[0]!.createdAt.getTime())
    await w.host.consume({ sessionId: starter, messageIds: [relayed[0]!.id] })
    expect((await readAt(w.owner, spawned.sessionId))!.getTime()).toBeGreaterThanOrEqual(relayed[0]!.createdAt.getTime())

    // A child's line in the agent's OWN tree stays news for the member.
    const childId = await w.child(starter)
    await w.host.claim({ sessionId: childId, deviceId: w.deviceId })
    const ownBefore = await readAt(w.owner, starter)
    await w.host.reply({ sessionId: childId, body: `Child done.` })
    const fromOwnChild = (await fromChild(starter, childId)).filter((m) => m.card === null)
    await w.host.consume({ sessionId: starter, messageIds: [fromOwnChild[0]!.id] })
    expect(await readAt(w.owner, starter)).toEqual(ownBefore)
  })
})

