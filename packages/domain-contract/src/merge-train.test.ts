import { describe, expect, it } from "bun:test"
import contract from "../contract.json" with { type: "json" }
import fixture from "../fixtures/merge-train.json" with { type: "json" }
import icons from "../../icons/icons.json" with { type: "json" }
import repoConfig from "../../../.exponential/merge-train.json" with { type: "json" }
import {
  assistantBatchLines,
  composeBatch,
  conflictText,
  failingText,
  heldText,
  landingConfirmed,
  mergedText,
  mergeMenuEntries,
  mergeStep,
  mergeWait,
  prMergeRoute,
  resolveConflicts,
  statusPill,
  stuckRow,
  offlineSince,
  stuckText,
  trainRun,
  trainRef,
  trainChecksMode,
  trainVerdict,
  type PrMergeState,
  type TeamMergeMode,
} from "./merge-train"

// EXP-1707: the TS reference replays fixtures/merge-train.json.
describe(`merge-train fixture`, () => {
  it.each(fixture.readiness)(`readiness: $name`, ({ input, expected }) => {
    expect(mergeWait(input as never)).toBe(expected as never)
  })
  it.each(fixture.steps)(`step: $name`, ({ pr, event, expected }) => {
    expect(mergeStep(pr as never, event as never)).toEqual(expected as never)
  })
  it.each(fixture.landing)(`landing: $name`, ({ input, expected }) => {
    expect(landingConfirmed(input)).toBe(expected)
  })
  it.each(fixture.batches)(`batch: $name`, (c) => {
    const { units, expected } = c
    const batchMax = (c as { batchMax?: number }).batchMax
    expect(batchMax === undefined ? composeBatch(units as never) : composeBatch(units as never, batchMax)).toEqual(expected)
  })
  it.each(fixture.bisect)(`bisect: $name`, ({ input, expected }) => {
    expect(trainRun(input)).toEqual(expected)
  })
  it.each(fixture.resolver)(`resolver: $name`, ({ paths, expected }) => {
    expect(resolveConflicts(paths, fixture.repoPolicy.generated as never)).toEqual(expected as never)
  })
  it.each(fixture.pipeline.verdicts)(`pipeline verdict: $name`, ({ outcome, checks, required, expected }) => {
    expect(trainVerdict({ outcome: outcome as never, checks: checks as never, required })).toEqual(expected as never)
  })
  it.each(fixture.pipeline.checksModes)(`pipeline checks: $name`, ({ input, expected }) => {
    expect(trainChecksMode(input as never)).toBe(expected as never)
  })
  it(`pipeline: the ref and the example outcome replay`, () => {
    expect(trainRef(`t42`)).toBe(`exp-train/t42`)
    expect(fixture.pipeline.inputs).toEqual(Object.keys(fixture.pipeline.example.inputs))
  })
  it(`repoPolicy is this repo's .exponential/merge-train.json`, () => {
    expect(repoConfig).toEqual(fixture.repoPolicy as never)
  })
  it.each(fixture.texts)(`text: $kind`, ({ kind, input, expected }) => {
    const i = input as Record<string, never>
    const text =
      kind === `merged`
        ? mergedText(i.numbers, i.base, i.issues)
        : kind === `conflict`
          ? conflictText(i.number, i.base, i.files, i.attempts)
          : kind === `failing`
            ? failingText(i.number, i.failing, i.attempts)
            : kind === `held`
              ? heldText(i.number, i.reason)
              : stuckText(i.number, i.reason)
    expect(text).toBe(expected)
  })
  it.each(fixture.routing)(`routing: $name`, ({ input, expected }) => {
    expect(prMergeRoute(input as never)).toEqual(expected as never)
  })
  it.each(fixture.assistantBatch)(`assistant: $name`, ({ landed, base, expected }) => {
    expect(assistantBatchLines(landed, base)).toEqual(expected)
  })
  it.each(fixture.stuckRow)(`needs you: $name`, ({ input, expected }) => {
    expect(stuckRow(input as never)).toEqual(expected as never)
  })
  it.each(fixture.offlineSince)(`offline since: $name`, ({ input, expected }) => {
    expect(offlineSince(input as never)).toEqual(expected)
  })
  it.each(fixture.statusPill)(`pill: $name`, ({ input, expected }) => {
    expect(statusPill(input as never)).toEqual(expected as never)
  })
  it.each(fixture.menu)(`menu: $name`, ({ input, expected }) => {
    expect(mergeMenuEntries(input as never)).toEqual(expected as never)
  })
})

describe(`merge-train contract`, () => {
  it(`every step lands in a contract prMergeState`, () => {
    for (const c of fixture.steps) expect(contract.prMergeState.values).toContain(c.expected.pr.state)
  })
  it(`the TS unions match the contract enums`, () => {
    const states: PrMergeState[] = [`draft`, `checks`, `queued`, `testing`, `merged`, `conflict`, `failing`, `held`]
    const modes: TeamMergeMode[] = [`auto`, `approval`]
    expect(contract.prMergeState.values).toEqual(states)
    expect(contract.teamMergeMode.values).toEqual(modes)
  })
  it(`auto is the default merge mode`, () => {
    expect(contract.teamMergeMode.values[0]).toBe(`auto`)
  })
  it(`every pill glyph is an icon concept (or THE Spinner)`, () => {
    for (const c of fixture.statusPill) {
      if (c.expected.glyph !== `spinner`) expect(Object.keys(icons.semantic)).toContain(c.expected.glyph)
    }
  })
})
