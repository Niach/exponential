// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import rules from "../../eagent/fixtures/rules.json" with { type: "json" }
import fixture from "../fixtures/tasks.json" with { type: "json" }
import { newBurstState } from "./deletes"
import { secretFields } from "./elicitation"
import {
  cardAnswer,
  cardRequest,
  TASK_WATCHER_KIND,
  taskHandle,
  taskReduce,
  taskWatchFor,
  taskWire,
  type Card,
  type TaskCall,
  type TaskEvent,
  type TaskRequest,
  type TaskState,
} from "./tasks"

/** `{repeat, times}` = that string repeated; `{concat: [...]}` = the parts joined. */
function expand(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(expand)
  if (value && typeof value === `object`) {
    const v = value as Record<string, unknown>
    if (typeof v.repeat === `string` && typeof v.times === `number`) return v.repeat.repeat(v.times)
    if (Array.isArray(v.concat)) return v.concat.map(expand).join(``)
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, expand(x)]))
  }
  return value
}

describe(`one watch system (eagent rules.json watcher)`, () => {
  test(`a task is a watcher kind of session_watches`, () => {
    expect(rules.watcher.kinds).toContain(TASK_WATCHER_KIND)
    expect(rules.watcher.default).toBe(`session`)
  })
  test(`plan clipping and the stall copy are eagent's`, () => {
    expect(fixture.planMax).toBe(rules.ask.planMax)
    expect(fixture.failures.stalled).toBe(rules.report.reasons.stalled)
    expect(fixture.copy.noSummary).toBe(rules.report.fallback)
  })
})

describe(`taskWatchFor`, () => {
  for (const entry of fixture.eligibilityCases) test(entry.name, () => expect(taskWatchFor(entry.call as TaskCall)).toEqual(entry.expected as never))
})

describe(`taskReduce`, () => {
  for (const entry of fixture.reduceCases) {
    test(entry.name, () => {
      let task = entry.task as unknown as TaskState
      for (const step of entry.steps) {
        task = taskReduce(task, step.event as TaskEvent)
        expect(taskWire(task)).toEqual(step.expected as never)
      }
    })
  }
})

describe(`cards`, () => {
  for (const entry of fixture.cardCases) {
    test(entry.name, () => {
      const card = expand(entry.card) as Card
      const request = cardRequest(card)
      expect(request).toEqual(expand(entry.request) as never)
      if (request?.params.mode === `form`) expect(secretFields(request.params.requestedSchema)).toEqual([])
      for (const answer of entry.answers) expect(cardAnswer(`m1`, card, answer.result)).toEqual(answer.expected as never)
    })
  }
})

describe(`taskHandle`, () => {
  const tasks = new Map((fixture.handleGiven.tasks as unknown as TaskState[]).map((t) => [t.taskId, t]))
  for (const entry of fixture.handleCases) {
    test(entry.name, () => {
      const outcome = taskHandle(entry.request as TaskRequest, (id) => tasks.get(id) ?? null, newBurstState())
      const expected = entry.expected as { ok: boolean; status?: string }
      if (!outcome.ok) return expect(outcome).toEqual(expected as never)
      expect({ ok: true, result: outcome.result, effects: outcome.effects }).toEqual({ ok: true, result: (expected as never as { result: unknown }).result, effects: (expected as never as { effects: unknown }).effects } as never)
      if (expected.status) expect(outcome.task?.status).toBe(expected.status as never)
    })
  }

  for (const entry of fixture.rateCases) {
    test(`rate: ${entry.name}`, () => {
      const rate = newBurstState()
      let refused = 0
      for (let i = 0; i < entry.calls; i++) {
        const request: TaskRequest = { method: `tasks/get`, params: { taskId: `tsk_A` }, principal: `user:u1|client:c1`, declared: true, now: 1764158400000 + i * entry.everyMs }
        const outcome = taskHandle(request, (id) => tasks.get(id) ?? null, rate)
        if (!outcome.ok && outcome.error.code === fixture.errorCodes.rateLimited) refused++
      }
      expect(refused).toBe(entry.expectedRefused)
    })
  }
})
