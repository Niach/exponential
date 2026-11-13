import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import { aggregateChecksState, requiredChecksState } from "@exp/domain-contract/pr-checks"
import ecodeContract from "@exp/ecode/contract"
import checks from "@exp/ecode/fixtures/checks.json"
import forge from "@exp/ecode/fixtures/forge.json"
import pipeline from "@exp/ecode/fixtures/pipeline.json"
import remote from "@exp/ecode/fixtures/remote.json"
import runner from "@exp/ecode/fixtures/runner.json"
import { REPLAYS } from "@exp/ecode/replay"
import { rollup } from "@exp/ecode/checks"
import { providerValues } from "@exp/ecode"
import { repositoryProviderValues } from "@exp/db-schema"

// #EXP-1613: eCode's spec (packages/ecode) replays here, inside the gate, and
// stays bound to what this server already does: the check enums and the
// rollup are domain-contract's (#EXP-1610), so eCode's own CI writes rows the
// Merge pill, the wake-ups and the merge train read unchanged. The forge
// fixture is what `lib/ecode`'s Forge implementations conform to (#EXP-1614).

describe(`ecode spec: the fixtures replay on the TS reference`, () => {
  const files = { forge, runner, checks, pipeline, remote } as const
  for (const [name, file] of Object.entries(files)) {
    it(name, () => {
      const replay = REPLAYS[name as keyof typeof REPLAYS] as (fixture: unknown) => unknown
      expect(replay(file)).toEqual(file)
    })
  }
})

describe(`ecode spec: bound to domain-contract's checks`, () => {
  it(`the check enums are the same`, () => {
    expect(ecodeContract.checkStatus.values).toEqual(contract.prCheckStatus.values)
    expect(ecodeContract.checkConclusion.values).toEqual(contract.prCheckConclusion.values)
    expect(ecodeContract.checkConclusion.failing).toEqual(contract.prChecks.failingConclusions)
    expect(ecodeContract.checksState.values).toEqual(contract.prChecksState.values)
    expect(ecodeContract.runner.queuedFlagMinutes).toBe(contract.prChecks.queuedFlagMinutes)
    expect([...ecodeContract.prState.values].sort()).toEqual([...contract.prState.values].sort())
  })

  it(`repositories.provider is the contract's provider enum (#EXP-1614)`, () => {
    expect([...repositoryProviderValues]).toEqual([...providerValues])
  })

  it(`the rollup is aggregateChecksState`, () => {
    for (const row of checks.rollup) expect(rollup(row.checks), row.name).toBe(aggregateChecksState(row.checks))
  })

  it(`the gate agrees with requiredChecksState`, () => {
    const expected = { ready: `success`, blocked: `failure`, waiting: `pending`, no_checks: `none` } as const
    for (const row of checks.gate) {
      if (row.required.length === 0) continue // domain-contract then reads the rows' `required` flags
      expect(requiredChecksState(row.checks, row.required), row.name).toBe(expected[row.result.gate as keyof typeof expected])
    }
  })
})
