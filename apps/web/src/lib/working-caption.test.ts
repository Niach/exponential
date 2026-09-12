import { describe, expect, it } from "vitest"
import { contract } from "@exp/domain-contract"
import {
  formatTokenCount,
  formatTurnDuration,
  workingCaption,
  workingVerb,
  WORKING_FALLBACK,
} from "@/lib/working-caption"

// EXP-850 §5: the working caption — the verb ladder, the duration format, the
// token format and the workflow override. Mirrored ×4.

describe(`workingVerb`, () => {
  it(`picks verbs[startedAt % verbs.length]`, () => {
    const verbs = contract.steerWorking.verbs
    expect(workingVerb(0)).toBe(verbs[0])
    expect(workingVerb(verbs.length + 3)).toBe(verbs[3])
    expect(workingVerb(1_789_204_409_163)).toBe(
      verbs[1_789_204_409_163 % verbs.length]
    )
  })

  it(`never falls off the ladder`, () => {
    for (const startedAt of [-5, 0.5, Number.MAX_SAFE_INTEGER]) {
      expect(contract.steerWorking.verbs).toContain(workingVerb(startedAt))
    }
  })
})

describe(`formatTurnDuration`, () => {
  it(`reads seconds, minutes then hours`, () => {
    expect(formatTurnDuration(37_000)).toBe(`37s`)
    expect(formatTurnDuration(59_999)).toBe(`59s`)
    expect(formatTurnDuration(60_000)).toBe(`1m 00s`)
    expect(formatTurnDuration(124_000)).toBe(`2m 04s`)
    expect(formatTurnDuration(3_600_000)).toBe(`1h 00m`)
    expect(formatTurnDuration(3_780_000)).toBe(`1h 03m`)
  })

  it(`a clock that ran backwards reads as zero`, () => {
    expect(formatTurnDuration(-1_000)).toBe(`0s`)
    expect(formatTurnDuration(400)).toBe(`0s`)
  })
})

describe(`formatTokenCount`, () => {
  it(`plain under a thousand, then one truncated decimal`, () => {
    expect(formatTokenCount(812)).toBe(`812`)
    expect(formatTokenCount(999)).toBe(`999`)
    expect(formatTokenCount(1_000)).toBe(`1.0k`)
    expect(formatTokenCount(2_049)).toBe(`2.0k`)
    expect(formatTokenCount(12_400)).toBe(`12.4k`)
    expect(formatTokenCount(999_999)).toBe(`999.9k`)
    expect(formatTokenCount(1_200_000)).toBe(`1.2M`)
  })
})

describe(`workingCaption`, () => {
  const startedAt = 1_000_000
  const verb = workingVerb(startedAt)

  it(`is verb + duration + tokens`, () => {
    expect(
      workingCaption({ startedAt, now: startedAt + 124_000, tokens: 12_400 })
    ).toBe(`${verb}… (2m 04s · ↓ 12.4k tokens)`)
  })

  it(`omits the group while it is unknown`, () => {
    expect(workingCaption({ startedAt, now: null, tokens: null })).toBe(
      `${verb}…`
    )
    expect(workingCaption({ startedAt, now: startedAt + 5_000 })).toBe(
      `${verb}… (5s)`
    )
    expect(workingCaption({ startedAt, tokens: 812 })).toBe(
      `${verb}… (↓ 812 tokens)`
    )
  })

  it(`without a turn start it is the bare fallback`, () => {
    expect(workingCaption({})).toBe(WORKING_FALLBACK)
    // A codex run reports neither: no verb to pick, and no clock to read.
    expect(workingCaption({ startedAt: null, now: 5, tokens: null })).toBe(
      WORKING_FALLBACK
    )
  })

  it(`a running workflow replaces the verb, keeping the suffix (§7)`, () => {
    expect(
      workingCaption({
        startedAt,
        now: startedAt + 37_000,
        tokens: 2_049,
        workflow: {
          name: `wire-probe`,
          status: `running`,
          phases: [{ index: 1, title: `Alpha` }],
          agents: [
            { index: 1, state: `done`, phaseIndex: 1 },
            { index: 2, state: `running`, phaseIndex: 1 },
          ],
        },
      })
    ).toBe(`Workflow wire-probe · 1/2 agents done · Alpha (37s · ↓ 2.0k tokens)`)
  })
})
