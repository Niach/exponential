import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest"
import { EmojiPicker } from "@/components/emoji-picker"

// EXP-551 — the picker against a small dataset (the real generated JSON is
// exercised in lib/emoji.test.ts). Mocked at the module boundary lib/emoji.ts
// dynamic-imports, so the lazy path is the one under test.
vi.mock(`@/lib/emoji.generated.json`, () => ({
  default: {
    version: `test`,
    groups: [
      `Smileys & emotion`,
      `People & body`,
      `Animals & nature`,
      `Food & drink`,
      `Travel & places`,
      `Activities`,
      `Objects`,
      `Symbols`,
      `Flags`,
    ],
    emojis: [
      { u: `😀`, l: `grinning face`, g: 0, s: [`grinning`], t: [`happy`] },
      { u: `😄`, l: `grinning face with smiling eyes`, g: 0, s: [`smile`], t: [] },
      {
        u: `👍`,
        l: `thumbs up`,
        g: 1,
        s: [`+1`, `thumbsup`],
        t: [`yes`],
        k: [`👍🏻`, `👍🏼`, `👍🏽`, `👍🏾`, `👍🏿`],
      },
      { u: `🐛`, l: `bug`, g: 2, s: [`bug`], t: [`insect`] },
      { u: `🎉`, l: `party popper`, g: 5, s: [`tada`], t: [`celebration`] },
    ],
  },
}))

function memoryStorage(): Storage {
  const map = new Map<string, string>()
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => void map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  }
}

describe(`EmojiPicker`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })
  beforeEach(() => {
    window.localStorage.clear()
  })

  it(`renders the groups once the dataset loads and picks the unicode`, async () => {
    const onPick = vi.fn()
    render(<EmojiPicker onPick={onPick} />)
    await waitFor(() =>
      expect(screen.getByRole(`heading`, { name: `Smileys & emotion` })).toBeTruthy()
    )
    // Every group renders (an empty one simply has no cells).
    expect(screen.getByRole(`heading`, { name: `Flags` })).toBeTruthy()

    fireEvent.click(screen.getByRole(`button`, { name: `party popper` }))
    expect(onPick).toHaveBeenCalledWith(`🎉`, expect.objectContaining({ u: `🎉` }))
    // Recorded as a recent (base unicode).
    expect(JSON.parse(window.localStorage.getItem(`exp.emojiRecent`)!)).toEqual([
      `🎉`,
    ])
  })

  it(`shows a Recent section for previous picks`, async () => {
    window.localStorage.setItem(`exp.emojiRecent`, JSON.stringify([`🐛`, `🎉`]))
    render(<EmojiPicker onPick={() => {}} />)
    await waitFor(() =>
      expect(screen.getByRole(`heading`, { name: `Recent` })).toBeTruthy()
    )
    const recent = screen.getByRole(`region`, { name: `Recent` })
    const cells = recent.querySelectorAll(`button`)
    expect([...cells].map((c) => c.textContent)).toEqual([`🐛`, `🎉`])
  })

  it(`filters by search and Enter picks the first result`, async () => {
    const onPick = vi.fn()
    render(<EmojiPicker onPick={onPick} />)
    const search = await screen.findByLabelText(`Search emoji`)
    await act(async () => {
      fireEvent.change(search, { target: { value: `tad` } })
    })
    await waitFor(() =>
      expect(screen.getByRole(`button`, { name: `party popper` })).toBeTruthy()
    )
    expect(screen.queryByRole(`button`, { name: `bug` })).toBeNull()
    expect(screen.queryByRole(`heading`, { name: `Flags` })).toBeNull()

    fireEvent.keyDown(search, { key: `Enter` })
    expect(onPick).toHaveBeenCalledWith(`🎉`, expect.objectContaining({ u: `🎉` }))
  })

  it(`says so when nothing matches`, async () => {
    render(<EmojiPicker onPick={() => {}} />)
    const search = await screen.findByLabelText(`Search emoji`)
    await act(async () => {
      fireEvent.change(search, { target: { value: `zzz` } })
    })
    await waitFor(() => expect(screen.getByText(`No emoji found`)).toBeTruthy())
  })

  it(`applies and persists the skin tone`, async () => {
    const onPick = vi.fn()
    render(<EmojiPicker onPick={onPick} />)
    await screen.findByRole(`heading`, { name: `People & body` })
    fireEvent.click(screen.getByRole(`radio`, { name: `Medium skin tone` }))
    expect(window.localStorage.getItem(`exp.emojiSkinTone`)).toBe(`3`)
    const thumbs = screen.getByRole(`button`, { name: `thumbs up` })
    expect(thumbs.textContent).toBe(`👍🏽`)
    fireEvent.click(thumbs)
    // Toned unicode goes to the caller, the BASE is what recents remember.
    expect(onPick).toHaveBeenCalledWith(`👍🏽`, expect.objectContaining({ u: `👍` }))
    expect(JSON.parse(window.localStorage.getItem(`exp.emojiRecent`)!)).toEqual([
      `👍`,
    ])
    // Tone-less emoji are unaffected.
    expect(screen.getByRole(`button`, { name: `bug` }).textContent).toBe(`🐛`)
  })
})
