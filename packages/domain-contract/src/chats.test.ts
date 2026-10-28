import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/chats.json" with { type: "json" }
import titleFixture from "../fixtures/session-title.json" with { type: "json" }
import { sessionTitle } from "./session-title"
import { partitionThreads, type TreeMessage } from "./session-tree"
import {
  agentBubbles,
  agentInstructionsChanged,
  agentMenuEntries,
  chatDeleteTitle,
  chatSelectionFailedTitle,
  chatList,
  chatSelectionActions,
  chatSelectionRange,
  chatsUnread,
  chatPreview,
  chatTimeLabel,
  commandDivider,
  conversationDigest,
  digestMessages,
  DIGEST_AGENT_TAIL,
  plainLine,
  surfaceReply,
  threadMenuEntries,
  transcriptWindowStart,
} from "./chats"

// Thread 58cb32df: the Chats reference replays every case group of
// fixtures/chats.json (the mirrors replay the same file).
describe(`chats.json`, () => {
  for (const c of fixture.previewCases) test(`preview: ${c.name}`, () => expect(chatPreview(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.timeCases) test(`time: ${c.name}`, () => expect(chatTimeLabel(c.at, c.now, c.utcOffsetMinutes)).toBe(c.expected))
  for (const c of fixture.listCases) test(`list: ${c.name}`, () => expect(chatList(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.bubbleCases) test(`bubbles: ${c.name}`, () => expect(agentBubbles(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.menuCases) test(`menu: ${c.name}`, () => expect(threadMenuEntries(c.input)).toEqual(c.expected as never))
  for (const c of fixture.agentMenuCases) test(`agent menu: ${c.name}`, () => expect(agentMenuEntries(c.input)).toEqual(c.expected as never))
  for (const c of fixture.replyCases) test(`reply: ${c.name}`, () => expect(surfaceReply(c.event)).toEqual(c.expected as never))
  for (const c of fixture.dividerCases) test(`divider: ${JSON.stringify(c.body)}`, () => expect(commandDivider(c.body)).toBe(c.expected as never))
  for (const c of fixture.agentHint.cases) test(`agent hint: ${c.name}`, () => expect(agentInstructionsChanged(c.input)).toBe(c.expected))
  for (const c of fixture.windowCases) test(`window: ${c.name}`, () => expect(transcriptWindowStart(c)).toBe(c.start))
  for (const c of fixture.plainCases) test(`plain: ${JSON.stringify(c.text)}`, () => expect(plainLine(c.text)).toBe(c.expected))
  for (const c of fixture.digestCases)
    test(`digest: ${c.name}`, () => expect(digestMessages(c.sessionId, c.digest as never)).toEqual(c.messages as never))
  for (const c of fixture.selectionCases) test(`selection: ${c.name}`, () => expect(chatSelectionActions(c.input as never)).toEqual(c.expected as never))
  for (const c of fixture.rangeCases) test(`range: ${c.name}`, () => expect(chatSelectionRange(c.order, c.anchor, c.target)).toEqual(c.expected))
  for (const c of fixture.dotCases) test(`dot: ${c.name}`, () => expect(chatsUnread(c.input.groups as never, c.input.reads as never)).toBe(c.expected))
  test(`delete titles`, () => {
    expect(chatDeleteTitle(1)).toBe(fixture.copy.confirmDelete.title)
    expect(chatDeleteTitle(3)).toBe(`Delete 3 chats?`)
  })
  test(`selection failure titles`, () => {
    expect(chatSelectionFailedTitle(2, 5)).toBe(`2 of 5 chats couldn't be updated`)
    expect(chatSelectionFailedTitle(5, 5)).toBe(fixture.copy.toasts.selectionFailed)
    expect(chatSelectionFailedTitle(1, 1)).toBe(fixture.copy.toasts.selectionFailed)
  })
  test(`every menu entry and tab has copy`, () => {
    for (const e of [`delete`, `compact`, `newConversation`, `settings`]) expect(fixture.copy.menu[e as keyof typeof fixture.copy.menu]).toBeTruthy()
    for (const t of fixture.shell.tabs) expect(fixture.copy.tabs[t as keyof typeof fixture.copy.tabs]).toBeTruthy()
  })
})

// #EXP-1380: the list rules over each conversation's DIGEST answer exactly
// as over the whole conversation (the web list reads digests only).
const viaDigest = (messages: readonly TreeMessage[]): TreeMessage[] =>
  [...new Set(messages.map((m) => m.sessionId))].flatMap((id) => digestMessages(id, conversationDigest(id, messages)))

describe(`conversation digest`, () => {
  for (const c of fixture.previewCases)
    test(`preview: ${c.name}`, () => {
      const input = c.input as unknown as Parameters<typeof chatPreview>[0]
      expect(chatPreview({ ...input, messages: viaDigest(input.messages) })).toEqual(c.expected as never)
    })
  for (const c of fixture.listCases)
    test(`list: ${c.name}`, () => {
      const input = c.input as unknown as Parameters<typeof chatList>[0]
      expect(chatList({ ...input, messages: viaDigest(input.messages) })).toEqual(c.expected as never)
    })
  for (const c of fixture.bubbleCases)
    test(`bubbles: ${c.name}`, () => {
      const input = c.input as unknown as Parameters<typeof agentBubbles>[0]
      expect(agentBubbles({ ...input, messages: viaDigest(input.messages) })).toEqual(c.expected as never)
    })
  for (const c of titleFixture.cases)
    test(`title: ${c.name}`, () => {
      const input = c.input as unknown as { session: never; links: never; issues: never; messages: TreeMessage[] }
      const messages = input.messages.map((m) => ({ answeredAt: null, consumedAt: null, authorId: null, ...m }))
      expect(sessionTitle(input.session, input.links, input.issues, viaDigest(messages))).toBe(c.expected)
    })

  // A random conversation mix: every rule the list reads agrees.
  test(`random conversations agree`, () => {
    let seed = 7
    const rand = () => ((seed = (seed * 1103515245 + 12345) % 2 ** 31) / 2 ** 31)
    const pick = <T,>(xs: readonly T[]) => xs[Math.floor(rand() * xs.length)]!
    const sessions = [`s1`, `s2`, `s3`]
    const cards = [null, null, null, { kind: `question`, question: `Ship it?` }, { kind: `plan`, plan: `# Plan\n- a` }, { kind: `ask`, question: `Pick` }, { kind: `picture` }, { kind: `surface`, title: `Board` }, { kind: `event` }, { kind: `replies`, items: [] }]
    const bodies = [``, `  `, `hello **world**`, `![shot](/a.png)`, `![x](/a.png)\nsecond line`, `/new`, `/compact now`, `a [link](https://x.y) here`]
    for (let round = 0; round < 200; round++) {
      const messages: TreeMessage[] = []
      const n = Math.floor(rand() * 40)
      for (let i = 0; i < n; i++) {
        const at = new Date(Date.UTC(2026, 9, 1) + Math.floor(rand() * 50) * 1000).toISOString()
        messages.push({
          id: `m${round}-${i}`,
          sessionId: pick(sessions),
          authorKind: pick([`user`, `agent`, `agent`, `system`]),
          authorId: null,
          authorSessionId: rand() < 0.2 ? pick([`s1`, `s2`, `s9`]) : null,
          body: pick(bodies),
          card: pick(cards),
          answeredAt: rand() < 0.5 ? null : at,
          consumedAt: null,
          createdAt: at,
        })
      }
      const digested = viaDigest(messages)
      const reads = [{ sessionId: `s1`, readAt: new Date(Date.UTC(2026, 9, 1) + 25_000).toISOString() }]
      for (const id of sessions) {
        for (const needsYou of [false, true])
          expect(chatPreview({ rootId: id, messages: digested, needsYou, busy: false })).toEqual(
            chatPreview({ rootId: id, messages, needsYou, busy: false })
          )
        const session = { id, rootId: id, title: null }
        expect(sessionTitle(session, [], [], digested)).toBe(sessionTitle(session, [], [], messages))
      }
      const tree = sessions.map((id) => ({ id, rootId: id, parentId: null, teamId: `t`, kind: `thread`, status: `idle`, busy: false, needsInput: false, title: null, userId: `u`, createdAt: `2026-10-01T00:00:00.000Z`, updatedAt: `2026-10-01T00:00:00.000Z` }))
      const groups = (ms: TreeMessage[]) => partitionThreads({ sessions: tree as never, sessionIssues: [], issues: [], messages: ms, teamId: `t`, reads })
      expect(groups(digested)).toEqual(groups(messages))
      expect(chatList({ groups: groups(messages), messages: digested, reads, filter: `all`, now: `2026-10-02T00:00:00.000Z`, utcOffsetMinutes: 0 })).toEqual(
        chatList({ groups: groups(messages), messages, reads, filter: `all`, now: `2026-10-02T00:00:00.000Z`, utcOffsetMinutes: 0 })
      )
    }
  })

  // Review of #1218: the cut runs AFTER the leading blank / image lines are
  // dropped, so a long first prompt titles (and previews) the same.
  test(`long bodies keep the title and the preview's words`, () => {
    const images = Array.from({ length: 14 }, (_, i) => `![shot ${i}](/api/attachments/0000000${i}-aaaa-bbbb-cccc-dddddddddddd)`).join(`\n`)
    const long = `Fix the window so the waiting card is never cut off, `.repeat(14).trim()
    const bodies = [`\n \n${images}\n\u00a0\n${long}\nsecond`, `${images}\n![a](b) caption ${long}`, `${long} ${long}`, `😀`.repeat(520), images]
    for (const body of bodies) {
      const messages: TreeMessage[] = [
        { id: `u1`, sessionId: `s1`, authorKind: `user`, authorId: null, authorSessionId: null, body, card: null, answeredAt: null, consumedAt: null, createdAt: `2026-10-24T12:00:01.000Z` },
        { id: `a1`, sessionId: `s1`, authorKind: `agent`, authorId: null, authorSessionId: null, body, card: null, answeredAt: null, consumedAt: null, createdAt: `2026-10-24T12:00:02.000Z` },
        { id: `p1`, sessionId: `s1`, authorKind: `agent`, authorId: null, authorSessionId: null, body: ``, card: { kind: `plan`, plan: body }, answeredAt: null, consumedAt: null, createdAt: `2026-10-24T12:00:03.000Z` },
      ]
      const session = { id: `s1`, rootId: `s1`, title: null }
      expect(sessionTitle(session, [], [], viaDigest(messages))).toBe(sessionTitle(session, [], [], messages))
      for (const needsYou of [false, true]) {
        const whole = chatPreview({ rootId: `s1`, messages, needsYou, busy: false }).text
        const digested = chatPreview({ rootId: `s1`, messages: viaDigest(messages), needsYou, busy: false }).text
        expect(whole.startsWith(digested)).toBe(true)
        expect(digested.length).toBeGreaterThanOrEqual(Math.min(whole.length, 60))
      }
    }
    // Cuts count code points (SQL `left()`), never splitting a pair.
    const digest = conversationDigest(`s1`, [
      { id: `a1`, sessionId: `s1`, authorKind: `agent`, authorId: null, authorSessionId: null, body: `😀`.repeat(520), card: null, answeredAt: null, consumedAt: null, createdAt: `2026-10-24T12:00:02.000Z` },
    ])
    expect(digest.preview!.body).toBe(`😀`.repeat(500))
  })

  test(`the unread tail is capped`, () => {
    const messages: TreeMessage[] = Array.from({ length: DIGEST_AGENT_TAIL + 20 }, (_, i) => ({
      id: `a${String(i).padStart(4, `0`)}`, sessionId: `s1`, authorKind: `agent`, authorId: null, authorSessionId: null,
      body: `reply ${i}`, card: null, answeredAt: null, consumedAt: null, createdAt: new Date(Date.UTC(2026, 9, 1) + i * 1000).toISOString(),
    }))
    expect(conversationDigest(`s1`, messages).agentTimes).toHaveLength(DIGEST_AGENT_TAIL)
    expect(viaDigest(messages).filter((m) => m.authorKind === `agent` && m.card === null)).toHaveLength(DIGEST_AGENT_TAIL)
  })
})
