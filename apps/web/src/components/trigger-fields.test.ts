import { describe, expect, it } from "vitest"

import {
  draftFromTrigger,
  draftToTrigger,
  emptyTriggerDraft,
  validateTriggerDraft,
  accountPinOf,
  triggerAccountKey,
  pickedAccountOption,
  seedAccountPin,
} from "@/components/trigger-fields"
import { ACCOUNT_FIXTURE } from "@/lib/accounts/account-option.test"
import { flattenAccounts } from "@/lib/accounts/account-option"
import type { SteerDevice } from "@/lib/steer-devices"

// EXP-995: the trigger editor's ACCOUNT pin — the pure half of the web
// dialog (`seedAccountPin` / `pickedAccountOption`), which the desktop, iOS
// and Android editors mirror rule for rule: a bound machine seeds its LAST
// USED login (which names the agent), a stored profile the machine no longer
// reports reads back as that agent's first login, and every pick stores its
// profile id verbatim (`` for an unpinned option).

const device: SteerDevice = {
  deviceId: `dev-1`,
  deviceLabel: `Build box`,
  agents: [`claude`, `codex`],
  caps: [`triggers`],
  launchDefaults: ACCOUNT_FIXTURE.launchDefaults ?? undefined,
  agentAccounts: ACCOUNT_FIXTURE.agentAccounts ?? undefined,
}

describe(`trigger account pin (EXP-995)`, () => {
  it(`keys the pin like every account picker`, () => {
    expect(triggerAccountKey({ agent: `claude`, account: `work` })).toBe(`claude:work`)
    expect(triggerAccountKey({ agent: `codex`, account: `` })).toBe(`codex:`)
  })

  it(`stores a picked option as its agent + profile, unpinned included`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(accountPinOf(options[0]!)).toEqual({ agent: `codex`, account: `main` })
    expect(
      accountPinOf({
        id: ``,
        agent: `claude`,
        email: `Claude Code`,
        isLastUsed: false,
        health: `unknown`,
      })
    ).toEqual({ agent: `claude`, account: `` })
  })

  it(`seeds a bound machine's last used login and leaves a reported pin alone`, () => {
    // Nothing pinned yet: the machine's last used login, which names the agent.
    expect(seedAccountPin(device, { agent: ``, account: `` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // A pin the machine reports as-is is never disturbed (a manual pick sticks).
    expect(seedAccountPin(device, { agent: `claude`, account: `home` })).toBeUndefined()
    expect(seedAccountPin(device, { agent: `codex`, account: `main` })).toBeUndefined()
    // No machine bound: nothing to seed from.
    expect(seedAccountPin(undefined, { agent: ``, account: `` })).toBeUndefined()
  })

  // Profile ids are device-LOCAL: after a device switch (or a stale stored
  // id) the pin lands on a login the bound machine REPORTS, so the Account
  // row never shows a fallback it would not save.
  it(`re-seeds a pin the machine does not report to that agent's login there`, () => {
    // Same agent, a profile this machine never had (another machine's id, or
    // a deleted one): that agent's first login here, its last used one.
    expect(seedAccountPin(device, { agent: `claude`, account: `gone` })).toEqual({
      agent: `claude`,
      account: `work`,
    })
    expect(seedAccountPin(device, { agent: `codex`, account: `other-box` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // An unpinned row (stored NULL): that agent's last used login, so the
    // Account row shows what Save stores.
    expect(seedAccountPin(device, { agent: `codex`, account: `` })).toEqual({
      agent: `codex`,
      account: `main`,
    })
    // An agent this machine does not run at all: the machine's last used login.
    const claudeOnly: SteerDevice = {
      ...device,
      deviceId: `dev-2`,
      agents: [`claude`],
      launchDefaults: { defaultAgent: `claude` },
      agentAccounts: { claude: ACCOUNT_FIXTURE.agentAccounts!.claude! },
    }
    expect(seedAccountPin(claudeOnly, { agent: `codex`, account: `main` })).toEqual({
      agent: `claude`,
      account: `work`,
    })
    // A machine reporting no profile offers an unpinned row per agent: a
    // profile pin there lands unpinned (its last used login).
    const bare: SteerDevice = { ...claudeOnly, agentAccounts: undefined }
    expect(seedAccountPin(bare, { agent: `claude`, account: `work` })).toEqual({
      agent: `claude`,
      account: ``,
    })
  })

  it(`reads a stored pin back as its row, else the agent's first login`, () => {
    const options = flattenAccounts(ACCOUNT_FIXTURE)
    expect(pickedAccountOption(options, { agent: `claude`, account: `home` })?.id).toBe(`home`)
    // A profile the machine no longer reports falls to that agent's first row.
    expect(pickedAccountOption(options, { agent: `claude`, account: `gone` })?.id).toBe(`work`)
    expect(pickedAccountOption(options, { agent: ``, account: `` })).toBeUndefined()
  })
})

// NEW-2: the when-part = THE schedule picker's rows (the stored form a
// 5-field cron + IANA zone; `@exp/ui` `ScheduleFields`, fixture-locked in
// `schedule-picker.json`), or an event name.
describe(`trigger draft (NEW-2)`, () => {
  it(`starts as a daily 09:00 schedule in the browser's zone`, () => {
    const draft = emptyTriggerDraft()
    expect(draft.kind).toBe(`schedule`)
    expect(draft.schedule.schedule).toEqual({
      repeat: `daily`,
      minute: 0,
      hour: 9,
      weekdays: [1, 2, 3, 4, 5],
      dayOfMonth: 1,
    })
    expect(draft.schedule.tz).toBe(Intl.DateTimeFormat().resolvedOptions().timeZone)
    expect(draftToTrigger(draft).kind).toBe(`schedule`)
    expect((draftToTrigger(draft) as { cron: string }).cron).toBe(`0 9 * * *`)
  })

  it(`reads a stored schedule into the rows and writes the canonical cron back`, () => {
    const weekdays = { kind: `schedule` as const, cron: `0 9 * * 1-5`, tz: `Europe/Vienna` }
    const draft = draftFromTrigger(weekdays)
    expect(draft.schedule.schedule?.repeat).toBe(`weekly`)
    expect(draft.schedule.schedule?.weekdays).toEqual([1, 2, 3, 4, 5])
    expect(draftToTrigger(draft)).toEqual({ kind: `schedule`, cron: `0 9 * * 1,2,3,4,5`, tz: `Europe/Vienna` })
    // A zone-less trigger reads as UTC and writes it.
    const utc = { kind: `schedule` as const, cron: `0 0 * * 0` }
    expect(draftToTrigger(draftFromTrigger(utc))).toEqual({ kind: `schedule`, cron: `0 0 * * 0`, tz: `UTC` })
    const event = { kind: `event` as const, name: `ui-changed` }
    expect(draftToTrigger(draftFromTrigger(event))).toEqual(event)
  })

  it(`keeps a cron the rows cannot express as custom, whitespace collapsed`, () => {
    const custom = { kind: `schedule` as const, cron: ` */10  * * *   1-5 `, tz: `Europe/Vienna` }
    const draft = draftFromTrigger(custom)
    expect(draft.schedule.schedule).toBeNull()
    expect(draft.schedule.cron).toBe(`*/10 * * * 1-5`)
    expect(draftToTrigger(draft)).toEqual({ kind: `schedule`, cron: `*/10 * * * 1-5`, tz: `Europe/Vienna` })
  })

  it(`validates the visible pane only`, () => {
    const base = draftFromTrigger({ kind: `schedule`, cron: `0 9 * * 1-5`, tz: `Europe/Vienna` })
    expect(validateTriggerDraft(base)).toEqual([])
    // A weekly needs a day.
    const noDays = { ...base, schedule: { ...base.schedule, schedule: { ...base.schedule.schedule!, weekdays: [] } } }
    expect(validateTriggerDraft(noDays).map((e) => e.field)).toEqual([`days`])
    // A custom cron needs five valid fields.
    const custom = (cron: string) => ({ ...base, schedule: { ...base.schedule, schedule: null, cron } })
    expect(validateTriggerDraft(custom(`*/10 * * * *`))).toEqual([])
    expect(validateTriggerDraft(custom(`0 9 * *`)).map((e) => e.field)).toEqual([`cron`])
    expect(validateTriggerDraft(custom(`61 9 * * *`)).map((e) => e.field)).toEqual([`cron`])
    expect(
      validateTriggerDraft({ ...base, schedule: { ...base.schedule, tz: `Mars/Base` } }).map((e) => e.field)
    ).toEqual([`tz`])
    const event = { ...base, kind: `event` as const }
    expect(validateTriggerDraft({ ...event, name: `issue.created` })).toEqual([])
    expect(validateTriggerDraft({ ...event, name: `deploy:prod` })).toEqual([])
    expect(validateTriggerDraft({ ...event, name: `` }).map((e) => e.field)).toEqual([`name`])
    expect(validateTriggerDraft({ ...event, name: `has space` }).map((e) => e.field)).toEqual([`name`])
  })
})
