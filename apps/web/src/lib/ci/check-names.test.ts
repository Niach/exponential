import { describe, expect, it } from "vitest"
import { planPipeline, validatePipeline } from "@exp/ecode"
import { readWorkflowText } from "@exp/ecode/workflow-text"
import { mergeOwnChecks, prefixWorkflowChecks } from "@/lib/ci/check-names"

// #EXP-2078: a GitHub repo with our CI on plans its workflows, and Actions
// runs them too: our checks must not share GitHub's names.

const WORKFLOW = `name: ci
on: [push, pull_request]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
      - run: npm ci
  Test:
    name: unit tests
    runs-on: ubuntu-latest
    needs: build
    strategy:
      matrix:
        shard: [1, 2]
    steps:
      - uses: dtolnay/rust-toolchain@stable
      - uses: astral-sh/setup-uv@v5
      - run: cargo test
`

const TRIGGER = { kind: `pull_request`, branch: null, base: `main`, tag: null, changedFiles: null, fork: false } as const

describe(`checks planned from a GitHub repo's workflows (#EXP-2078)`, () => {
  it(`reads the common setup actions as steps instead of refusing the workflow`, () => {
    const read = readWorkflowText(WORKFLOW)
    expect(read.errors).toEqual([])
    const plan = planPipeline(read.file as never, TRIGGER)!
    expect(plan.jobs[0]!.steps.map((step) => step.name)).toEqual([`actions/setup-node`, `npm ci`])
    expect(plan.jobs[1]!.steps.map((step) => step.name)).toEqual([`dtolnay/rust-toolchain`, `astral-sh/setup-uv`, `cargo test`])
  })

  it(`names every check "ecode <job>" and keeps needs resolving`, () => {
    const file = prefixWorkflowChecks(readWorkflowText(WORKFLOW).file)
    expect(validatePipeline(file as never)).toEqual([])
    const plan = planPipeline(file as never, TRIGGER)!
    expect(plan.jobs.map((job) => [job.check, job.needs])).toEqual([
      [`ecode build`, []],
      [`ecode unit tests (1)`, [`ecode build`]],
      [`ecode unit tests (2)`, [`ecode build`]],
    ])
  })

  it(`a long job name still validates with the prefix`, () => {
    const file = prefixWorkflowChecks({ version: 1, on: { push: {} }, jobs: { a: { name: `n`.repeat(64), runsOn: [`linux`], steps: [{ name: `x`, run: `true` }] } } })
    expect(validatePipeline(file as never)).toEqual([])
  })

  it(`GitHub's check wins a name clash, so only the prefixed check survives the repair snapshot`, () => {
    const github = [{ name: `build`, from: `github` }]
    const plain = planPipeline(readWorkflowText(WORKFLOW).file as never, TRIGGER)!
    const prefixed = planPipeline(prefixWorkflowChecks(readWorkflowText(WORKFLOW).file) as never, TRIGGER)!
    const own = (plan: typeof plain) => [{ name: plan.jobs[0]!.check, from: `ours` }]
    expect(mergeOwnChecks(github, own(plain))).toEqual([{ name: `build`, from: `github` }])
    expect(mergeOwnChecks(github, own(prefixed))).toEqual([
      { name: `build`, from: `github` },
      { name: `ecode build`, from: `ours` },
    ])
  })
})
