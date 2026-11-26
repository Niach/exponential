import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/step-up.json" with { type: "json" }
import { STEP_UP_INITIAL, stepUpMethods, stepUpReduce, type StepUpEvent, type StepUpState } from "./step-up"

describe(`step-up.json`, () => {
  test(`initial state`, () => expect(STEP_UP_INITIAL).toEqual(fixture.initial as StepUpState))
  for (const c of fixture.methodCases) {
    test(`methods: ${c.name}`, () => expect(stepUpMethods(c.account, c.instance)).toEqual(c.methods as never))
  }
  for (const c of fixture.flows) {
    test(`flow: ${c.name}`, () => {
      const end = (c.events as StepUpEvent[]).reduce(stepUpReduce, STEP_UP_INITIAL)
      expect(end).toEqual(c.expect as StepUpState)
    })
  }
})
