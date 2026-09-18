import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

// EXP-551 — the BINDING half: the picker's grid and popover are @exp/ui's
// (packages/ui/src/emoji-picker.test.tsx covers them), this file only checks
// what the app adds — the lazy dataset load on open and the per-device
// recents.

const useEmojiData = vi.hoisted(() => vi.fn())
const pushRecentEmoji = vi.hoisted(() => vi.fn(() => [`🎉`]))
const readRecentEmoji = vi.hoisted(() => vi.fn(() => [] as string[]))

vi.mock(`@/lib/emoji`, () => ({
  useEmojiData,
  pushRecentEmoji,
  readRecentEmoji,
}))

import { EmojiPickerPopover } from "@/components/emoji-picker"

const DATA = {
  dataset: {
    version: `test`,
    groups: [`Smileys & emotion`],
    emojis: [{ u: `🎉`, l: `party popper`, g: 0, s: [`tada`], t: [] }],
  },
  groups: [
    {
      index: 0,
      label: `Smileys & emotion`,
      emojis: [{ u: `🎉`, l: `party popper`, g: 0, s: [`tada`], t: [] }],
    },
  ],
  byShortcode: new Map(),
  byUnicode: new Map(),
}

describe(`EmojiPickerPopover (binding)`, () => {
  it(`loads the dataset only once opened, and records the pick`, async () => {
    useEmojiData.mockImplementation((enabled: boolean) =>
      enabled ? (DATA as never) : null
    )
    const onPick = vi.fn()
    render(
      <EmojiPickerPopover onPick={onPick}>
        <button type="button">Emoji</button>
      </EmojiPickerPopover>
    )
    // Closed: the hook is asked for nothing.
    expect(useEmojiData).toHaveBeenCalledWith(false)
    expect(readRecentEmoji).toHaveBeenCalled()

    fireEvent.click(screen.getByText(`Emoji`))
    await waitFor(() => expect(useEmojiData).toHaveBeenCalledWith(true))
    const cell = await screen.findByRole(`button`, { name: `party popper` })

    fireEvent.click(cell)
    expect(pushRecentEmoji).toHaveBeenCalledWith(`🎉`)
    expect(onPick).toHaveBeenCalledWith(`🎉`, expect.objectContaining({ u: `🎉` }))
  })
})
