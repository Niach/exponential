import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  ackAnswer,
  activeQuestionIds,
  mergeNarrationFragment,
  modeChip,
  parseConfigState,
  planModeToggle,
  parseRateLimit,
  parseSessionUsage,
  parseToolKind,
  rateLimitClears,
  feedItemBytes,
  TOOL_KINDS,
  answerKey,
  applyQuestionResolved,
  askStepperView,
  beginAnswer,
  clearAnswer,
  collectSubagents,
  consumeEcho,
  createActivityCoalescer,
  failAnswer,
  groupFeedRows,
  isAnswerLocked,
  looksLikeMarkdown,
  pushEcho,
  resumesAfterCompaction,
  rowClass,
  subagentIdOf,
  summarizeSubagentRow,
  spliceBeforeQuestion,
  transcriptGap,
  upsertQuestion,
  visibleSubagentTabs,
  COMPACTION_TIMEOUT_MS,
  ECHO_CAP,
  ECHO_TTL_MS,
  diffTruncationNote,
  freeAnswerFor,
  optionForHotkey,
  optionHotkey,
  pendingAnswerable,
  pendingPlaceholder,
  rateLimitBanner,
  rateLimitResetsAtMs,
  splitTruncatedDiff,
  toolGroupCaption,
  FREE_TEXT_KEY,
  PLAN_PENDING_PLACEHOLDER,
  QUESTION_PENDING_PLACEHOLDER,
  type AnswerableCard,
  type AnswerStates,
  type EchoEntry,
  type FeedRow,
  type QuestionLike,
} from "./agent-feed"
import { CONFIG_DEFAULT_VALUE_LABEL } from "./steer-commands"
import { contract } from "@exp/domain-contract"
import designTokens from "../../../../packages/design-tokens/tokens.json" with { type: "json" }

describe(`local-echo dedupe`, () => {
  it(`consumes a matching echo exactly once`, () => {
    const echoes: EchoEntry[] = []
    pushEcho(echoes, `fix the login bug`, 1_000)
    expect(consumeEcho(echoes, `fix the login bug`, 2_000)).toBe(true)
    // The second identical event (e.g. relay replay) is NOT swallowed.
    expect(consumeEcho(echoes, `fix the login bug`, 3_000)).toBe(false)
  })

  it(`matches on trimmed text`, () => {
    const echoes: EchoEntry[] = []
    pushEcho(echoes, `  hello  `, 0)
    expect(consumeEcho(echoes, `hello\n`, 1)).toBe(true)
  })

  it(`expired echoes never match`, () => {
    const echoes: EchoEntry[] = []
    pushEcho(echoes, `late message`, 0)
    expect(consumeEcho(echoes, `late message`, ECHO_TTL_MS + 1)).toBe(false)
    expect(echoes).toHaveLength(0)
  })

  it(`keeps at most ECHO_CAP entries`, () => {
    const echoes: EchoEntry[] = []
    for (let i = 0; i < ECHO_CAP + 3; i++) pushEcho(echoes, `msg ${i}`, i)
    expect(echoes).toHaveLength(ECHO_CAP)
    expect(consumeEcho(echoes, `msg 0`, 10)).toBe(false)
    expect(consumeEcho(echoes, `msg ${ECHO_CAP + 2}`, 10)).toBe(true)
  })
})

describe(`activeQuestionIds — protocol v2 cards`, () => {
  it(`an id-carrying card stays answerable behind any later event`, () => {
    const feed = [
      { id: 1, kind: `question`, questionId: `tu_1`, planMode: true },
      { id: 2, kind: `tool` },
      { id: 3, kind: `narration`, text: `Working on it` },
      { id: 4, kind: `user_message`, text: `go` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([1]))
  })

  it(`every unresolved step of an ask is answerable at once`, () => {
    const feed = [
      { id: 1, kind: `question`, questionId: `tu_1#0` },
      { id: 2, kind: `question`, questionId: `tu_1#1` },
      { id: 3, kind: `question`, questionId: `tu_1#submit` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([1, 2, 3]))
  })

  it(`a resolved card is never answerable`, () => {
    const feed = [
      { id: 1, kind: `question`, questionId: `tu_1#0`, resolved: true },
      { id: 2, kind: `question`, questionId: `tu_1#1` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([2]))
  })
})

type QuestionItem = QuestionLike

describe(`resolved cards`, () => {
  it(`a resolved question is never active`, () => {
    expect(
      activeQuestionIds([
        { id: 1, kind: `question`, questionId: `tu_1`, resolved: true },
      ])
    ).toEqual(new Set())
    expect(
      activeQuestionIds([
        { id: 1, kind: `question`, questionId: `tu_2`, resolved: true },
      ])
    ).toEqual(new Set())
  })
})

// ── Protocol v2 question identity ────────────────────────────────────────────

describe(`upsertQuestion`, () => {
  const card = (over: Partial<QuestionItem> = {}): QuestionItem => ({
    id: 7,
    kind: `question`,
    questionId: `tu_1`,
    text: `Which color?`,
    ...over,
  })

  it(`replaces the card in place, keeping its feed id`, () => {
    const feed = [{ id: 3, kind: `narration` } as QuestionItem, card()]
    const next = upsertQuestion(feed, `tu_1`, {
      kind: `question`,
      questionId: `tu_1`,
      text: `Which color?`,
      total: 2,
    })!
    expect(next).toHaveLength(2)
    expect(next[1]).toMatchObject({ id: 7, total: 2 })
  })

  it(`a re-emission never clears an applied resolution`, () => {
    const feed = [card({ resolved: true, answer: `Red` })]
    const next = upsertQuestion(feed, `tu_1`, {
      kind: `question`,
      questionId: `tu_1`,
      text: `Which color?`,
    })!
    expect(next[0]).toMatchObject({ resolved: true, answer: `Red` })
  })

  it(`is null for an unknown id — the caller appends`, () => {
    expect(
      upsertQuestion([card()], `tu_9`, { kind: `question`, questionId: `tu_9` })
    ).toBeNull()
  })
})

describe(`spliceBeforeQuestion (EXP-483)`, () => {
  const narration = (id: number, text: string): QuestionItem =>
    ({ id, kind: `narration`, text }) as QuestionItem

  it(`splices before the FIRST card of the anchored ask group`, () => {
    const feed: QuestionItem[] = [
      narration(1, `working`),
      { id: 2, kind: `question`, questionId: `tu_1#0`, askId: `tu_1` },
      { id: 3, kind: `question`, questionId: `tu_1#1`, askId: `tu_1` },
    ]
    const next = spliceBeforeQuestion(feed, `tu_1`, narration(4, `summary`))!
    expect(next.map((i) => i.id)).toEqual([1, 4, 2, 3])
  })

  it(`matches a plan card by its wire questionId, resolved or not`, () => {
    const feed: QuestionItem[] = [
      {
        id: 1,
        kind: `question`,
        questionId: `tu_plan`,
        planMode: true,
        resolved: true,
      },
    ]
    const next = spliceBeforeQuestion(feed, `tu_plan`, narration(2, `plan prose`))!
    expect(next.map((i) => i.id)).toEqual([2, 1])
  })

  it(`keeps the order of successive anchored narrations`, () => {
    const feed: QuestionItem[] = [
      { id: 1, kind: `question`, questionId: `tu_1#0`, askId: `tu_1` },
    ]
    const once = spliceBeforeQuestion(feed, `tu_1`, narration(2, `first`))!
    const twice = spliceBeforeQuestion(once, `tu_1`, narration(3, `second`))!
    expect(twice.map((i) => i.id)).toEqual([2, 3, 1])
  })

  it(`is null when no card matches — the caller appends`, () => {
    expect(
      spliceBeforeQuestion(
        [narration(1, `working`)],
        `tu_gone`,
        narration(2, `late`)
      )
    ).toBeNull()
  })
})

describe(`applyQuestionResolved`, () => {
  const ask = (): QuestionItem[] => [
    { id: 1, kind: `narration` },
    { id: 2, kind: `question`, questionId: `a#0`, askId: `a`, index: 1 },
    { id: 3, kind: `question`, questionId: `a#1`, askId: `a`, index: 2 },
    { id: 4, kind: `question`, questionId: `a#submit`, askId: `a` },
  ]

  it(`retires exactly the card with the matching id`, () => {
    const next = applyQuestionResolved(ask(), {
      id: `a#0`,
      answers: [`Red`],
    })!
    expect(next[1]).toMatchObject({ resolved: true, answer: `Red` })
    expect(next[2].resolved).toBeUndefined()
  })

  it(`folds several answers of a multi-select into the one card`, () => {
    const next = applyQuestionResolved(ask(), {
      id: `a#0`,
      answers: [`Red`, `Blue`],
    })!
    expect(next[1].answer).toBe(`Red, Blue`)
  })

  it(`retires a whole ask, assigning answers positionally`, () => {
    const next = applyQuestionResolved(ask(), {
      askId: `a`,
      answers: [`Red`, `Tabs`],
    })!
    expect(next[1]).toMatchObject({ resolved: true, answer: `Red` })
    expect(next[2]).toMatchObject({ resolved: true, answer: `Tabs` })
    // The submit step consumes no answer of its own.
    expect(next[3]).toMatchObject({ resolved: true })
    expect(next[3].answer).toBeUndefined()
  })

  it(`a dismissal retires without answers`, () => {
    const next = applyQuestionResolved(ask(), {
      askId: `a`,
      dismissed: true,
    })!
    expect(next[1]).toMatchObject({ resolved: true, dismissed: true })
    expect(next[1].answer).toBeUndefined()
  })

  it(`with neither id nor askId retires every pending card`, () => {
    const feed: QuestionItem[] = [
      { id: 1, kind: `question`, planMode: true },
      { id: 2, kind: `question`, resolved: true, answer: `Red` },
      { id: 3, kind: `question` },
    ]
    const next = applyQuestionResolved(feed, {})!
    expect(next[0].resolved).toBe(true)
    expect(next[1].answer).toBe(`Red`)
    expect(next[2].resolved).toBe(true)
  })

  it(`is null when nothing matched — the feed is kept as-is`, () => {
    expect(applyQuestionResolved(ask(), { id: `other` })).toBeNull()
    expect(applyQuestionResolved([], { askId: `a` })).toBeNull()
  })
})

// ── Answer lock state machine ────────────────────────────────────────────────

describe(`answer locks`, () => {
  it(`keys on the wire id; the feed id only keeps the lookup total`, () => {
    expect(answerKey({ id: 4, questionId: `tu_1#0` })).toBe(`tu_1#0`)
    expect(answerKey({ id: 4 })).toBe(`#4`)
  })

  it(`locks on send, stays locked through the ack`, () => {
    let states: AnswerStates = {}
    expect(isAnswerLocked(states[`q`])).toBe(false)
    states = beginAnswer(states, `q`, [`1`], [`Red`])
    expect(states.q).toMatchObject({ status: `sending`, labels: [`Red`] })
    expect(isAnswerLocked(states.q)).toBe(true)
    states = ackAnswer(states, `q`)
    expect(states.q.status).toBe(`acked`)
    expect(isAnswerLocked(states.q)).toBe(true)
  })

  it(`a missing ack re-enables the card, an acked one never does`, () => {
    let states = beginAnswer({}, `q`, [`1`], [`Red`])
    states = failAnswer(states, `q`)
    expect(states.q.status).toBe(`error`)
    expect(isAnswerLocked(states.q)).toBe(false)
    // The retry's answer is kept for the label render.
    expect(states.q.labels).toEqual([`Red`])

    const acked = ackAnswer(beginAnswer({}, `q`, [`1`], [`Red`]), `q`)
    expect(failAnswer(acked, `q`).q.status).toBe(`acked`)
    expect(failAnswer({}, `nope`)).toEqual({})
  })

  it(`resolution clears the lock`, () => {
    const states = beginAnswer({}, `q`, [`1`], [`Red`])
    expect(clearAnswer(states, `q`)).toEqual({})
    expect(clearAnswer(states, `other`)).toBe(states)
  })

  it(`acking an unknown key is a no-op`, () => {
    const states: AnswerStates = {}
    expect(ackAnswer(states, `q`)).toBe(states)
  })
})

// ── Multi-question stepper ───────────────────────────────────────────────────

describe(`askStepperView`, () => {
  const step = (
    id: number,
    index: number,
    over: Partial<QuestionItem> = {}
  ): QuestionItem => ({
    id,
    kind: `question`,
    questionId: `a#${index - 1}`,
    askId: `a`,
    index,
    total: 2,
    text: `Q${index}`,
    ...over,
  })
  const submit = (id: number, over: Partial<QuestionItem> = {}): QuestionItem => ({
    id,
    kind: `question`,
    questionId: `a#submit`,
    askId: `a`,
    text: `Submit?`,
    ...over,
  })

  it(`walks one question at a time, in index order`, () => {
    const view = askStepperView([step(2, 2), step(1, 1)], {})
    expect(view.steps.map((s) => s.item.id)).toEqual([1, 2])
    expect(view.steps.map((s) => s.phase)).toEqual([`current`, `pending`])
    expect(view.position).toBe(1)
    expect(view.total).toBe(2)
    expect(view.submit).toBeNull()
    expect(view.waiting).toBe(false)
  })

  it(`an in-flight answer advances the stepper immediately`, () => {
    const items = [step(1, 1), step(2, 2)]
    const states = beginAnswer({}, `a#0`, [`1`], [`Red`])
    const view = askStepperView(items, states)
    expect(view.steps[0]).toMatchObject({ phase: `answered`, answer: `Red` })
    expect(view.steps[1].phase).toBe(`current`)
    expect(view.position).toBe(2)
  })

  it(`an acked answer keeps the step answered`, () => {
    const states = ackAnswer(beginAnswer({}, `a#0`, [`1`], [`Red`]), `a#0`)
    const view = askStepperView([step(1, 1), step(2, 2)], states)
    expect(view.steps[0].phase).toBe(`answered`)
  })

  it(`the resolved answer wins over the locally picked labels`, () => {
    const states = beginAnswer({}, `a#0`, [`1`], [`Red`])
    const view = askStepperView(
      [step(1, 1, { resolved: true, answer: `Crimson` }), step(2, 2)],
      states
    )
    expect(view.steps[0].answer).toBe(`Crimson`)
  })

  it(`the submit step becomes current once every question is answered`, () => {
    const states = beginAnswer(
      beginAnswer({}, `a#0`, [`1`], [`Red`]),
      `a#1`,
      [`2`],
      [`Tabs`]
    )
    const view = askStepperView([step(1, 1), step(2, 2), submit(3)], states)
    expect(view.steps.every((s) => s.phase === `answered`)).toBe(true)
    expect(view.submit).toMatchObject({ phase: `current` })
    expect(view.waiting).toBe(false)
  })

  it(`the submit step waits while a question is still open`, () => {
    const view = askStepperView([step(1, 1), step(2, 2), submit(3)], {})
    expect(view.submit?.phase).toBe(`pending`)
  })

  it(`waits for the next question when all published steps are answered`, () => {
    const states = beginAnswer({}, `a#0`, [`1`], [`Red`])
    const view = askStepperView([step(1, 1)], states)
    expect(view.steps[0].phase).toBe(`answered`)
    expect(view.waiting).toBe(true)
  })

  it(`a fully resolved ask waits for nothing`, () => {
    const view = askStepperView(
      [step(1, 1, { resolved: true, answer: `Red` })],
      {}
    )
    expect(view.waiting).toBe(false)
    expect(view.submit).toBeNull()
  })

  it(`a dismissed ask renders every step as answered`, () => {
    const view = askStepperView(
      [
        step(1, 1, { resolved: true, dismissed: true }),
        step(2, 2, { resolved: true, dismissed: true }),
      ],
      {}
    )
    expect(view.steps.map((s) => s.phase)).toEqual([`answered`, `answered`])
    expect(view.steps[0].answer).toBeUndefined()
  })

  it(`falls back to the published step count when total is absent`, () => {
    const view = askStepperView(
      [step(1, 1, { total: undefined }), step(2, 2, { total: undefined })],
      {}
    )
    expect(view.total).toBe(2)
  })
})

// ── Render rows ──────────────────────────────────────────────────────────────

describe(`groupFeedRows`, () => {
  const item = (id: number, kind: string, over: Record<string, unknown> = {}) =>
    ({ id, kind, ...over }) as {
      id: number
      kind: string
      askId?: string
      subagentId?: string
    }

  it(`collapses runs of >=2 consecutive tools, leaves everything else single`, () => {
    const feed = [
      item(1, `narration`),
      item(2, `tool`),
      item(3, `tool`),
      item(4, `tool`),
      item(5, `user_message`),
      item(6, `tool`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `toolRun`, id: 2, items: [feed[1], feed[2], feed[3]] },
      { kind: `single`, item: feed[4] },
      { kind: `single`, item: feed[5] },
    ])
  })

  // EXP-783: the transcript renders a WINDOW of the run, and a window that
  // cuts a group re-keys it onto the first item the reader can actually see.
  it(`a window restricts the projection without changing it`, () => {
    const feed = [
      item(1, `narration`),
      item(2, `tool`),
      item(3, `tool`),
      item(4, `tool`),
      item(5, `tool`),
    ]
    expect(groupFeedRows(feed, 0)).toEqual(groupFeedRows(feed))
    expect(groupFeedRows(feed, 3)).toEqual([
      { kind: `toolRun`, id: 4, items: [feed[3], feed[4]] },
    ])
    // Past the end is an empty projection, never a crash.
    expect(groupFeedRows(feed, 99)).toEqual([])
  })

  it(`a lone tool between other kinds stays a single row`, () => {
    const feed = [item(1, `tool`), item(2, `narration`), item(3, `tool`)]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `single`, item: feed[1] },
      { kind: `single`, item: feed[2] },
    ])
  })

  it(`two runs split by a narration stay separate runs`, () => {
    const feed = [
      item(1, `tool`),
      item(2, `tool`),
      item(3, `narration`),
      item(4, `tool`),
      item(5, `tool`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `toolRun`, id: 1, items: [feed[0], feed[1]] },
      { kind: `single`, item: feed[2] },
      { kind: `toolRun`, id: 4, items: [feed[3], feed[4]] },
    ])
  })

  it(`an all-tool feed is one run; an empty feed has no rows`, () => {
    const feed = [item(1, `tool`), item(2, `tool`), item(3, `tool`)]
    expect(groupFeedRows(feed)).toEqual([{ kind: `toolRun`, id: 1, items: feed }])
    expect(groupFeedRows([])).toEqual([])
  })

  it(`run id stays the FIRST tool's id as the trailing run grows`, () => {
    const feed = [item(1, `narration`), item(2, `tool`), item(3, `tool`)]
    const before = groupFeedRows(feed)
    const after = groupFeedRows([...feed, item(4, `tool`)])
    expect(before[1]).toMatchObject({ kind: `toolRun`, id: 2 })
    expect(after[1]).toMatchObject({ kind: `toolRun`, id: 2 })
    expect((after[1] as { items: unknown[] }).items).toHaveLength(3)
  })

  it(`questions adjacent to tools are never absorbed into a run`, () => {
    const feed = [
      item(1, `tool`),
      item(2, `tool`),
      item(3, `question`),
      item(4, `question`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `toolRun`, id: 1, items: [feed[0], feed[1]] },
      { kind: `single`, item: feed[2] },
      { kind: `single`, item: feed[3] },
    ])
  })

  it(`one ask's questions collapse into a single stepper row`, () => {
    const feed = [
      item(1, `question`, { askId: `a` }),
      item(2, `narration`),
      item(3, `question`, { askId: `a` }),
      item(4, `question`, { askId: `b` }),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `ask`, id: 1, askId: `a`, items: [feed[0], feed[2]] },
      { kind: `single`, item: feed[1] },
      { kind: `ask`, id: 4, askId: `b`, items: [feed[3]] },
    ])
  })

  it(`a question without an askId (plan approval) stays its own row`, () => {
    const feed = [item(1, `question`, { questionId: `tu_1` })]
    expect(groupFeedRows(feed)).toEqual([{ kind: `single`, item: feed[0] }])
  })

  it(`a subagent's events and its tool calls group under its id`, () => {
    const feed = [
      item(1, `subagent`, { subagentId: `s1` }),
      item(2, `tool`, { subagentId: `s1` }),
      item(3, `tool`, { subagentId: `s1` }),
      item(4, `subagent`, { subagentId: `s1` }),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `subagent`, id: 1, subagentId: `s1`, items: feed },
    ])
  })

  it(`two subagents keep separate groups and never absorb main-thread tools`, () => {
    const feed = [
      item(1, `tool`),
      item(2, `subagent`, { subagentId: `s1` }),
      item(3, `tool`, { subagentId: `s1` }),
      item(4, `subagent`, { subagentId: `s2` }),
      item(5, `tool`, { subagentId: `s2` }),
      item(6, `tool`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `subagent`, id: 2, subagentId: `s1`, items: [feed[1], feed[2]] },
      { kind: `subagent`, id: 4, subagentId: `s2`, items: [feed[3], feed[4]] },
      { kind: `single`, item: feed[5] },
    ])
  })

  // EXP-773: a subagent's PROSE and its user turns are scoped the same way —
  // hidden from Main, shown in that subagent's view.
  it(`a subagent's narration and user turns join its group, not the main feed`, () => {
    const feed = [
      item(1, `narration`),
      item(2, `subagent`, { subagentId: `s1` }),
      item(3, `narration`, { subagentId: `s1` }),
      item(4, `user_message`, { subagentId: `s1` }),
      item(5, `tool`, { subagentId: `s1` }),
      item(6, `user_message`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      {
        kind: `subagent`,
        id: 2,
        subagentId: `s1`,
        items: [feed[1], feed[2], feed[3], feed[4]],
      },
      { kind: `single`, item: feed[5] },
    ])
    // The accessor every renderer uses agrees.
    expect(feed.map(subagentIdOf)).toEqual([
      null,
      `s1`,
      `s1`,
      `s1`,
      `s1`,
      null,
    ])
    // A kind that is NOT scopable keeps its main-feed row even with an id.
    expect(subagentIdOf({ kind: `compaction`, subagentId: `s1` })).toBeNull()
  })

  it(`a subagent tool breaks a main-thread run instead of joining it`, () => {
    const feed = [
      item(1, `tool`),
      item(2, `tool`, { subagentId: `s1` }),
      item(3, `tool`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `subagent`, id: 2, subagentId: `s1`, items: [feed[1]] },
      { kind: `single`, item: feed[2] },
    ])
  })

  it(`permission rows stay single`, () => {
    const feed = [item(1, `permission`), item(2, `permission`)]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `single`, item: feed[1] },
    ])
  })
})

describe(`summarizeSubagentRow`, () => {
  const marker = (over: Record<string, unknown> = {}) => ({
    kind: `subagent`,
    ...over,
  })
  const tool = () => ({ kind: `tool` })

  it(`an old desktop's "agent" completed edge never degrades the label`, () => {
    const row = summarizeSubagentRow([
      marker({ agentType: `explore`, status: `started`, detail: `Map the crate` }),
      tool(),
      marker({ agentType: `agent`, status: `completed` }),
    ])
    expect(row).toEqual({
      agentType: `explore`,
      done: true,
      detail: `Map the crate`,
      toolCount: 1,
    })
  })

  it(`a completed-only marker with a real type keeps it`, () => {
    const row = summarizeSubagentRow([
      marker({ agentType: `review`, status: `completed` }),
    ])
    expect(row).toMatchObject({ agentType: `review`, done: true, toolCount: 0 })
  })

  it(`an honest "agent"-only group still reads "agent"`, () => {
    const row = summarizeSubagentRow([
      marker({ agentType: `agent`, status: `completed` }),
    ])
    expect(row).toMatchObject({ agentType: `agent`, done: true })
  })

  it(`a tools-only orphan group falls back and counts its tools`, () => {
    const row = summarizeSubagentRow([tool(), tool()])
    expect(row).toEqual({
      agentType: `agent`,
      done: false,
      detail: undefined,
      toolCount: 2,
    })
  })

  // EXP-748: the completed edge carries the publisher's own tool-call count.
  // A replay buffer evicts subagent tool rows first, so the rows still in the
  // feed undercount — the reported number wins.
  it(`a completed marker's toolCalls beats the visible tool rows`, () => {
    const row = summarizeSubagentRow([
      marker({ agentType: `explore`, status: `started` }),
      tool(),
      marker({ agentType: `explore`, status: `completed`, toolCalls: 12 }),
    ])
    expect(row.toolCount).toBe(12)
  })

  it(`the visible rows win when the publisher reports none or fewer`, () => {
    expect(
      summarizeSubagentRow([
        marker({ agentType: `explore`, status: `completed` }),
        tool(),
        tool(),
      ]).toolCount
    ).toBe(2)
    expect(
      summarizeSubagentRow([
        tool(),
        tool(),
        marker({ agentType: `explore`, status: `completed`, toolCalls: 1 }),
      ]).toolCount
    ).toBe(2)
  })

  it(`the LATEST non-empty detail wins (the completed edge restates it)`, () => {
    const row = summarizeSubagentRow([
      marker({ agentType: `explore`, status: `started`, detail: `Old` }),
      marker({ agentType: `explore`, status: `completed`, detail: `Fresh` }),
    ])
    expect(row.detail).toBe(`Fresh`)
  })
})

describe(`collectSubagents`, () => {
  it(`one summary per subagent id, in first-appearance order (EXP-356)`, () => {
    const feed = [
      { kind: `narration`, text: `Delegating.` },
      {
        kind: `subagent`,
        subagentId: `toolu_a`,
        agentType: `Explore`,
        status: `started`,
        detail: `Map the crate`,
      },
      { kind: `tool`, subagentId: `toolu_a` },
      { kind: `tool` }, // main-agent tool — never a tab
      {
        kind: `subagent`,
        subagentId: `toolu_b`,
        agentType: `review`,
        status: `started`,
      },
      { kind: `tool`, subagentId: `toolu_a` },
      {
        kind: `subagent`,
        subagentId: `toolu_a`,
        agentType: `Explore`,
        status: `completed`,
        detail: `Map the crate`,
      },
    ]
    expect(collectSubagents(feed)).toEqual([
      {
        subagentId: `toolu_a`,
        agentType: `Explore`,
        done: true,
        detail: `Map the crate`,
        toolCount: 2,
      },
      {
        subagentId: `toolu_b`,
        agentType: `review`,
        done: false,
        detail: undefined,
        toolCount: 0,
      },
    ])
  })

  it(`an empty or subagent-free feed yields no tabs`, () => {
    expect(collectSubagents([])).toEqual([])
    expect(
      collectSubagents([{ kind: `tool` }, { kind: `narration` }])
    ).toEqual([])
  })
})

describe(`visibleSubagentTabs`, () => {
  const run = (subagentId: string, done: boolean) => ({
    subagentId,
    agentType: `Explore`,
    done,
    detail: undefined,
    toolCount: 0,
  })

  it(`drops completed runs and keeps running ones (EXP-387)`, () => {
    const agents = [run(`toolu_a`, true), run(`toolu_b`, false)]
    expect(visibleSubagentTabs(agents, null)).toEqual([run(`toolu_b`, false)])
  })

  it(`the focused tab survives its own completion until deselected`, () => {
    const agents = [run(`toolu_a`, true), run(`toolu_b`, false)]
    expect(visibleSubagentTabs(agents, `toolu_a`)).toEqual(agents)
    expect(visibleSubagentTabs(agents, `toolu_b`)).toEqual([
      run(`toolu_b`, false),
    ])
  })

  it(`all done and Main selected leaves the strip empty`, () => {
    expect(
      visibleSubagentTabs([run(`toolu_a`, true), run(`toolu_b`, true)], null)
    ).toEqual([])
  })
})

describe(`looksLikeMarkdown`, () => {
  it(`leaves plain prose on the plain path`, () => {
    expect(looksLikeMarkdown(`Reading the file to find the handler.`)).toBe(
      false
    )
    expect(looksLikeMarkdown(`Done — 3 tests pass, 0 fail.`)).toBe(false)
    expect(looksLikeMarkdown(``)).toBe(false)
  })

  it(`leaves a bare URL on the plain path (linkSegments already links it)`, () => {
    expect(
      looksLikeMarkdown(
        `Open https://claude.ai/oauth/authorize?code=a_b-c#frag to sign in.`
      )
    ).toBe(false)
    expect(looksLikeMarkdown(`https://example.dev/a_long_path/v2`)).toBe(false)
  })

  it(`detects emphasis, code spans and headings`, () => {
    expect(looksLikeMarkdown(`I updated the **status** column.`)).toBe(true)
    expect(looksLikeMarkdown(`That is *definitely* the bug.`)).toBe(true)
    expect(looksLikeMarkdown(`Dropped the ~~old~~ path.`)).toBe(true)
    expect(looksLikeMarkdown(`Call \`resolveTeamAccess\` instead.`)).toBe(true)
    expect(looksLikeMarkdown(`## What changed\n\nQuite a lot.`)).toBe(true)
    expect(looksLikeMarkdown(`> quoting the spec here`)).toBe(true)
  })

  it(`detects lists, fences and tables`, () => {
    expect(looksLikeMarkdown(`Plan:\n- read the shape\n- fix the filter`)).toBe(
      true
    )
    expect(looksLikeMarkdown(`Steps:\n1. install\n2. migrate`)).toBe(true)
    expect(looksLikeMarkdown(`Run:\n\`\`\`bash\nbun run migrate\n\`\`\``)).toBe(
      true
    )
    expect(looksLikeMarkdown(`| col | col |\n| --- | --- |`)).toBe(true)
  })

  it(`detects links and images — the point of EXP-440`, () => {
    expect(looksLikeMarkdown(`![screenshot](/api/attachments/abc)`)).toBe(true)
    expect(
      looksLikeMarkdown(`See [the runbook](https://example.dev/run).`)
    ).toBe(true)
  })
})

// ── Frame coalescing (REV-33) ────────────────────────────────────────────────

describe(`createActivityCoalescer`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it(`applies a burst as ONE batch in arrival order`, () => {
    const batches: number[][] = []
    const queue = createActivityCoalescer<number>((b) => batches.push(b), 50)
    for (let i = 0; i < 2000; i++) queue.enqueue(i)
    // Nothing flushes synchronously — that is the whole point.
    expect(batches).toHaveLength(0)
    vi.advanceTimersByTime(50)
    expect(batches).toHaveLength(1)
    expect(batches[0]).toHaveLength(2000)
    expect(batches[0][0]).toBe(0)
    expect(batches[0][1999]).toBe(1999)
  })

  it(`throttles from the FIRST op — a continuous stream flushes every window`, () => {
    const batches: string[][] = []
    const queue = createActivityCoalescer<string>((b) => batches.push(b), 50)
    queue.enqueue(`a`)
    vi.advanceTimersByTime(40)
    // A late arrival must not push the deadline out (debounce would starve).
    queue.enqueue(`b`)
    vi.advanceTimersByTime(10)
    expect(batches).toEqual([[`a`, `b`]])
    queue.enqueue(`c`)
    vi.advanceTimersByTime(50)
    expect(batches).toEqual([[`a`, `b`], [`c`]])
  })

  it(`an empty window applies nothing`, () => {
    const batches: number[][] = []
    createActivityCoalescer<number>((b) => batches.push(b), 50)
    vi.advanceTimersByTime(500)
    expect(batches).toHaveLength(0)
  })

  it(`cancel drops the buffered ops and the timer`, () => {
    const batches: number[][] = []
    const queue = createActivityCoalescer<number>((b) => batches.push(b), 50)
    queue.enqueue(1)
    queue.cancel()
    vi.advanceTimersByTime(500)
    expect(batches).toHaveLength(0)
    // The queue stays usable after a cancel (redial within the same effect).
    queue.enqueue(2)
    vi.advanceTimersByTime(50)
    expect(batches).toEqual([[2]])
  })
})

// EXP-724 — the compaction strip's pure half.
describe(`compaction`, () => {
  it(`only the agent WORKING again resumes a stuck strip`, () => {
    for (const kind of [`narration`, `tool`, `question`, `subagent`]) {
      expect(resumesAfterCompaction(kind)).toBe(true)
    }
    for (const kind of [
      `user_message`,
      `diff`,
      `answer_ack`,
      `permission`,
      `question_resolved`,
      `compaction`,
    ]) {
      expect(resumesAfterCompaction(kind)).toBe(false)
    }
  })

  it(`pins the backstop at the desktop's COMPACTION_TIMEOUT (180s)`, () => {
    expect(COMPACTION_TIMEOUT_MS).toBe(180_000)
  })

  it(`a compaction marker is its own single row, never folded into a run`, () => {
    const item = (id: number, kind: string) => ({ id, kind })
    const feed = [
      item(1, `tool`),
      item(2, `compaction`),
      item(3, `tool`),
      item(4, `tool`),
    ]
    expect(groupFeedRows(feed)).toEqual([
      { kind: `single`, item: feed[0] },
      { kind: `single`, item: feed[1] },
      { kind: `toolRun`, id: 3, items: [feed[2], feed[3]] },
    ])
  })
})

// EXP-746: the ACP engine's latest-wins configuration + context meter.
describe(`config state`, () => {
  const state = (over: Record<string, unknown> = {}) => ({
    kind: `config_state`,
    modes: [
      { id: `plan`, label: `Plan` },
      { id: `bypassPermissions`, label: `Build` },
    ],
    currentMode: `bypassPermissions`,
    ...over,
  })

  // EXP-772: options are gone from the state entirely — an older publisher
  // still sends them, the fold simply drops them.
  it(`parseConfigState drops malformed modes, commands and every option`, () => {
    const parsed = parseConfigState(
      state({
        options: [{ id: `model`, label: `Model`, value: `opus` }],
        modes: [{ id: `plan`, label: `Plan`, description: `Read-only` }, 7],
        commands: [{ name: `review`, description: `Review the diff` }, null],
        currentMode: `plan`,
      })
    )
    expect(parsed).toEqual({
      modes: [{ id: `plan`, label: `Plan`, description: `Read-only` }],
      commands: [{ name: `review`, description: `Review the diff` }],
      currentMode: `plan`,
    })
    // An options-only payload from an older publisher is still a config
    // state — it just carries no modes and no commands.
    expect(
      parseConfigState({ kind: `config_state`, options: [] })
    ).toEqual({ modes: [], commands: [] })
    // Not a config state at all — the caller keeps its previous snapshot.
    expect(parseConfigState({ kind: `config_state` })).toBeNull()
    expect(parseConfigState(null)).toBeNull()
    expect(parseConfigState([])).toBeNull()
  })

  it(`parseSessionUsage refuses a zero context size`, () => {
    expect(
      parseSessionUsage({
        kind: `usage`,
        contextUsed: 124_000,
        contextSize: 200_000,
        costUsd: 1.235,
      })
    ).toEqual({ contextUsed: 124_000, contextSize: 200_000, costUsd: 1.235 })
    // A zero window is the engine saying "unknown", not "empty".
    expect(
      parseSessionUsage({ kind: `usage`, contextUsed: 0, contextSize: 0 })
    ).toBeNull()
    expect(
      parseSessionUsage({ kind: `usage`, contextUsed: `lots`, contextSize: 10 })
    ).toBeNull()
    // A negative cost is dropped, the counts still stand.
    expect(
      parseSessionUsage({
        kind: `usage`,
        contextUsed: 10,
        contextSize: 20,
        costUsd: -1,
      })
    ).toEqual({ contextUsed: 10, contextSize: 20 })
  })

  // EXP-772: this name is mirrored ×4 - Android carries it verbatim, iOS as
  // `testModeChipIsTheOnlyComposerControl`, desktop as
  // `mode_chip_is_the_only_composer_control`; keep the four in step.
  it(`the mode chip is the only composer control`, () => {
    const config = parseConfigState(
      state({
        // An older publisher's options are ignored, not rendered.
        options: [{ id: `model`, label: `Model`, value: `opus` }],
        modes: [
          { id: `default`, label: `Default` },
          { id: `plan`, label: `Plan` },
        ],
        currentMode: `plan`,
      })
    )
    const chip = modeChip(config)
    expect([chip?.id, chip?.valueLabel]).toEqual([`mode`, `Plan`])
    expect(chip?.values).toEqual([
      { id: `default`, label: `Default` },
      { id: `plan`, label: `Plan` },
    ])
    // No modes on this run → no chip at all (codex advertises none).
    expect(modeChip(parseConfigState(state({ modes: [] })))).toBeNull()
    expect(modeChip(null)).toBeNull()
  })

  it(`an unknown current mode still reads something`, () => {
    const config = parseConfigState(
      state({ modes: [{ id: `plan`, label: `Plan` }], currentMode: `weird` })
    )
    expect(modeChip(config)?.valueLabel).toBe(`weird`)
    const blank = parseConfigState(
      state({ modes: [{ id: `plan`, label: `Plan` }], currentMode: undefined })
    )
    expect(modeChip(blank)?.valueLabel).toBe(CONFIG_DEFAULT_VALUE_LABEL)
  })

  // EXP-772: the claude/pi pair draws a Plan SWITCH; anything else keeps the
  // two-value chip.
  it(`planModeToggle recognizes the plan + one other pair`, () => {
    expect(planModeToggle(parseConfigState(state()))).toEqual({
      planId: `plan`,
      buildId: `bypassPermissions`,
      active: false,
    })
    expect(
      planModeToggle(parseConfigState(state({ currentMode: `plan` })))?.active
    ).toBe(true)
    // Three modes, or two without a plan, are not the pair.
    expect(
      planModeToggle(
        parseConfigState(
          state({
            modes: [
              { id: `plan`, label: `Plan` },
              { id: `default`, label: `Default` },
              { id: `acceptEdits`, label: `Accept edits` },
            ],
          })
        )
      )
    ).toBeNull()
    expect(
      planModeToggle(
        parseConfigState(
          state({
            modes: [
              { id: `default`, label: `Default` },
              { id: `acceptEdits`, label: `Accept edits` },
            ],
          })
        )
      )
    ).toBeNull()
    expect(planModeToggle(null)).toBeNull()
  })
})

// EXP-772: the ACP coalescer flushes one assistant message as several
// narration events keyed by `messageId`.
// EXP-784/785/786: the shared wire additions' pure folds.
describe(`tool kinds, rate limit and diff bytes (EXP-784/785/786)`, () => {
  it(`TOOL_KINDS is the contract's toolKind list, byte-equal`, () => {
    expect([...TOOL_KINDS]).toEqual([...contract.toolKind.values])
    expect(parseToolKind(`switch_mode`)).toBe(`switch_mode`)
    expect(parseToolKind(`teleport`)).toBeUndefined()
    expect(parseToolKind(undefined)).toBeUndefined()
  })

  it(`parseRateLimit keeps a limited window and clears on ok/empty/junk`, () => {
    expect(
      parseRateLimit({
        kind: `rate_limit`,
        status: ` allowed_warning `,
        resetsAt: 1_700_000_000_000,
        message: ` 80% used `,
      })
    ).toEqual({
      status: `allowed_warning`,
      resetsAt: 1_700_000_000_000,
      message: `80% used`,
    })
    expect(parseRateLimit({ kind: `rate_limit`, status: `rejected`, resetsAt: -1 })).toEqual({
      status: `rejected`,
    })
    expect(parseRateLimit({ kind: `rate_limit`, status: `ok` })).toBeNull()
    expect(parseRateLimit({ kind: `rate_limit`, status: `` })).toBeNull()
    expect(parseRateLimit({ kind: `rate_limit` })).toBeNull()
    expect(parseRateLimit(null)).toBeNull()
    expect(rateLimitClears(` OK `)).toBe(true)
    expect(rateLimitClears(`allowed`)).toBe(false)
  })

  it(`a folded diff weighs against the byte budget`, () => {
    const base = feedItemBytes({ kind: `tool`, name: `Edit`, detail: `a.ts` })
    expect(feedItemBytes({ kind: `tool`, name: `Edit`, detail: `a.ts`, diff: `+abc\n` })).toBe(
      base + 5
    )
  })
})

describe(`narration fragments`, () => {
  const row = (over: Record<string, unknown>) => ({
    id: 1,
    kind: `narration`,
    text: `Looking`,
    ...over,
  })

  it(`appends onto the previous row with the same messageId`, () => {
    const feed = [row({ messageId: `m1` })]
    expect(
      mergeNarrationFragment(feed, { messageId: `m1`, text: ` at the code.` })
    ).toEqual([{ id: 1, kind: `narration`, text: `Looking at the code.`, messageId: `m1` }])
  })

  it(`never merges across a different message, a gap or a missing id`, () => {
    const feed = [row({ messageId: `m1` })]
    // Another message id.
    expect(
      mergeNarrationFragment(feed, { messageId: `m2`, text: `x` })
    ).toBeNull()
    // No id at all (an older publisher) — every event stays its own row.
    expect(mergeNarrationFragment(feed, { text: `x` })).toBeNull()
    // A row in between: the last row is not the narration any more.
    expect(
      mergeNarrationFragment(
        [...feed, { id: 2, kind: `tool`, text: `` }],
        { messageId: `m1`, text: `x` }
      )
    ).toBeNull()
    // An empty feed.
    expect(mergeNarrationFragment([], { messageId: `m1`, text: `x` })).toBeNull()
  })

  it(`keeps a subagent's fragments out of the main bubble`, () => {
    const feed = [row({ messageId: `m1` })]
    expect(
      mergeNarrationFragment(feed, {
        messageId: `m1`,
        text: `x`,
        subagentId: `sub-1`,
      })
    ).toBeNull()
  })
})

// EXP-787: the gap ladder — the space ABOVE a row, chosen from the row before
// it. The same nine pairings are locked on desktop, iOS and Android; the
// numbers come from tokens.json `transcript`, so this asserts the RULE.
describe(`transcriptGap ladder`, () => {
  const {
    gapTurn: TURN,
    gapBlock: BLOCK,
    gapTool: TOOL,
    gapDefault: DEFAULT_GAP,
  } = designTokens.transcript

  const single = (kind: string) =>
    ({ kind: `single`, item: { id: 1, kind } }) as FeedRow<{
      id: number
      kind: string
    }>

  it(`classifies a sent user message as a turn`, () => {
    expect(rowClass(single(`user_message`))).toBe(`turn`)
  })

  it(`classifies prose rows`, () => {
    expect(rowClass(single(`narration`))).toBe(`prose`)
    expect(rowClass(single(`question`))).toBe(`prose`)
    expect(rowClass(single(`compaction`))).toBe(`prose`)
    expect(
      rowClass({ kind: `ask`, id: 1, askId: `a1`, items: [] })
    ).toBe(`prose`)
  })

  it(`classifies tool rows`, () => {
    expect(rowClass(single(`tool`))).toBe(`tool`)
    expect(rowClass(single(`permission`))).toBe(`tool`)
    expect(rowClass(single(`subagent`))).toBe(`tool`)
    expect(rowClass({ kind: `toolRun`, id: 1, items: [] })).toBe(`tool`)
    expect(
      rowClass({ kind: `subagent`, id: 1, subagentId: `s1`, items: [] })
    ).toBe(`tool`)
  })

  it(`gives the first rendered row no gap at all`, () => {
    expect(transcriptGap(null, `turn`)).toBe(0)
    expect(transcriptGap(null, `prose`)).toBe(0)
    expect(transcriptGap(null, `tool`)).toBe(0)
  })

  it(`opens and closes a user turn with the turn gap`, () => {
    expect(transcriptGap(`turn`, `turn`)).toBe(TURN)
    expect(transcriptGap(`turn`, `prose`)).toBe(TURN)
    expect(transcriptGap(`turn`, `tool`)).toBe(TURN)
    expect(transcriptGap(`prose`, `turn`)).toBe(TURN)
    expect(transcriptGap(`tool`, `turn`)).toBe(TURN)
  })

  it(`packs consecutive tool rows tightest`, () => {
    expect(transcriptGap(`tool`, `tool`)).toBe(DEFAULT_GAP)
  })

  it(`uses the tool gap where prose meets a tool row`, () => {
    expect(transcriptGap(`prose`, `tool`)).toBe(TOOL)
    expect(transcriptGap(`tool`, `prose`)).toBe(TOOL)
  })

  it(`separates two prose rows with the block gap`, () => {
    expect(transcriptGap(`prose`, `prose`)).toBe(BLOCK)
  })

  it(`matches the shared tokens`, () => {
    expect([TURN, BLOCK, TOOL, DEFAULT_GAP]).toEqual([16, 12, 12, 8])
  })
})

// EXP-788: the composer answers the pending card — which card, and what the
// typed reply sends. The strings are the ×4 copy contract.
describe(`pending card routing (EXP-788)`, () => {
  // `kind` must stay a LITERAL on both card shapes: `pendingAnswerable`
  // narrows its return with `Extract<T, { kind: \`question\` }>`, which
  // collapses to `never` the moment the feed's element type widens `kind` to
  // `string` (a bare object literal does exactly that).
  type QuestionCard = AnswerableCard & { kind: `question` }
  type NarrationCard = { id: number; kind: `narration`; text: string }
  // `over` stays a loose overlay because the fixtures deliberately carry
  // wire fields `AnswerableOption` does not declare (a plan option's
  // `description`); the assertion below is what re-pins `kind`.
  const question = (
    id: number,
    over: Record<string, unknown> = {}
  ): QuestionCard =>
    ({
      id,
      kind: `question`,
      text: `Which?`,
      options: [
        { key: `1`, label: `Refactor` },
        { key: `2`, label: `Rewrite` },
      ],
      multiSelect: false,
      planMode: false,
      questionId: `q-${id}`,
      ...over,
    }) as QuestionCard
  const plan = (id: number): QuestionCard =>
    question(id, {
      planMode: true,
      options: [
        { key: `exit-plan-bypass`, label: `Yes` },
        { key: `exit-plan-clear-bypass`, label: `Yes, and start with a fresh context` },
        {
          key: `reject`,
          label: `No, keep planning`,
          description: `Sends your next message back to planning`,
        },
      ],
    })
  const active = (...ids: number[]) => new Set(ids)

  it(`picks the newest active card and skips one whose answer is in flight`, () => {
    const feed: (QuestionCard | NarrationCard)[] = [
      { id: 0, kind: `narration`, text: `hi` },
      question(1),
      plan(2),
    ]
    expect(pendingAnswerable(feed, active(1, 2), {})?.id).toBe(2)
    const locked: AnswerStates = {
      [`q-2`]: { keys: [`reject`], labels: [`No`], status: `sending` },
    }
    expect(pendingAnswerable(feed, active(1, 2), locked)?.id).toBe(1)
    expect(pendingAnswerable(feed, active(), {})).toBeNull()
    // A resolved card is never active (activeQuestionIds already drops it);
    // an inactive one is never pending even when unlocked.
    expect(pendingAnswerable(feed, active(2), {})?.id).toBe(2)
  })

  it(`a plan card rejects and forwards the text as the next message`, () => {
    expect(freeAnswerFor(plan(2), `  drop the cache layer  `)).toEqual({
      keys: [`reject`],
      labels: [`No, keep planning`],
      followUp: `drop the cache layer`,
    })
    // A plan card with one option has no reject to pick.
    expect(
      freeAnswerFor(
        plan(2) as ReturnType<typeof plan> & { options: unknown[] },
        ``
      )
    ).toBeNull()
    expect(
      freeAnswerFor(question(3, { planMode: true, options: [{ key: `y`, label: `Yes` }] }), `x`)
    ).toBeNull()
  })

  it(`a question rides its free-text row, else the mapper's text key`, () => {
    expect(freeAnswerFor(question(1), `purple`)).toEqual({
      keys: [FREE_TEXT_KEY],
      labels: [`purple`],
      text: `purple`,
    })
    const withRow = question(1, {
      options: [
        { key: `1`, label: `Red` },
        { key: `text`, label: `Type something.`, freeText: true },
      ],
    })
    expect(freeAnswerFor(withRow, `purple`)).toEqual({
      keys: [`text`],
      labels: [`purple`],
      text: `purple`,
    })
    expect(freeAnswerFor(question(1), `   `)).toBeNull()
  })

  it(`number chips and hotkeys cover 1-9 only`, () => {
    expect(optionHotkey(0)).toBe(`1`)
    expect(optionHotkey(8)).toBe(`9`)
    expect(optionHotkey(9)).toBeNull()
    const options = Array.from({ length: 12 }, (_, i) => ({ key: `k${i}` }))
    expect(optionForHotkey(options, `1`)?.key).toBe(`k0`)
    expect(optionForHotkey(options, `9`)?.key).toBe(`k8`)
    expect(optionForHotkey(options, `0`)).toBeNull()
    expect(optionForHotkey(options.slice(0, 2), `3`)).toBeNull()
    expect(optionForHotkey(options, `a`)).toBeNull()
    expect(optionForHotkey(options, `Enter`)).toBeNull()
  })

  it(`the placeholder names the card kind`, () => {
    expect(pendingPlaceholder(plan(1))).toBe(PLAN_PENDING_PLACEHOLDER)
    expect(pendingPlaceholder(question(1))).toBe(QUESTION_PENDING_PLACEHOLDER)
    expect(PLAN_PENDING_PLACEHOLDER).toBe(
      `Tell the agent what to change, or pick an option above`
    )
    expect(QUESTION_PENDING_PLACEHOLDER).toBe(
      `Answer directly, or pick an option above`
    )
  })
})

// EXP-785: the collapsed group's caption is the contract summary over the
// rows' kinds; a kind-less row (pre-EXP-785 publisher) counts as `other`.
describe(`tool group caption (EXP-785)`, () => {
  it(`summarises kinds, dedupes edited paths and lists failures last`, () => {
    expect(
      toolGroupCaption([
        { toolKind: `execute` },
        { toolKind: `execute`, failed: true },
        { toolKind: `edit`, detail: `src/a.ts` },
        { toolKind: `edit`, detail: `src/a.ts` },
        { toolKind: `edit`, detail: `src/b.ts` },
        { toolKind: `read`, detail: `src/c.ts` },
      ])
    ).toBe(`Ran 2 commands · edited 2 files · read 1 file · 1 failed`)
  })

  it(`counts kind-less rows as other tools`, () => {
    expect(toolGroupCaption([{}, {}, { toolKind: `think` }])).toBe(`Used 3 tools`)
    expect(toolGroupCaption([])).toBe(`No tool calls`)
  })
})

// EXP-786: the publisher's cut note is a footer, never a diff line.
describe(`per-call diff truncation (EXP-786)`, () => {
  const diff = `diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b`

  it(`splits the trailing truncation line off`, () => {
    expect(splitTruncatedDiff(`${diff}\n\\ 120 more lines truncated`)).toEqual({
      diff,
      truncated: 120,
    })
    expect(splitTruncatedDiff(`${diff}\n\\ 1 more line truncated\n`)).toEqual({
      diff,
      truncated: 1,
    })
  })

  it(`leaves an uncut diff alone, "no newline" markers included`, () => {
    const eof = `${diff}\n\\ No newline at end of file`
    expect(splitTruncatedDiff(eof)).toEqual({ diff: eof, truncated: null })
  })

  it(`words the footer`, () => {
    expect(diffTruncationNote(1)).toBe(`1 more line truncated`)
    expect(diffTruncationNote(120)).toBe(`120 more lines truncated`)
  })
})

// EXP-784: the banner strings.
describe(`rate-limit banner (EXP-784)`, () => {
  const clock = (ms: number) => `T${ms}`

  it(`prefers the agent's message and names the local reset time`, () => {
    expect(
      rateLimitBanner(
        { status: `rejected`, message: `5-hour limit reached`, resetsAt: 1_700_000_000_000 },
        clock
      )
    ).toEqual({ text: `5-hour limit reached`, resets: `resets T1700000000000` })
  })

  it(`falls back on the status and omits the reset when unknown`, () => {
    expect(rateLimitBanner({ status: `rejected` }, clock)).toEqual({
      text: `Rate limit reached`,
      resets: null,
    })
    expect(rateLimitBanner({ status: `allowed_warning` }, clock)).toEqual({
      text: `Approaching the rate limit`,
      resets: null,
    })
  })

  it(`scales a seconds-valued resetsAt up to ms`, () => {
    expect(rateLimitResetsAtMs(1_700_000_000)).toBe(1_700_000_000_000)
    expect(rateLimitResetsAtMs(1_700_000_000_000)).toBe(1_700_000_000_000)
    expect(rateLimitBanner({ status: `rejected`, resetsAt: 1_700_000_000 }, clock).resets).toBe(
      `resets T1700000000000`
    )
  })

  it(`formats HH:MM in local time by default`, () => {
    const at = new Date(2026, 8, 9, 7, 5).getTime()
    expect(rateLimitBanner({ status: `rejected`, resetsAt: at }).resets).toBe(`resets 07:05`)
  })
})
