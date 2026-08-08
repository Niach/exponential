import { describe, expect, it } from "vitest"
import {
  ackAnswer,
  activeQuestionIds,
  answerKey,
  applyQuestionResolved,
  askStepperView,
  attachQuestionAnswer,
  beginAnswer,
  clearAnswer,
  collectSubagents,
  consumeEcho,
  dismissPendingQuestions,
  failAnswer,
  groupFeedRows,
  hasSemanticQuestions,
  isAnswerLocked,
  looksLikeMarkdown,
  pushEcho,
  summarizeSubagentRow,
  upsertQuestion,
  visibleSubagentTabs,
  ECHO_CAP,
  ECHO_TTL_MS,
  PLAN_RESOLVED_NARRATION,
  type AnswerStates,
  type EchoEntry,
  type QuestionLike,
} from "./agent-feed"

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

// ── Legacy cards (no wire question id) ───────────────────────────────────────

describe(`activeQuestionIds — legacy cards`, () => {
  it(`returns the trailing consecutive question run`, () => {
    const feed = [
      { id: 1, kind: `narration` },
      { id: 2, kind: `question` },
      { id: 3, kind: `tool` },
      { id: 4, kind: `question` },
      { id: 5, kind: `question` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([4, 5]))
  })

  it(`is empty when the feed ends with a non-question`, () => {
    const feed = [
      { id: 1, kind: `question` },
      { id: 2, kind: `narration` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set())
  })

  it(`handles an all-question feed and an empty feed`, () => {
    expect(
      activeQuestionIds([
        { id: 1, kind: `question` },
        { id: 2, kind: `question` },
      ])
    ).toEqual(new Set([1, 2]))
    expect(activeQuestionIds([])).toEqual(new Set())
  })

  it(`is unaffected by tool runs preceding the trailing questions`, () => {
    const feed = [
      { id: 1, kind: `tool` },
      { id: 2, kind: `tool` },
      { id: 3, kind: `question` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([3]))
  })

  // EXP-174: plan questions publish from the live terminal grid at pending
  // time while the transcript tail lags — lagged flushes must not retire them.
  it(`keeps a plan question active behind lagged tool and narration flushes`, () => {
    const feed = [
      { id: 1, kind: `question`, planMode: true },
      { id: 2, kind: `tool` },
      { id: 3, kind: `narration`, text: `Let me finalize the plan file:` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([1]))
  })

  it(`retires a plan question on the resolution narration`, () => {
    const feed = [
      { id: 1, kind: `question`, planMode: true },
      { id: 2, kind: `tool` },
      { id: 3, kind: `narration`, text: PLAN_RESOLVED_NARRATION },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set())
  })

  // EXP-249: steering mid-plan leaves the picker up — a human message is no
  // resolution signal.
  it(`keeps a plan question active behind a human message`, () => {
    const feed = [
      { id: 1, kind: `question`, planMode: true },
      { id: 2, kind: `tool` },
      { id: 3, kind: `user_message`, text: `also handle the empty state` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([1]))
  })

  it(`retires a plan question when a newer question follows`, () => {
    const feed = [
      { id: 1, kind: `question`, planMode: true },
      { id: 2, kind: `tool` },
      { id: 3, kind: `question` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set([3]))
  })

  it(`still retires a non-plan question on any later event`, () => {
    const feed = [
      { id: 1, kind: `question` },
      { id: 2, kind: `tool` },
    ]
    expect(activeQuestionIds(feed)).toEqual(new Set())
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

describe(`hasSemanticQuestions`, () => {
  it(`is true only once a card carries a wire id`, () => {
    expect(hasSemanticQuestions([])).toBe(false)
    expect(
      hasSemanticQuestions([
        { kind: `question` },
        { kind: `narration`, questionId: `x` },
      ])
    ).toBe(false)
    expect(
      hasSemanticQuestions([{ kind: `question`, questionId: `tu_1` }])
    ).toBe(true)
  })
})

// EXP-197 legacy path: `Question answered:` narrations fold into the earliest
// unanswered card; resolved cards are never active.
type QuestionItem = QuestionLike

describe(`legacy narration resolution`, () => {
  it(`resolved question is never active and retires earlier plan cards`, () => {
    expect(
      activeQuestionIds([{ id: 1, kind: `question`, resolved: true }])
    ).toEqual(new Set())
    expect(
      activeQuestionIds([
        { id: 1, kind: `question`, planMode: true },
        { id: 2, kind: `question`, resolved: true },
      ])
    ).toEqual(new Set())
  })

  it(`answers attach earliest-first in question order`, () => {
    const feed: QuestionItem[] = [
      { id: 1, kind: `question` },
      { id: 2, kind: `question` },
    ]
    const first = attachQuestionAnswer(feed, `Red`)!
    expect(first[0]).toMatchObject({ resolved: true, answer: `Red` })
    expect(first[1].resolved).toBeUndefined()
    const second = attachQuestionAnswer(first, `Blue`)!
    expect(second[1]).toMatchObject({ resolved: true, answer: `Blue` })
  })

  it(`answers never attach to plan cards or already-answered cards`, () => {
    expect(
      attachQuestionAnswer([{ id: 1, kind: `question`, planMode: true }], `x`)
    ).toBeNull()
    expect(
      attachQuestionAnswer(
        [{ id: 1, kind: `question`, resolved: true, answer: `Red` }],
        `Blue`
      )
    ).toBeNull()
    expect(attachQuestionAnswer([], `x`)).toBeNull()
  })

  it(`dismissal retires every pending non-plan card`, () => {
    const feed: QuestionItem[] = [
      { id: 1, kind: `question` },
      { id: 2, kind: `question`, planMode: true },
      { id: 3, kind: `question` },
    ]
    const out = dismissPendingQuestions(feed)!
    expect(out[0].resolved).toBe(true)
    expect(out[1].resolved).toBeUndefined()
    expect(out[2].resolved).toBe(true)
    expect(dismissPendingQuestions(out)).toBeNull()
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
  it(`keys on the wire id, falling back to the feed id`, () => {
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

  it(`leaves the legacy magic narrations on the plain path`, () => {
    expect(looksLikeMarkdown(PLAN_RESOLVED_NARRATION)).toBe(false)
    expect(
      looksLikeMarkdown(`Question answered: Yes, go ahead and refactor it`)
    ).toBe(false)
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
