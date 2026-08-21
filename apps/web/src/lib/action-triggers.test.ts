import { describe, expect, it } from "vitest"
import type {
  AutomationScheduleTrigger,
  AutomationTrigger,
  ActionTriggerEvent,
} from "@exp/db-schema/domain"
import {
  formatAutomationBlock,
  nextScheduleRun,
  parseAutomationTrigger,
  triggerSummary,
} from "./action-triggers"

const schedule = (
  overrides: Partial<AutomationScheduleTrigger> = {}
): AutomationScheduleTrigger => ({
  kind: `schedule`,
  interval: `daily`,
  minuteOfDay: 420,
  ...overrides,
})

describe(`parseAutomationTrigger`, () => {
  it(`returns null for non-objects`, () => {
    expect(parseAutomationTrigger(null)).toBeNull()
    expect(parseAutomationTrigger(undefined)).toBeNull()
    expect(parseAutomationTrigger(`schedule`)).toBeNull()
    expect(parseAutomationTrigger(42)).toBeNull()
    expect(parseAutomationTrigger([{ kind: `schedule` }])).toBeNull()
  })

  it(`returns null for an unknown kind (future vocabulary reads as no automation)`, () => {
    expect(parseAutomationTrigger({ kind: `webhook` })).toBeNull()
    expect(parseAutomationTrigger({})).toBeNull()
  })

  it(`returns null for an unknown event value`, () => {
    expect(
      parseAutomationTrigger({
        kind: `event`,
        event: `comment_added`,
      })
    ).toBeNull()
  })

  it(`rejects out-of-range schedule fields`, () => {
    expect(parseAutomationTrigger(schedule({ minuteOfDay: -1 }))).toBeNull()
    expect(parseAutomationTrigger(schedule({ minuteOfDay: 1440 }))).toBeNull()
    expect(parseAutomationTrigger(schedule({ minuteOfDay: 7.5 }))).toBeNull()
    expect(
      parseAutomationTrigger(schedule({ interval: `weekly` }))
    ).toBeNull()
    expect(
      parseAutomationTrigger(schedule({ interval: `weekly`, weekday: 8 }))
    ).toBeNull()
    expect(
      parseAutomationTrigger(schedule({ interval: `monthly` }))
    ).toBeNull()
    expect(
      parseAutomationTrigger(schedule({ interval: `monthly`, dayOfMonth: 29 }))
    ).toBeNull()
    expect(
      parseAutomationTrigger({ ...schedule(), interval: `hourly` })
    ).toBeNull()
  })

  it(`round-trips a valid schedule`, () => {
    const parsed = parseAutomationTrigger({
      kind: `schedule`,
      interval: `weekly`,
      minuteOfDay: 540,
      weekday: 1,
    })
    expect(parsed).toEqual({
      kind: `schedule`,
      interval: `weekly`,
      minuteOfDay: 540,
      weekday: 1,
    })
  })

  it(`keeps only well-formed event filters and drops empty lists`, () => {
    const parsed = parseAutomationTrigger({
      kind: `event`,
      event: `label_added`,
      filters: {
        boardIds: [`b1`, 7, `b2`],
        labelIds: [],
        priorities: [`urgent`, `bogus`],
        somethingElse: [`x`],
      },
    })
    expect(parsed).toEqual({
      kind: `event`,
      event: `label_added`,
      filters: { boardIds: [`b1`, `b2`], priorities: [`urgent`] },
    })
  })

  it(`parses an event trigger without filters`, () => {
    expect(
      parseAutomationTrigger({
        kind: `event`,
        event: `pr_merged`,
      })
    ).toEqual({
      kind: `event`,
      event: `pr_merged`,
    })
  })
})

describe(`triggerSummary`, () => {
  it(`locks the schedule sentences`, () => {
    expect(triggerSummary(schedule({ minuteOfDay: 420 }))).toBe(`Daily at 07:00`)
    expect(
      triggerSummary(schedule({ interval: `weekly`, minuteOfDay: 540, weekday: 1 }))
    ).toBe(`Weekly on Monday at 09:00`)
    expect(
      triggerSummary(
        schedule({ interval: `monthly`, minuteOfDay: 540, dayOfMonth: 5 })
      )
    ).toBe(`Monthly on day 5 at 09:00`)
    expect(
      triggerSummary(schedule({ interval: `weekly`, minuteOfDay: 5, weekday: 7 }))
    ).toBe(`Weekly on Sunday at 00:05`)
  })

  it(`locks the event sentences`, () => {
    const event = (name: ActionTriggerEvent): AutomationTrigger => ({
      kind: `event`,
      event: name,
    })
    expect(triggerSummary(event(`created`))).toBe(`When an issue is created`)
    expect(triggerSummary(event(`status_changed`))).toBe(`When status changes`)
    expect(triggerSummary(event(`assignee_changed`))).toBe(
      `When the assignee changes`
    )
    expect(triggerSummary(event(`label_added`))).toBe(`When a label is added`)
    expect(triggerSummary(event(`priority_changed`))).toBe(
      `When priority changes`
    )
    expect(triggerSummary(event(`pr_opened`))).toBe(
      `When a pull request is opened`
    )
    expect(triggerSummary(event(`pr_merged`))).toBe(
      `When a pull request is merged`
    )
  })

  it(`appends the total filter count across lists`, () => {
    expect(
      triggerSummary({
        kind: `event`,
        event: `status_changed`,
        filters: { boardIds: [`b1`, `b2`], toStatusIds: [`s1`] },
      })
    ).toBe(`When status changes · 3 filters`)
    expect(
      triggerSummary({
        kind: `event`,
        event: `label_added`,
        filters: { labelIds: [`l1`] },
      })
    ).toBe(`When a label is added · 1 filter`)
  })
})

describe(`nextScheduleRun`, () => {
  // 2026-08-18 is a Tuesday.
  const tue10 = new Date(2026, 7, 18, 10, 0, 0, 0)

  it(`daily just-passed rolls to tomorrow`, () => {
    const next = nextScheduleRun(schedule({ minuteOfDay: 540 }), tue10)
    expect(next).toEqual(new Date(2026, 7, 19, 9, 0, 0, 0))
  })

  it(`daily still-ahead fires today`, () => {
    const next = nextScheduleRun(schedule({ minuteOfDay: 690 }), tue10)
    expect(next).toEqual(new Date(2026, 7, 18, 11, 30, 0, 0))
  })

  it(`an occurrence exactly at now rolls forward (strictly after)`, () => {
    const next = nextScheduleRun(schedule({ minuteOfDay: 600 }), tue10)
    expect(next).toEqual(new Date(2026, 7, 19, 10, 0, 0, 0))
  })

  it(`weekly wraps to next week when today's slot has passed`, () => {
    const next = nextScheduleRun(
      schedule({ interval: `weekly`, weekday: 2, minuteOfDay: 540 }),
      tue10
    )
    // Tuesday 09:00 already passed at 10:00 — next Tuesday.
    expect(next).toEqual(new Date(2026, 7, 25, 9, 0, 0, 0))
  })

  it(`weekly picks the coming weekday including Sunday (7)`, () => {
    const next = nextScheduleRun(
      schedule({ interval: `weekly`, weekday: 7, minuteOfDay: 0 }),
      tue10
    )
    expect(next).toEqual(new Date(2026, 7, 23, 0, 0, 0, 0))
  })

  it(`monthly day 28 stays in the current month when ahead, else advances`, () => {
    const ahead = nextScheduleRun(
      schedule({ interval: `monthly`, dayOfMonth: 28, minuteOfDay: 60 }),
      tue10
    )
    expect(ahead).toEqual(new Date(2026, 7, 28, 1, 0, 0, 0))
    const wrapped = nextScheduleRun(
      schedule({ interval: `monthly`, dayOfMonth: 5, minuteOfDay: 60 }),
      tue10
    )
    expect(wrapped).toEqual(new Date(2026, 8, 5, 1, 0, 0, 0))
  })

  it(`monthly wraps across the year end`, () => {
    const dec30 = new Date(2026, 11, 30, 12, 0, 0, 0)
    const next = nextScheduleRun(
      schedule({ interval: `monthly`, dayOfMonth: 28, minuteOfDay: 0 }),
      dec30
    )
    expect(next).toEqual(new Date(2027, 0, 28, 0, 0, 0, 0))
  })

  it(`returns null when the required part is missing`, () => {
    expect(
      nextScheduleRun(schedule({ interval: `weekly` }), tue10)
    ).toBeNull()
    expect(
      nextScheduleRun(schedule({ interval: `monthly` }), tue10)
    ).toBeNull()
  })
})

describe(`formatAutomationBlock`, () => {
  it(`emits the machine-readable block with the exact JSON`, () => {
    const trigger = schedule({ minuteOfDay: 420 })
    expect(
      formatAutomationBlock({ trigger, deviceId: `dev-1`, agent: `claude`, model: `opus` })
    ).toBe(
      `\n\nAutomation — after creating the action, call exponential_automations_create with its id and exactly these fields: \`${JSON.stringify({ deviceId: `dev-1`, trigger, agent: `claude`, model: `opus` })}\`. An automated run fills no inputs, so declare none as required.`
    )
  })

  it(`omits blank agent/model/effort`, () => {
    const trigger = schedule()
    expect(formatAutomationBlock({ trigger, deviceId: `dev-1`, model: `` })).toContain(
      JSON.stringify({ deviceId: `dev-1`, trigger })
    )
  })
})
