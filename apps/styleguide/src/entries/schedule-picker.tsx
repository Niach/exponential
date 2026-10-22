import fixture from "@exp/domain-contract/fixtures/schedule-picker.json"
import { scheduleSummary } from "@exp/domain-contract/schedule-picker"
import {
  GlassGroup,
  GlassTabsRow,
  ScheduleFields,
  SubShellHost,
  TabsTrigger,
  scheduleDraftFromCron,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-2 — THE schedule picker (fixtures/schedule-picker.json ×4): the rows an
// agent's schedule trigger is edited through. Nobody speaks cron: the stored
// form is a 5-field cron in an IANA zone, these rows are the only UI. Shown
// as the trigger editor draws it — ONE glass group whose first row is the
// Schedule · On event strip — for a weekly Monday–Friday 09:00 schedule, and
// below it the custom case: a cron an MCP wrote that the rows cannot
// express keeps its monospace Cron row until a Repeat is picked.

const noop = (): void => {}
const COPY = fixture.copy

const WEEKLY = scheduleDraftFromCron(`0 9 * * 1-5`, `Europe/Vienna`)
const CUSTOM = scheduleDraftFromCron(`*/10 * * * 1-5`, `Europe/Vienna`)

export const entry: StyleguideEntry = {
  id: `schedule-picker`,
  section: `general`,
  owner: `NEW-2`,
  title: `Schedule picker`,
  blurb: `The rows a schedule is edited through, identical ×4: ${COPY.rows.repeat} (${COPY.repeat.hourly} · ${COPY.repeat.daily} · ${COPY.repeat.weekly} · ${COPY.repeat.monthly}), then the repeat's own row — ${COPY.rows.days} (weekly: a multi picker whose value reads "${COPY.weekdaysWork}", "${COPY.weekdaysWeekend}", one full name or short names), ${COPY.rows.day} (monthly, 1–${fixture.dayOfMonthMax}) or ${COPY.rows.minute} (hourly) — the 24h ${COPY.rows.time} row (hour and minute each a picker, never the platform's time control) and ${COPY.rows.timeZone} (a searchable picker over the contract's zones, seeded with the device's own). The stored form stays a 5-field cron in an IANA zone (the server fires it); a cron the rows cannot express — one an MCP wrote — reads "${COPY.repeat.custom}" and keeps a monospace ${COPY.rows.cron} row until a repeat is picked. A Triggers row reads \`scheduleSummary\`: "${scheduleSummary(`0 9 * * 1-5`, `Europe/Vienna`)}".`,
  status: {
    web: { state: `ok`, symbol: `ScheduleFields`, file: `packages/ui/src/schedule-fields.tsx` },
    desktop: {
      state: `ok`,
      symbol: `ScheduleFieldsState`,
      file: `apps/desktop/crates/ui/src/schedule_fields.rs`,
    },
    ios: { state: `ok`, symbol: `ScheduleFields`, file: `apps/ios/ExpUI/Sources/ScheduleFields.swift` },
    android: {
      state: `ok`,
      symbol: `ScheduleFields`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/ScheduleFields.kt`,
    },
  },
  island: () => (
    <SubShellHost className="w-[30rem] space-y-4">
      <GlassGroup>
        <GlassTabsRow value="schedule" onValueChange={noop}>
          <TabsTrigger value="schedule">Schedule</TabsTrigger>
          <TabsTrigger value="event">On event</TabsTrigger>
        </GlassTabsRow>
        <ScheduleFields value={WEEKLY} onChange={noop} />
      </GlassGroup>
      <GlassGroup>
        <ScheduleFields value={CUSTOM} onChange={noop} />
      </GlassGroup>
    </SubShellHost>
  ),
}
