import { useMemo, useState } from "react"
import contract from "@exp/domain-contract/contract"
import {
  SCHEDULE_DAY_OF_MONTH_MAX,
  SCHEDULE_EVERY_VALUES,
  SCHEDULE_PICKER_COPY,
  SCHEDULE_PICKER_TEST_IDS,
  SCHEDULE_REPEAT_VALUES,
  cronFromSchedule,
  defaultSchedule,
  scheduleFromCron,
  scheduleTimeLabel,
  weekdaysLabel,
  type Schedule,
  type ScheduleRepeat,
} from "@exp/domain-contract/schedule-picker"

import { cn } from "./cn"
import { Input } from "./input"
import { Picker, PickerTrigger } from "./picker"

// NEW-2: THE schedule picker ×4 (`fixtures/schedule-picker.json`; desktop
// `schedule_fields.rs`, iOS ExpUI `ScheduleFields.swift`, Android
// `ScheduleFields.kt`). Nobody speaks cron: a schedule trigger STORES a
// 5-field cron, these rows edit it. Rendered INSIDE a `GlassGroup` (the
// trigger editor's card, whose first row is the Schedule · On event strip):
// Repeat · Days (weekly) | Day (monthly) · Every (minutes) | Minute
// (hourly) | Time · Time zone (EXP-1363: daily, weekly, monthly, custom) · Cron (a custom, MCP-written expression only). CONTROLLED: the
// parent holds a `ScheduleDraft`.

const COPY = SCHEDULE_PICKER_COPY
const TEST_IDS = SCHEDULE_PICKER_TEST_IDS

/** What the rows edit: the structured schedule, or — for a cron the rows
 *  cannot express — null with the expression in `cron`; plus the IANA zone. */
export interface ScheduleDraft {
  schedule: Schedule | null
  /** The raw expression while `schedule` is null (custom). */
  cron: string
  /** IANA zone, never blank on a draft (a read without one is `UTC`). */
  tz: string
}

/** The zone a new schedule reads in: the device's own. */
export function deviceTimeZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || `UTC`
  } catch {
    return `UTC`
  }
}

/** A new schedule: daily at 09:00 in the device's zone. */
export function emptyScheduleDraft(tz: string = deviceTimeZone()): ScheduleDraft {
  return { schedule: defaultSchedule(), cron: ``, tz }
}

/** A stored schedule read into the rows (a cron the rows cannot express
 *  = custom, shown in its Cron row; a missing zone reads as UTC). */
export function scheduleDraftFromCron(cron: string, tz?: string | null): ScheduleDraft {
  return { schedule: scheduleFromCron(cron), cron: cron.trim().replace(/\s+/g, ` `), tz: tz?.trim() || `UTC` }
}

/** The wire cron of a draft: the structured schedule's canonical form, else
 *  the custom expression (whitespace collapsed). */
export function scheduleDraftCron(draft: ScheduleDraft): string {
  return draft.schedule ? cronFromSchedule(draft.schedule) : draft.cron.trim().replace(/\s+/g, ` `)
}

export interface ScheduleDraftError {
  field: `days` | `cron`
  message: string
}

/** Inline errors; [] = savable. A weekly needs a day; a custom cron five fields. */
export function validateScheduleDraft(draft: ScheduleDraft): ScheduleDraftError[] {
  if (draft.schedule) {
    return draft.schedule.repeat === `weekly` && draft.schedule.weekdays.length === 0
      ? [{ field: `days`, message: COPY.errors.noDays }]
      : []
  }
  return draft.cron.trim().split(/\s+/).length === 5 ? [] : [{ field: `cron`, message: COPY.errors.cron }]
}

const pad2 = (value: number) => String(value).padStart(2, `0`)
const range = (count: number, from = 0) => Array.from({ length: count }, (_, index) => index + from)

const REPEAT_ITEMS = SCHEDULE_REPEAT_VALUES.map((repeat) => ({
  value: repeat,
  label: COPY.repeat[repeat],
}))
const CUSTOM_ITEM = { value: `custom` as const, label: COPY.repeat.custom }
const WEEKDAY_ITEMS = range(7, 1).map((day) => ({
  value: String(day),
  label: COPY.weekdays[day - 1] ?? ``,
}))
const DAY_ITEMS = range(SCHEDULE_DAY_OF_MONTH_MAX, 1).map((day) => ({
  value: String(day),
  label: COPY.dayOfMonth.replace(`{day}`, String(day)),
}))
const EVERY_ITEMS = SCHEDULE_EVERY_VALUES.map((every) => ({
  value: String(every),
  label: COPY.every.replace(`{n}`, String(every)),
}))
const HOUR_ITEMS = range(24).map((hour) => ({ value: pad2(hour), label: pad2(hour) }))
const MINUTE_ITEMS = range(60).map((minute) => ({ value: pad2(minute), label: pad2(minute) }))
const MINUTE_OF_HOUR_ITEMS = range(60).map((minute) => ({
  value: pad2(minute),
  label: COPY.minuteOfHour.replace(`{minute}`, pad2(minute)),
}))

/** The contract's zones as picker rows; `_` and `/` read as spaces for search. */
const TIME_ZONE_ITEMS = contract.timeZones.names.map((zone) => ({
  value: zone,
  label: zone,
  keywords: [zone, zone.replace(/[_/]/g, ` `)],
}))

export const SCHEDULE_ROW_INPUT_CLASS = `h-auto rounded-none border-0 bg-transparent px-4 py-3 text-sm shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm`

/** The rows of the schedule picker. */
export function ScheduleFields({
  value,
  onChange,
  errors = [],
  disabled,
}: {
  value: ScheduleDraft
  onChange: (draft: ScheduleDraft) => void
  errors?: readonly ScheduleDraftError[]
  disabled?: boolean
}) {
  const { schedule } = value
  const setSchedule = (patch: Partial<Schedule>) => {
    if (!schedule) return
    onChange({ ...value, schedule: { ...schedule, ...patch } })
  }
  const pickRepeat = (repeat: ScheduleRepeat) => {
    const seed = defaultSchedule()
    const base = schedule ?? seed
    const next: Schedule = {
      ...base,
      repeat,
      weekdays: repeat === `weekly` && base.weekdays.length === 0 ? seed.weekdays : base.weekdays,
      dayOfMonth: repeat === `monthly` && !schedule ? seed.dayOfMonth : base.dayOfMonth,
    }
    onChange({ ...value, schedule: next, cron: `` })
  }
  const errorOf = (field: ScheduleDraftError[`field`]) => errors.some((error) => error.field === field)

  const repeatItems = useMemo(() => (schedule ? REPEAT_ITEMS : [...REPEAT_ITEMS, CUSTOM_ITEM]), [schedule])
  const timeZoneItems = useMemo(
    () =>
      TIME_ZONE_ITEMS.some((item) => item.value === value.tz)
        ? TIME_ZONE_ITEMS
        : [{ value: value.tz, label: value.tz, keywords: [value.tz] }, ...TIME_ZONE_ITEMS],
    [value.tz]
  )
  const [zoneQuery, setZoneQuery] = useState(``)

  return (
    <>
      <Picker
        mode="single"
        triggerVariant="row"
        mobileTitle={COPY.rows.repeat}
        value={schedule ? schedule.repeat : `custom`}
        onChange={(picked) => {
          if (picked !== null && picked !== `custom`) pickRepeat(picked as ScheduleRepeat)
        }}
        items={repeatItems}
        disabled={disabled}
        data-testid={TEST_IDS.repeat}
      />

      {schedule?.repeat === `weekly` && (
        <Picker
          mode="multi"
          triggerVariant="row"
          mobileTitle={COPY.rows.days}
          triggerLabel={COPY.errors.noDays}
          value={schedule.weekdays.map(String)}
          onChange={(picked) =>
            setSchedule({ weekdays: picked.map(Number).sort((a, b) => a - b) })
          }
          items={WEEKDAY_ITEMS}
          // The row's value is the fixture's label ("Weekdays", "Mon, Wed,
          // Fri"), never the primitive's "Monday, Tuesday +3".
          trigger={
            <PickerTrigger
              variant="row"
              label={COPY.rows.days}
              value={weekdaysLabel(schedule.weekdays) || undefined}
              placeholder={COPY.errors.noDays}
              disabled={disabled}
              aria-invalid={errorOf(`days`) || undefined}
            />
          }
          disabled={disabled}
          data-testid={TEST_IDS.days}
        />
      )}

      {schedule?.repeat === `monthly` && (
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle={COPY.rows.day}
          value={String(schedule.dayOfMonth)}
          onChange={(picked) => {
            if (picked !== null) setSchedule({ dayOfMonth: Number(picked) })
          }}
          items={DAY_ITEMS}
          disabled={disabled}
          data-testid={TEST_IDS.day}
        />
      )}

      {schedule?.repeat === `minutes` && (
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle={COPY.rows.every}
          value={String(schedule.every)}
          onChange={(picked) => {
            if (picked !== null) setSchedule({ every: Number(picked) })
          }}
          items={EVERY_ITEMS}
          disabled={disabled}
          data-testid={TEST_IDS.every}
        />
      )}

      {schedule?.repeat === `hourly` && (
        <Picker
          mode="single"
          triggerVariant="row"
          mobileTitle={COPY.rows.minute}
          value={pad2(schedule.minute)}
          onChange={(picked) => {
            if (picked !== null) setSchedule({ minute: Number(picked) })
          }}
          items={MINUTE_OF_HOUR_ITEMS}
          disabled={disabled}
          data-testid={TEST_IDS.minute}
        />
      )}

      {schedule && schedule.repeat !== `hourly` && schedule.repeat !== `minutes` && (
        <ScheduleTimeRow
          hour={schedule.hour}
          minute={schedule.minute}
          onChange={(hour, minute) => setSchedule({ hour, minute })}
          disabled={disabled}
        />
      )}

      {/* EXP-1363: a zone means nothing every few minutes or every hour; the
          stored zone is kept as is. */}
      {(!schedule || (schedule.repeat !== `hourly` && schedule.repeat !== `minutes`)) && (
      <Picker
        mode="single"
        triggerVariant="row"
        mobileTitle={COPY.rows.timeZone}
        search
        searchPlaceholder={COPY.timeZonePlaceholder}
        query={zoneQuery}
        onQueryChange={setZoneQuery}
        value={value.tz}
        onChange={(picked) => {
          if (picked !== null) onChange({ ...value, tz: picked })
          setZoneQuery(``)
        }}
        items={timeZoneItems}
        width="md"
        disabled={disabled}
        data-testid={TEST_IDS.timeZone}
      />
      )}

      {!schedule && (
        <div className="flex w-full items-center gap-3 text-sm">
          <span className="shrink-0 pl-4 text-foreground">{COPY.rows.cron}</span>
          <Input
            aria-label={COPY.rows.cron}
            value={value.cron}
            onChange={(e) => onChange({ ...value, cron: e.target.value })}
            placeholder={COPY.cronPlaceholder}
            spellCheck={false}
            autoComplete="off"
            disabled={disabled}
            aria-invalid={errorOf(`cron`) || undefined}
            className={cn(SCHEDULE_ROW_INPUT_CLASS, `text-right font-mono`)}
            data-testid={TEST_IDS.cron}
          />
        </div>
      )}
    </>
  )
}

/** The Time row ×4: the 24h `HH:mm`, its hour and its minute each a Picker
 *  (every hour, every minute) — never the locale-dependent browser time
 *  control. */
export function ScheduleTimeRow({
  hour,
  minute,
  onChange,
  disabled,
}: {
  hour: number
  minute: number
  onChange: (hour: number, minute: number) => void
  disabled?: boolean
}) {
  return (
    <div
      className="flex w-full items-center gap-3 px-4 py-3 text-sm"
      data-testid={TEST_IDS.time}
      aria-label={`${COPY.rows.time} ${scheduleTimeLabel(hour, minute)}`}
    >
      <span className="shrink-0 text-foreground">{COPY.rows.time}</span>
      <span className="ml-auto flex items-center gap-0.5 text-foreground/70 tabular-nums">
        <Picker
          mode="single"
          triggerVariant="inline"
          // EXP-1363: the 24/60 rows open in THEIR OWN scrolling popover
          // (a sheet on phones), never disclosed inline inside the row.
          presentation="popover"
          mobileTitle="Hour"
          width="sm"
          value={pad2(hour)}
          items={HOUR_ITEMS}
          onChange={(picked) => {
            if (picked !== null) onChange(Number(picked), minute)
          }}
          disabled={disabled}
        />
        <span aria-hidden>:</span>
        <Picker
          mode="single"
          triggerVariant="inline"
          // EXP-1363: the 24/60 rows open in THEIR OWN scrolling popover
          // (a sheet on phones), never disclosed inline inside the row.
          presentation="popover"
          mobileTitle="Minute"
          width="sm"
          value={pad2(minute)}
          items={MINUTE_ITEMS}
          onChange={(picked) => {
            if (picked !== null) onChange(hour, Number(picked))
          }}
          disabled={disabled}
        />
      </span>
    </div>
  )
}
