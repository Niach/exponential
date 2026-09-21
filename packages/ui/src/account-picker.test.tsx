import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  ACCOUNT_LIMIT_LABELS,
  AccountLimitBars,
  AccountPicker,
  accountLimitBars,
  limitTone,
  type AccountPickerOption,
} from "./account-picker"

// Radix positions popovers with ResizeObserver, which jsdom lacks.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
// cmdk scrolls the active row into view; jsdom has no layout.
Element.prototype.scrollIntoView ??= () => {}

const options: AccountPickerOption[] = [
  {
    key: `codex:main`,
    agent: `codex`,
    email: `codex@x.test`,
    limits: { fiveHour: 0.05, week: 0.5 },
  },
  {
    key: `claude:work`,
    agent: `claude`,
    email: `work@x.test`,
    limits: { fiveHour: 0.4, week: 0.85, model: { label: `Fable`, used: 0.97 } },
  },
  { key: `claude:home`, agent: `claude`, email: `home@x.test`, hint: `Needs re-login` },
]

// EXP-872: ONE account picker — brand mark + email in the chip and in every
// row, never a profile name, never "default".
describe(`AccountPicker`, () => {
  it(`says the picked login's email beside its brand mark`, () => {
    render(
      <AccountPicker value="claude:work" options={options} onChange={vi.fn()} />
    )
    const trigger = screen.getByLabelText(`Account`)
    expect(trigger.textContent).toContain(`work@x.test`)
    expect(trigger.querySelector(`svg`)).toBeTruthy()
    expect(trigger.textContent?.toLowerCase()).not.toContain(`default`)
  })

  it(`collapses to plain text with a single login`, () => {
    render(
      <AccountPicker value="codex:main" options={[options[0]!]} onChange={vi.fn()} />
    )
    expect(screen.queryByRole(`button`)).toBeNull()
    expect(screen.getByText(`codex@x.test`)).toBeTruthy()
  })

  it(`lists every login with its email and reports the picked key`, () => {
    const onChange = vi.fn()
    render(
      <AccountPicker value="claude:work" options={options} onChange={onChange} />
    )
    act(() => {
      fireEvent.click(screen.getByLabelText(`Account`))
    })
    expect(screen.getByText(`codex@x.test`)).toBeTruthy()
    expect(screen.getByText(`home@x.test`)).toBeTruthy()
    expect(screen.getByText(`Needs re-login`)).toBeTruthy()
    fireEvent.click(screen.getByText(`home@x.test`))
    expect(onChange).toHaveBeenCalledWith(`claude:home`)
  })

  // EXP-992: a pointer resting on a row opens the compact preview beside it.
  it(`previews a login's limits on hover`, async () => {
    vi.useFakeTimers()
    try {
      render(
        <AccountPicker value="claude:work" options={options} onChange={vi.fn()} />
      )
      act(() => {
        fireEvent.click(screen.getByLabelText(`Account`))
      })
      // The trigger says the email too; the ROW is the one inside cmdk.
      const row = screen
        .getAllByText(`work@x.test`)
        .map((node) => node.closest(`[cmdk-item]`))
        .find(Boolean)!
        .querySelector(`[data-slot=hover-card-trigger]`)!
      act(() => {
        fireEvent.pointerEnter(row, { pointerType: `mouse` })
      })
      act(() => {
        vi.advanceTimersByTime(300)
      })
      const preview = document.querySelector(`[data-slot=account-limits-preview]`)
      expect(preview).toBeTruthy()
      expect(preview!.querySelectorAll(`[data-slot=meter]`)).toHaveLength(3)
      expect(preview!.textContent).toContain(`5h`)
      expect(preview!.textContent).toContain(`week`)
      expect(preview!.textContent).toContain(`fable`)
      // A login without a report has no preview to hover.
      expect(
        screen
          .getAllByText(`home@x.test`)
          .some((node) => node.closest(`[data-slot=hover-card-trigger]`))
      ).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`the row variant leads with the title and trails with the pick`, () => {
    render(
      <AccountPicker
        variant="row"
        mobileTitle="Default account"
        value="codex:main"
        options={options}
        onChange={vi.fn()}
      />
    )
    const row = screen.getByRole(`button`)
    expect(row.textContent).toContain(`Default account`)
    expect(row.textContent).toContain(`codex@x.test`)
  })
})

// EXP-992: the compact preview — three bars, `5h` / `week` / `<model>`.
describe(`account limit bars`, () => {
  it(`orders the bars 5h, week, model and lower-cases the model label`, () => {
    expect(
      accountLimitBars({ fiveHour: 0.4, week: 0.85, model: { label: `Fable`, used: 0.1 } })
    ).toEqual([
      { key: `fiveHour`, label: `5h`, used: 0.4 },
      { key: `week`, label: `week`, used: 0.85 },
      { key: `model`, label: `fable`, used: 0.1 },
    ])
    expect(accountLimitBars({ fiveHour: 0, week: 0 })).toHaveLength(2)
    expect(ACCOUNT_LIMIT_LABELS).toEqual({ fiveHour: `5h`, week: `week` })
  })

  it(`tones the bars on the usage thresholds`, () => {
    expect(limitTone(0.1)).toBe(`normal`)
    expect(limitTone(0.75)).toBe(`warning`)
    expect(limitTone(0.95)).toBe(`danger`)
  })

  it(`renders one meter per bar`, () => {
    const { container } = render(
      <AccountLimitBars
        limits={{ fiveHour: 0.4, week: 0.85, model: { label: `Fable`, used: 0.97 } }}
      />
    )
    const meters = container.querySelectorAll(`[data-slot=meter]`)
    expect(meters).toHaveLength(3)
    expect(meters[2]!.getAttribute(`data-tone`)).toBe(`danger`)
    expect(container.textContent).toContain(`fable`)
  })
})
