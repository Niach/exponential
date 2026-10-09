import { describe, expect, it } from "vitest"
import { render } from "@testing-library/react"

import { UsageWindows, usageTone, type UsageWindow } from "./usage-windows"

const WINDOWS: UsageWindow[] = [
  { key: `session`, label: `Current session`, percent: 9, caption: `resets in 1h 4m` },
  { key: `weekly`, label: `All models`, percent: 81 },
  { key: `model:fable`, label: `Fable`, percent: 97, caption: null },
]

describe(`usageTone`, () => {
  it(`tones on the EXP-909 thresholds`, () => {
    expect(usageTone(0)).toBe(`normal`)
    expect(usageTone(74)).toBe(`normal`)
    expect(usageTone(75)).toBe(`warning`)
    expect(usageTone(94)).toBe(`warning`)
    expect(usageTone(95)).toBe(`danger`)
  })
})

describe(`UsageWindows`, () => {
  it(`draws nothing without windows`, () => {
    const { container } = render(<UsageWindows windows={[]} />)
    expect(container.innerHTML).toBe(``)
  })

  it(`full: one meter + percent per window, the caption and the footnote`, () => {
    const { container } = render(<UsageWindows windows={WINDOWS} footnote="as of 2h ago" />)
    const meters = container.querySelectorAll(`[data-slot=meter]`)
    expect(meters).toHaveLength(3)
    expect(meters[1]!.getAttribute(`data-tone`)).toBe(`warning`)
    expect(meters[2]!.getAttribute(`data-tone`)).toBe(`danger`)
    expect(container.textContent).toContain(`Current session`)
    expect(container.textContent).toContain(`resets in 1h 4m`)
    expect(container.textContent).toContain(`81%`)
    expect(container.textContent).toContain(`as of 2h ago`)
  })

  it(`an explicit tone beats the percent's own`, () => {
    const { container } = render(
      <UsageWindows windows={[{ key: `a`, label: `5h`, percent: 99, tone: `normal` }]} />
    )
    expect(container.querySelector(`[data-slot=meter]`)!.getAttribute(`data-tone`)).toBe(`normal`)
  })

  it(`mini: one row, percents and the reset under the bar`, () => {
    const { container } = render(<UsageWindows windows={WINDOWS} density="mini" />)
    const root = container.querySelector(`[data-slot=usage-windows]`)!
    expect(root.getAttribute(`data-density`)).toBe(`mini`)
    expect(container.textContent).toContain(`97%`)
    expect(container.textContent).toContain(`resets in 1h 4m`)
  })

  it(`hover: label + meter only, no percent and no caption`, () => {
    const { container } = render(<UsageWindows windows={WINDOWS} density="hover" />)
    expect(container.querySelectorAll(`[data-slot=meter]`)).toHaveLength(3)
    expect(container.textContent).not.toContain(`%`)
    expect(container.textContent).not.toContain(`resets`)
  })

  it(`stale dims the block`, () => {
    const { container } = render(<UsageWindows windows={WINDOWS} density="mini" stale />)
    expect(container.querySelector(`[data-slot=usage-windows]`)!.className).toContain(`opacity-50`)
  })
})
