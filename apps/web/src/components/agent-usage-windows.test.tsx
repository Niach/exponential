import { describe, expect, it } from "vitest"
import { render } from "@testing-library/react"

import type { DeviceAgentUsage } from "@/db/schema"
import { formatResetCountdown } from "@/lib/agent-usage"
import {
  AgentUsageWindows,
  fullUsageWindows,
  miniUsageWindows,
} from "@/components/agent-usage-windows"

const NOW = new Date(`2026-10-09T12:00:00Z`)
const IN_2H = new Date(NOW.getTime() + 2 * 3600_000).toISOString()

const USAGE: DeviceAgentUsage = {
  fetchedAt: NOW.toISOString(),
  windows: [
    { key: `model:fable`, label: `Fable`, percent: 97, resetsAt: IN_2H },
    { key: `weekly`, label: `Week`, percent: 80, resetsAt: IN_2H },
    { key: `session`, label: `5h`, percent: 4, resetsAt: IN_2H },
    { key: `credits`, label: `Credits`, percent: 10 },
  ],
}

describe(`fullUsageWindows`, () => {
  it(`orders session, weekly, models, other and names each window`, () => {
    const rows = fullUsageWindows(USAGE, NOW)
    expect(rows.map((row) => row.label)).toEqual([
      `Current session`,
      `All models`,
      `Fable only`,
      `Credits`,
    ])
    expect(rows[1]!.tone).toBe(`warning`)
    expect(rows[2]!.tone).toBe(`danger`)
    expect(rows[0]!.caption).toBe(formatResetCountdown(IN_2H, NOW))
  })
})

describe(`miniUsageWindows`, () => {
  it(`keeps session, week and the first model window, wire labels`, () => {
    const rows = miniUsageWindows(USAGE, NOW)
    expect(rows.map((row) => row.label)).toEqual([`5h`, `Week`, `Fable`])
    // EXP-944: only the two windows carry a reset; the model bar repeats none.
    expect(rows[0]!.caption).toBe(formatResetCountdown(IN_2H, NOW))
    expect(rows[2]!.caption).toBeNull()
  })

  it(`no clock = bars only`, () => {
    expect(miniUsageWindows(USAGE).every((row) => row.caption === null)).toBe(true)
  })
})

describe(`AgentUsageWindows`, () => {
  it(`renders nothing without a report`, () => {
    const { container } = render(<AgentUsageWindows usage={null} now={NOW} />)
    expect(container.innerHTML).toBe(``)
  })

  it(`full dims and captions an old report`, () => {
    const old: DeviceAgentUsage = { ...USAGE, stale: true }
    const { container } = render(<AgentUsageWindows usage={old} now={NOW} />)
    const root = container.querySelector(`[data-slot=usage-windows]`)!
    expect(root.className).toContain(`opacity-50`)
    expect(container.textContent).toContain(`as of`)
  })

  it(`mini draws three meters on one line`, () => {
    const { container } = render(
      <AgentUsageWindows usage={USAGE} now={NOW} density="mini" />
    )
    expect(container.querySelectorAll(`[data-slot=meter]`)).toHaveLength(3)
    expect(
      container.querySelector(`[data-slot=usage-windows]`)!.getAttribute(`data-density`)
    ).toBe(`mini`)
  })
})
