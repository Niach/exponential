import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// The bulk bar's play pill: one press starts the selection on the composer,
// with no menu in between.

const openComposer = vi.hoisted(() => vi.fn())

vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})
vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  issueLabelCollection: {},
}))
vi.mock(`@/hooks/use-remote-start`, () => ({
  useRemoteStart: () => ({
    devices: [{ deviceId: `dev-1`, deviceLabel: `buildbox`, online: true }],
    starting: false,
    sentTo: null,
  }),
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/hooks/use-open-composer`, () => ({
  useOpenComposer: () => openComposer,
}))

import { BulkStartCodingControl } from "@/components/bulk-action-bar"

function mount(issueIds: string[]) {
  const onClear = vi.fn()
  render(
    <BulkStartCodingControl
      teamId="t1"
      currentUserId="u1"
      issueIds={issueIds}
      onClear={onClear}
    />
  )
  return { onClear, pill: screen.getByTestId(`bulk-start-coding`) }
}

beforeEach(() => {
  openComposer.mockReset()
})

describe(`BulkStartCodingControl`, () => {
  it(`is a plain pill: no menu opens behind it`, () => {
    const { pill } = mount([`i1`, `i2`])
    expect(pill.textContent).toBe(`Start coding`)
    expect(pill.getAttribute(`aria-haspopup`)).toBeNull()
    // Radix opens a menu on pointerdown; nothing may answer to it.
    fireEvent.pointerDown(pill, { button: 0, pointerType: `mouse` })
    expect(screen.queryByRole(`menu`)).toBeNull()
    expect(openComposer).not.toHaveBeenCalled()
  })

  it(`starts a batch on the composer and clears the selection`, () => {
    const { onClear, pill } = mount([`i1`, `i2`])
    fireEvent.click(pill)
    expect(openComposer).toHaveBeenCalledWith({ issueIds: [`i1`, `i2`] })
    expect(onClear).toHaveBeenCalled()
  })

  it(`starts a single issue the same way`, () => {
    const { onClear, pill } = mount([`i1`])
    fireEvent.click(pill)
    expect(openComposer).toHaveBeenCalledWith({ issueIds: [`i1`] })
    expect(onClear).toHaveBeenCalled()
  })
})
