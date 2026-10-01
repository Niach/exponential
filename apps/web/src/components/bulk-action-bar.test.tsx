import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// The bulk bar's play MENU: one pill whose one item starts the selection as
// a batch on the composer.

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

import {
  BulkStartCodingControl,
  START_AS_BATCH_LABEL,
} from "@/components/bulk-action-bar"

/** Radix opens on pointerdown, not click (the repo's menu-test idiom). */
function openMenu(trigger: HTMLElement) {
  act(() => {
    fireEvent.pointerDown(trigger, {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

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
  openMenu(screen.getByTestId(`bulk-start-coding`))
  return { onClear }
}

beforeEach(() => {
  openComposer.mockReset()
})

describe(`BulkStartCodingControl`, () => {
  it(`offers Start as batch`, () => {
    mount([`i1`, `i2`])
    expect(screen.getByTestId(`bulk-start-batch`).textContent).toBe(
      START_AS_BATCH_LABEL
    )
  })

  it(`starts a batch on the composer and clears the selection`, () => {
    const { onClear } = mount([`i1`, `i2`])
    fireEvent.click(screen.getByTestId(`bulk-start-batch`))
    expect(openComposer).toHaveBeenCalledWith({ issueIds: [`i1`, `i2`] })
    expect(onClear).toHaveBeenCalled()
  })
})
