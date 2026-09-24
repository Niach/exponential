import { describe, expect, it } from "vitest"
import {
  modelForNode,
  normalizeWorkflowLaunch,
  reviewModelFor,
  type WorkflowLaunch,
} from "./workflow-launch"

// EXP-1029 contract — the acceptance table for lib/workflow-launch.ts.
// EXP-1014 implements the three functions and un-skips this file; the Rust
// mirror (`coding::workflows::launch`) carries the same cases by name.

const claude: WorkflowLaunch = {
  agent: `claude`,
  model: `opus`,
  strongModel: `fable`,
}

describe.skip(`normalizeWorkflowLaunch (EXP-1029)`, () => {
  it(`reads the new shape verbatim`, () => {
    expect(
      normalizeWorkflowLaunch({
        agent: `claude`,
        account: `p-1`,
        model: `sonnet`,
        strongModel: `opus`,
      })
    ).toEqual({ agent: `claude`, account: `p-1`, model: `sonnet`, strongModel: `opus` })
  })

  it(`fills an empty row from the claude defaults`, () => {
    expect(normalizeWorkflowLaunch({})).toEqual(claude)
    expect(normalizeWorkflowLaunch(null)).toEqual(claude)
    expect(normalizeWorkflowLaunch(`garbage`)).toEqual(claude)
  })

  it(`fills a codex row from the codex defaults`, () => {
    expect(normalizeWorkflowLaunch({ agent: `codex` })).toEqual({
      agent: `codex`,
      model: `gpt-5.6-sol`,
      strongModel: `gpt-5.6-luna`,
    })
  })

  it(`degrades an unknown agent to claude`, () => {
    expect(normalizeWorkflowLaunch({ agent: `pi` }).agent).toBe(`claude`)
  })

  it(`folds an old row's pins into strongModel, reviewModel first`, () => {
    expect(
      normalizeWorkflowLaunch({ agent: `claude`, model: `opus`, contractModel: `sonnet` })
        .strongModel
    ).toBe(`sonnet`)
    expect(
      normalizeWorkflowLaunch({ agent: `claude`, riskModel: `sonnet`, reviewModel: `opus` })
        .strongModel
    ).toBe(`opus`)
    expect(
      normalizeWorkflowLaunch({ agent: `claude`, integrationModel: `sonnet` }).strongModel
    ).toBe(`sonnet`)
  })

  it(`lets a stored strongModel win over every legacy pin`, () => {
    expect(
      normalizeWorkflowLaunch({ strongModel: `opus`, contractModel: `sonnet` }).strongModel
    ).toBe(`opus`)
  })

  it(`drops subagentModel, effort and maxParallel`, () => {
    expect(
      normalizeWorkflowLaunch({
        agent: `claude`,
        subagentModel: `sonnet`,
        effort: `high`,
        maxParallel: 5,
      })
    ).toEqual(claude)
  })

  it(`keeps model as model, even beside old pins`, () => {
    expect(
      normalizeWorkflowLaunch({ model: `sonnet`, contractModel: `fable` })
    ).toEqual({ agent: `claude`, model: `sonnet`, strongModel: `fable` })
  })

  it(`drops a blank account`, () => {
    expect(normalizeWorkflowLaunch({ account: `` })).toEqual(claude)
    expect(normalizeWorkflowLaunch({ account: null })).toEqual(claude)
  })
})

describe.skip(`modelForNode (EXP-1029)`, () => {
  it(`runs a leaf on the cheap model`, () => {
    expect(modelForNode(claude, `leaf`, `low`)).toBe(`opus`)
    expect(modelForNode(claude, `leaf`, `medium`)).toBe(`opus`)
  })

  it(`runs contract and integration nodes on the strong model`, () => {
    expect(modelForNode(claude, `contract`, `low`)).toBe(`fable`)
    expect(modelForNode(claude, `integration`, `low`)).toBe(`fable`)
  })

  it(`runs a high-risk node on the strong model, whatever its kind`, () => {
    expect(modelForNode(claude, `leaf`, `high`)).toBe(`fable`)
  })
})

describe.skip(`reviewModelFor (EXP-1029)`, () => {
  it(`reviews every node on the strong model`, () => {
    expect(reviewModelFor(claude)).toBe(`fable`)
    expect(
      reviewModelFor({ agent: `codex`, model: `gpt-5.6-sol`, strongModel: `gpt-5.6-luna` })
    ).toBe(`gpt-5.6-luna`)
  })
})
