import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { contract } from "@exp/domain-contract"

import {
  AGENT_LABELS,
  AgentPicker,
  AgentPickerTabs,
  agentLabel,
  agentMarkIcon,
} from "@/components/agent-picker"
import { ClaudeIcon, CodexIcon } from "@/components/icons/brand-icons"
import { conceptIcon } from "@/lib/icons.generated"

// Radix positions menus with ResizeObserver, which jsdom lacks.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never

const openMenu = (trigger: HTMLElement) => {
  act(() => {
    fireEvent.pointerDown(trigger, {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

// EXP-862: ONE agent picker. The trigger is icon-only — the label lives in
// the accessible name, the tooltip and the menu rows.
describe(`agentLabel`, () => {
  it(`labels every agent the contract ships`, () => {
    for (const value of contract.codingAgent.values) {
      expect(AGENT_LABELS[value], value).toBeTruthy()
    }
    expect(agentLabel(`claude`)).toBe(`Claude Code`)
    expect(agentLabel(`codex`)).toBe(`Codex`)
  })

  it(`falls back to the raw id for an agent this build does not know`, () => {
    expect(agentLabel(`some-acp-binary`)).toBe(`some-acp-binary`)
  })
})

describe(`agentMarkIcon`, () => {
  it(`draws each shipped agent's own brand mark`, () => {
    expect(agentMarkIcon(`claude`)).toBe(ClaudeIcon)
    expect(agentMarkIcon(`codex`)).toBe(CodexIcon)
  })

  it(`falls back to the neutral agent concept for an unknown id`, () => {
    const neutral = conceptIcon(`settings-agents`)
    for (const id of [`pi`, `external`, ``]) {
      expect(agentMarkIcon(id)).toBe(neutral)
    }
  })
})

describe(`AgentPicker`, () => {
  it(`renders an icon-only trigger whose accessible name is the label`, () => {
    render(
      <AgentPicker
        value="claude"
        agents={[`claude`, `codex`]}
        onChange={vi.fn()}
      />
    )
    const trigger = screen.getByLabelText(`Claude Code`)
    // The mark plus the chevron, and no visible text on the trigger itself.
    expect(trigger.querySelectorAll(`svg`)).toHaveLength(2)
    expect(trigger.textContent).toBe(``)
  })

  it(`lists mark + label rows and reports the pick`, () => {
    const onChange = vi.fn()
    render(
      <AgentPicker
        value="claude"
        agents={[`claude`, `codex`]}
        onChange={onChange}
      />
    )
    openMenu(screen.getByLabelText(`Claude Code`))
    const items = screen.getAllByRole(`menuitem`)
    expect(items).toHaveLength(2)
    for (const item of items) {
      expect(item.querySelector(`svg`)).not.toBeNull()
    }
    expect(items[0]!.textContent).toContain(`Claude Code`)
    // Only the current pick carries the check glyph.
    expect(items[0]!.querySelectorAll(`svg`)).toHaveLength(2)
    expect(items[1]!.querySelectorAll(`svg`)).toHaveLength(1)
    fireEvent.click(screen.getByText(`Codex`))
    expect(onChange).toHaveBeenCalledWith(`codex`)
  })

  it(`disables itself when asked, and when no agent is runnable`, () => {
    const { rerender } = render(
      <AgentPicker value="claude" agents={[`claude`]} onChange={vi.fn()} disabled />
    )
    expect(
      screen.getByLabelText(`Claude Code`).hasAttribute(`disabled`)
    ).toBe(true)
    rerender(<AgentPicker value="claude" agents={[]} onChange={vi.fn()} />)
    expect(
      screen.getByLabelText(`Claude Code`).hasAttribute(`disabled`)
    ).toBe(true)
  })

  it(`names an agent this build ships no mark for by its id`, () => {
    render(
      <AgentPicker
        value="some-acp-binary"
        agents={[`some-acp-binary`]}
        onChange={vi.fn()}
      />
    )
    expect(screen.getByLabelText(`some-acp-binary`)).not.toBeNull()
  })
})

describe(`AgentPickerTabs`, () => {
  it(`renders one segmented tab per agent, marked and labelled`, () => {
    const onChange = vi.fn()
    render(
      <AgentPickerTabs
        value="claude"
        agents={[`claude`, `codex`]}
        onChange={onChange}
      />
    )
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs).toHaveLength(2)
    expect(tabs[0]!.getAttribute(`data-state`)).toBe(`active`)
    expect(tabs[1]!.textContent).toContain(`Codex`)
    expect(tabs[1]!.querySelector(`svg`)).not.toBeNull()
    fireEvent.mouseDown(tabs[1]!)
    fireEvent.click(tabs[1]!)
    expect(onChange).toHaveBeenCalledWith(`codex`)
  })

  it(`renders nothing without agents`, () => {
    const { container } = render(
      <AgentPickerTabs value="claude" agents={[]} onChange={vi.fn()} />
    )
    expect(container.firstChild).toBeNull()
  })
})
