import { describe, expect, it } from "vitest"

import {
  workflowDefaultsFor,
  workflowDefaultsSummary,
  workflowFallbackFor,
} from "@/lib/devices/workflow-defaults"

describe(`workflowDefaultsFor`, () => {
  it(`falls back to the agent's contract pair when the device stored nothing`, () => {
    expect(workflowDefaultsFor(`claude`, null)).toEqual({ model: `opus`, strongModel: `fable` })
    expect(workflowDefaultsFor(`codex`, undefined)).toEqual({
      model: `gpt-5.6-sol`,
      strongModel: `gpt-5.6-luna`,
    })
  })

  it(`keeps a stored pair that belongs to the agent's vocabulary`, () => {
    expect(workflowDefaultsFor(`claude`, { model: `sonnet`, strongModel: `opus` })).toEqual({
      model: `sonnet`,
      strongModel: `opus`,
    })
  })

  it(`drops a stored name from the OTHER agent's vocabulary`, () => {
    // The device was on claude and its default account moved to codex.
    expect(workflowDefaultsFor(`codex`, { model: `opus`, strongModel: `fable` })).toEqual({
      model: `gpt-5.6-sol`,
      strongModel: `gpt-5.6-luna`,
    })
  })

  it(`resolves each half on its own`, () => {
    expect(workflowDefaultsFor(`claude`, { model: `sonnet`, strongModel: `nonsense` })).toEqual({
      model: `sonnet`,
      strongModel: `fable`,
    })
  })

  it(`reads an unknown agent as claude's pair`, () => {
    expect(workflowFallbackFor(`gemini`)).toEqual({ model: `opus`, strongModel: `fable` })
  })
})

describe(`workflowDefaultsSummary`, () => {
  it(`joins the pair the way every platform's row shows it`, () => {
    expect(workflowDefaultsSummary({ model: `opus`, strongModel: `fable` })).toBe(`opus · fable`)
  })
})
