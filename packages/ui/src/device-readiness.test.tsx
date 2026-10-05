import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import spec from "@exp/domain-contract/fixtures/device-doctor.json"

import {
  DeviceReadiness,
  deviceReadinessBlocker,
  deviceReadinessModel,
  deviceReadinessRunnable,
  type DeviceReadinessDoctor,
} from "./device-readiness"

// EXP-1196: the fixture IS the spec — every case renders its rows, its
// pills (local: every action; remote: only remote ones) and its primary.

interface Case {
  name: string
  doctor: DeviceReadinessDoctor
  runnable: string[]
  localPrimary: string | null
  remotePills: Record<string, string>
}
const cases = spec.cases as unknown as Case[]

const visibleKeys = (doctor: DeviceReadinessDoctor) => {
  const parentOff = (key: string) =>
    doctor.items.find((item) => item.key === key)?.state === `off`
  return doctor.items
    .filter((item) => !item.parent || !parentOff(item.parent))
    .map((item) => item.key)
}

const pillsOf = (container: HTMLElement) =>
  Object.fromEntries(
    [...container.querySelectorAll(`[data-slot=pill]`)].map((pill) => [
      pill.closest(`[data-item]`)!.getAttribute(`data-item`)!,
      pill.getAttribute(`data-action`)!,
    ])
  )

const primaryOf = (container: HTMLElement) => {
  const primaries = [...container.querySelectorAll(`[data-slot=pill]`)].filter((pill) =>
    pill.className.includes(`bg-primary `)
  )
  expect(primaries.length).toBeLessThanOrEqual(1)
  return primaries[0]?.closest(`[data-item]`)?.getAttribute(`data-item`) ?? null
}

describe.each(cases)(`device-doctor.json: $name`, (fixtureCase) => {
  const { doctor } = fixtureCase

  it(`renders one band per used group with the tag, rows in order, the switch row`, () => {
    const { container } = render(
      <DeviceReadiness doctor={doctor} remote={false} onAction={() => {}} />
    )
    const bands = [...container.querySelectorAll(`[data-group]`)]
    const usedGroups = spec.groups.filter((group) =>
      doctor.items.some((item) => item.group === group.key)
    )
    expect(bands.map((band) => band.getAttribute(`data-group`))).toEqual(
      usedGroups.map((group) => group.key)
    )
    bands.forEach((band, index) => {
      const group = usedGroups[index]!
      const header = band.querySelector(`[data-slot=glass-section-header]`)!
      expect(header.textContent).toBe(`${group.label}${group.tag ?? ``}`)
    })
    // Glyph rows + the switch row, in the item order, children hidden while off.
    const rendered = [
      ...container.querySelectorAll(`[data-item], [data-slot=glass-toggle-row]`),
    ].map((row) => row.getAttribute(`data-item`) ?? `computer_use`)
    expect(rendered).toEqual(visibleKeys(doctor))
    // The switch reflects the state and carries no description.
    const toggle = container.querySelector(`[data-slot=glass-toggle-row]`)!
    expect(toggle.textContent).toBe(spec.labels.computer_use)
    const cu = doctor.items.find((item) => item.key === `computer_use`)!
    expect(toggle.querySelector(`[role=switch]`)!.getAttribute(`aria-checked`)).toBe(
      String(cu.state !== `off`)
    )
    // Labels + details come from the fixture / the device.
    for (const item of doctor.items) {
      const row = container.querySelector(`[data-item=${item.key}]`)
      if (!row) continue
      expect(row.textContent).toContain(
        (spec.labels as Record<string, string>)[item.key]
      )
      if (item.detail) expect(row.textContent).toContain(item.detail)
    }
  })

  it(`local: every action is a pill, the primary is the first action/error row`, () => {
    const { container } = render(
      <DeviceReadiness doctor={doctor} remote={false} onAction={() => {}} />
    )
    const expected = Object.fromEntries(
      doctor.items
        .filter((item) => item.action && visibleKeys(doctor).includes(item.key))
        .map((item) => [item.key, item.action!])
    )
    expect(pillsOf(container)).toEqual(expected)
    expect(primaryOf(container)).toBe(fixtureCase.localPrimary)
  })

  it(`remote: only remote actions are offered`, () => {
    const onAction = vi.fn()
    const { container } = render(
      <DeviceReadiness doctor={doctor} remote onAction={onAction} />
    )
    expect(pillsOf(container)).toEqual(fixtureCase.remotePills)
    const first = Object.entries(fixtureCase.remotePills)[0]
    if (first) {
      fireEvent.click(container.querySelector(`[data-item=${first[0]}] [data-slot=pill]`)!)
      expect(onAction).toHaveBeenCalledWith(first[0], first[1])
    }
  })

  it(`runnable agents match the fixture`, () => {
    const runnable = [`claude`, `codex`].filter((agent) =>
      deviceReadinessRunnable(doctor, agent)
    )
    expect(runnable).toEqual(fixtureCase.runnable)
  })

  it(`single-row mode renders only the composer's blocker, without bands`, () => {
    for (const agent of [`claude`, `codex`]) {
      const blocker = deviceReadinessBlocker(doctor, agent)
      if (fixtureCase.runnable.includes(agent)) {
        expect(blocker).toBeNull()
        continue
      }
      expect(blocker).not.toBeNull()
      const { container, unmount } = render(
        <DeviceReadiness doctor={doctor} remote={false} only={[blocker!]} onAction={() => {}} />
      )
      expect(container.querySelectorAll(`[data-group]`)).toHaveLength(0)
      expect(
        [...container.querySelectorAll(`[data-item]`)].map((row) => row.getAttribute(`data-item`))
      ).toEqual([blocker])
      unmount()
    }
  })
})

describe(`DeviceReadiness edge rules`, () => {
  it(`renders nothing for an older build (doctor = null)`, () => {
    const { container } = render(
      <DeviceReadiness doctor={null} remote onAction={() => {}} />
    )
    expect(container.innerHTML).toBe(``)
  })

  it(`skips vocabulary it cannot name instead of showing it raw`, () => {
    const groups = deviceReadinessModel(
      {
        checkedAt: `x`,
        items: [
          { key: `git`, group: `required`, state: `ok` },
          { key: `future`, group: `required`, state: `ok` },
          { key: `codex`, group: `agents`, state: `degraded` },
          { key: `claude`, group: `agents`, state: `action`, action: `repair` },
        ],
      },
      { remote: false }
    )
    expect(groups.flatMap((group) => group.rows.map((row) => [row.key, row.action]))).toEqual([
      [`git`, null],
      [`claude`, null],
    ])
  })

  it(`the switch's live value shows the permission rows before the report catches up`, () => {
    const doctor = cases[0]!.doctor
    const withChild: DeviceReadinessDoctor = {
      ...doctor,
      items: [
        ...doctor.items,
        { key: `accessibility`, group: `computer_use`, parent: `computer_use`, state: `action`, detail: `Not granted`, action: `grant` },
      ],
    }
    const keys = (on: boolean) =>
      deviceReadinessModel(withChild, { remote: false, computerUseOn: on })
        .flatMap((group) => group.rows)
        .map((row) => row.key)
    expect(keys(false)).not.toContain(`accessibility`)
    expect(keys(true)).toContain(`accessibility`)
  })

  it(`problemsOnly collapses to the rows that need something`, () => {
    const doctor = cases[2]!.doctor
    const { container } = render(
      <DeviceReadiness doctor={doctor} remote problemsOnly onAction={() => {}} />
    )
    expect(
      [...container.querySelectorAll(`[data-item]`)].map((row) => row.getAttribute(`data-item`))
    ).toEqual([`git`, `claude`])
  })
})
