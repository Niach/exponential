import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
  TASK_LIST_PROGRESS_MAX_SEGMENTS,
  TaskListProgress,
} from "./task-list-progress"

describe(`TaskListProgress`, () => {
  it(`draws one segment per task, coloured by its status`, () => {
    render(
      <TaskListProgress
        statuses={[`completed`, `completed`, `in_progress`, `pending`]}
      />
    )
    const segments = [
      ...screen.getByTestId(`task-list-progress`).querySelectorAll(`[data-status]`),
    ]
    expect(segments.map((s) => s.getAttribute(`data-status`))).toEqual([
      `completed`,
      `completed`,
      `in_progress`,
      `pending`,
    ])
    expect(segments[0]!.className).toContain(`bg-foreground/70`)
    expect(segments[2]!.className).toContain(`bg-foreground/35`)
    expect(segments[3]!.className).toContain(`bg-foreground/12`)
  })

  it(`switches to one filled track past the segment cap`, () => {
    const statuses = Array.from(
      { length: TASK_LIST_PROGRESS_MAX_SEGMENTS + 4 },
      (_, i) => (i < 4 ? (`completed` as const) : (`pending` as const))
    )
    render(<TaskListProgress statuses={statuses} />)
    const track = screen.getByTestId(`task-list-progress`)
    expect(track.querySelectorAll(`[data-status]`)).toHaveLength(0)
    expect((track.firstElementChild as HTMLElement).style.width).toBe(`25%`)
  })

  it(`draws nothing for an empty list`, () => {
    const { container } = render(<TaskListProgress statuses={[]} />)
    expect(container.firstChild).toBeNull()
  })
})
