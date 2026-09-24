import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import * as pickers from "./index"

// EXP-1029 contract — every typed picker renders THROUGH the primitive (the
// `data-slot="picker"` marker). Skipped until EXP-1021 moves the account and
// icon pickers onto it; the IDE, iOS and Android carry the same test by name.

const trigger = <button type="button">open</button>
const noop = vi.fn()

const TYPED: ReadonlyArray<[string, () => React.ReactElement]> = [
  [
    `BoardPicker`,
    () => (
      <pickers.BoardPicker
        boards={[{ id: `b1`, name: `Web`, icon: `flag`, color: `#3B82F6` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [
    `IssuePicker`,
    () => (
      <pickers.IssuePicker
        issues={[{ id: `i1`, identifier: `APP-1`, title: `Fix it` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [
    `ActionPicker`,
    () => (
      <pickers.ActionPicker
        actions={[{ id: `a1`, name: `Release`, icon: `rocket` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [
    `AccountPicker`,
    () => (
      <pickers.AccountPicker
        value={null}
        options={[{ key: `claude:system`, agent: `claude`, email: `me@example.com` }]}
        onChange={noop}
      />
    ),
  ],
  [
    `DevicePicker`,
    () => (
      <pickers.DevicePicker
        devices={[{ id: `d1`, name: `Studio`, icon: `laptop` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [
    `AssigneePicker`,
    () => (
      <pickers.AssigneePicker
        members={[{ id: `u1`, name: `Ada`, email: `ada@example.com` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [`IconPicker`, () => <pickers.IconPicker value="flag" onChange={noop} />],
  [
    `StatusPicker`,
    () => (
      <pickers.StatusPicker
        statuses={[{ id: `s1`, name: `Backlog`, category: `backlog` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [
    `PriorityPicker`,
    () => (
      <pickers.PriorityPicker
        options={[{ value: `high`, label: `High` }]}
        value={null}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
  [
    `LabelPicker`,
    () => (
      <pickers.LabelPicker
        labels={[{ id: `l1`, name: `bug`, color: `#EF4444` }]}
        value={[]}
        onChange={noop}
        trigger={trigger}
      />
    ),
  ],
]

describe(`the shared picker API (EXP-1029 contract)`, () => {
  it(`exports the primitive and all ten typed pickers`, () => {
    expect(typeof pickers.Picker).toBe(`function`)
    for (const [name] of TYPED) {
      expect(typeof (pickers as Record<string, unknown>)[name]).toBe(`function`)
    }
  })
})

describe.skip(`every typed picker renders through the primitive (EXP-1029 → EXP-1021)`, () => {
  it.each(TYPED)(`%s`, (_name, element) => {
    const { container } = render(element())
    expect(container.querySelector(`[data-slot="picker"]`)).not.toBeNull()
  })
})
