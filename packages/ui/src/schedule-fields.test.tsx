import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { SCHEDULE_PICKER_COPY as COPY, SCHEDULE_PICKER_TEST_IDS as IDS } from "@exp/domain-contract/schedule-picker"
import { ScheduleFields, scheduleDraftCron, scheduleDraftFromCron } from "./schedule-fields"

// EXP-1363 (schedule-picker.json): the minutes repeat's rows.
describe(`ScheduleFields`, () => {
  const noop = () => {}
  it(`every few minutes: the Every row, no Minute / Time / Time zone row`, () => {
    const draft = scheduleDraftFromCron(`*/15 * * * *`, `Europe/Vienna`)
    expect(draft.schedule?.repeat).toBe(`minutes`)
    expect(scheduleDraftCron(draft)).toBe(`*/15 * * * *`)
    render(<ScheduleFields value={draft} onChange={noop} />)
    expect(screen.getByText(COPY.rows.every)).toBeTruthy()
    expect(screen.getByText(`15 minutes`)).toBeTruthy()
    expect(screen.queryByText(COPY.rows.timeZone)).toBeNull()
    expect(screen.queryByTestId(IDS.time)).toBeNull()
    expect(screen.queryByText(COPY.rows.minute)).toBeNull()
  })
  it(`hourly has no Time zone row; daily keeps it`, () => {
    const { unmount } = render(<ScheduleFields value={scheduleDraftFromCron(`5 * * * *`, `UTC`)} onChange={noop} />)
    expect(screen.queryByText(COPY.rows.timeZone)).toBeNull()
    unmount()
    render(<ScheduleFields value={scheduleDraftFromCron(`0 9 * * *`, `UTC`)} onChange={noop} />)
    expect(screen.getByText(COPY.rows.timeZone)).toBeTruthy()
  })
})
