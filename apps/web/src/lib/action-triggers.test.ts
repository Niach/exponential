import { describe, expect, it } from "vitest"
import type {
  AutomationScheduleTrigger,
  AutomationTrigger,
  ActionTriggerEvent,
} from "@exp/db-schema/domain"
import {
  formatTriggerBlock,
  nextScheduleRun,
  parseActionTriggers,
  parseAutomationTrigger,
  actionRunTitle,
  triggerBadges,
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

describe(`formatTriggerBlock`, () => {
  it(`emits the machine-readable block, byte-locked ×4`, () => {
    expect(
      formatTriggerBlock({
        trigger: { kind: `schedule`, interval: `daily`, minuteOfDay: 540 },
        deviceId: `d-1`,
      })
    ).toBe(
      `\n\nTrigger — after creating the action, call exponential_actions_update with its id and \`triggers\` set to exactly this array: \`[{"kind":"schedule","interval":"daily","minuteOfDay":540,"deviceId":"d-1"}]\`. A triggered run fills no inputs, so declare none as required.`
    )
  })

  it(`appends the set pins after the device, and omits blank ones`, () => {
    const trigger = schedule()
    expect(
      formatTriggerBlock({ trigger, deviceId: `dev-1`, agent: `claude`, model: `opus` })
    ).toContain(
      JSON.stringify([{ ...trigger, deviceId: `dev-1`, agent: `claude`, model: `opus` }])
    )
    expect(formatTriggerBlock({ trigger, deviceId: `dev-1`, model: `` })).toContain(
      JSON.stringify([{ ...trigger, deviceId: `dev-1` }])
    )
  })
})

describe(`parseActionTriggers`, () => {
  const stored = {
    id: `t-1`,
    enabled: true,
    deviceId: `d-1`,
    kind: `schedule`,
    interval: `daily`,
    minuteOfDay: 540,
  }

  it(`reads a trigger with its runner, in order`, () => {
    expect(
      parseActionTriggers([
        { ...stored, agent: `claude`, account: `work`, model: `opus`, effort: `high` },
        { id: `t-2`, enabled: false, deviceId: `d-2`, kind: `event`, source: `exponential`, event: `created` },
      ])
    ).toEqual([
      { ...stored, agent: `claude`, account: `work`, model: `opus`, effort: `high` },
      { id: `t-2`, enabled: false, deviceId: `d-2`, kind: `event`, source: `exponential`, event: `created` },
    ])
  })

  it(`treats a missing enabled flag as enabled and a missing source as Exponential's`, () => {
    expect(
      parseActionTriggers([
        { id: `t-1`, deviceId: `d-1`, kind: `event`, event: `pr_merged` },
      ])
    ).toEqual([
      { id: `t-1`, enabled: true, deviceId: `d-1`, kind: `event`, source: `exponential`, event: `pr_merged` },
    ])
  })

  it(`skips what it cannot read instead of throwing`, () => {
    expect(parseActionTriggers(null)).toEqual([])
    expect(parseActionTriggers({})).toEqual([])
    expect(
      parseActionTriggers([
        `nope`,
        { ...stored, id: undefined },
        { ...stored, deviceId: `` },
        { ...stored, kind: `webhook` },
        // A future event source reads as "never fires".
        { id: `t-3`, deviceId: `d-1`, kind: `event`, source: `vapp:crm`, event: `created` },
        stored,
      ])
    ).toEqual([stored])
  })
})

describe(`actionRunTitle`, () => {
  it(`says what started the run`, () => {
    expect(actionRunTitle(`schedule`)).toBe(`Scheduled run`)
    expect(actionRunTitle(`event`)).toBe(`Event run`)
    expect(actionRunTitle(null)).toBe(`Manual run`)
    expect(actionRunTitle(undefined)).toBe(`Manual run`)
    // A child run another run started is neither a trigger's nor a person's.
    expect(actionRunTitle(`agent`)).toBe(`Agent run`)
    expect(actionRunTitle(`workflow`)).toBe(`Agent run`)
  })
})

describe(`triggerBadges`, () => {
  const of = (kind: `schedule` | `event`, enabled: boolean) =>
    parseActionTriggers([
      kind === `schedule`
        ? { id: `s`, enabled, deviceId: `d`, kind, interval: `daily`, minuteOfDay: 0 }
        : { id: `e`, enabled, deviceId: `d`, kind, event: `created` },
    ])

  it(`draws nothing for an action without triggers`, () => {
    expect(triggerBadges([])).toEqual({ schedule: null, event: null })
  })

  it(`draws a glyph per kind, muted while none of that kind is enabled`, () => {
    expect(
      triggerBadges([...of(`schedule`, false), ...of(`event`, true)])
    ).toEqual({ schedule: { active: false }, event: { active: true } })
    expect(
      triggerBadges([...of(`schedule`, false), ...of(`schedule`, true)])
    ).toEqual({ schedule: { active: true }, event: null })
  })
})
