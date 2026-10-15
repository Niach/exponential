// EXP-1321/1324: the fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/task-queue.json" with { type: "json" }
import { taskQueue, type TaskQueueInput } from "./task-queue"

describe(`taskQueue`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      expect(taskQueue(entry.input as TaskQueueInput)).toEqual(entry.expected as never)
    })
  }
})
