import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/ci-run-view.json"
import { replayAfter, replayCiRun, replayRunner } from "@/lib/ci-run-view-replay"

// #EXP-1804: the web reference IS what ci-run-view.json pins; desktop, iOS
// and Android replay the same file. A failure here after a deliberate change:
// `bun scripts/bless-ci-run-view.ts`, then review the fixture's diff.
const json = (value: unknown) => JSON.parse(JSON.stringify(value)) as unknown

describe(`ci-run-view fixture`, () => {
  it.each(fixture.ciRun.map((c) => [c.name, c] as const))(`run page: %s`, (_name, c) => {
    expect(json(replayCiRun(c))).toEqual(c.tree)
  })
  it.each(fixture.runner.map((c) => [c.name, c] as const))(`runner section: %s`, (_name, c) => {
    expect(json(replayRunner(c))).toEqual(c.tree)
  })
  it.each(fixture.after.map((c) => [`${c.event} ${JSON.stringify(c.payload)}`, c] as const))(`change: %s`, (_name, c) => {
    expect(json(replayAfter(c))).toEqual({ settings: c.settings, refused: c.refused })
  })
})
