import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { COMPOSER_DRAFT_DEBOUNCE_MS } from "@exp/domain-contract/composer-draft"
import {
  flushAllComposerDrafts,
  localComposerDraft,
  resetComposerDrafts,
  setComposerDraftTransport,
  writeComposerDraft,
} from "./composer-drafts"

// #FEED-219 (composer-draft.json): the debounced, flushed draft writer.
describe(`composer draft writer`, () => {
  const upsert = vi.fn()
  const remove = vi.fn()
  let echo: Array<() => void> = []
  let fail = false

  beforeEach(() => {
    vi.useFakeTimers()
    resetComposerDrafts()
    upsert.mockReset()
    remove.mockReset()
    echo = []
    fail = false
    setComposerDraftTransport({
      upsert: async (input) => {
        upsert(input)
        if (fail) throw new Error(`offline`)
        return { txId: 1 }
      },
      delete: async (input) => {
        remove(input)
        return { txId: 2 }
      },
      awaitTxId: () => new Promise<void>((resolve) => echo.push(resolve)),
    })
  })
  afterEach(() => {
    vi.useRealTimers()
    setComposerDraftTransport(null)
  })

  it(`coalesces a burst into one write after the debounce`, async () => {
    writeComposerDraft(`session:s`, `t`, `h`)
    writeComposerDraft(`session:s`, `t`, `he`)
    writeComposerDraft(`session:s`, `t`, `hey`)
    await vi.advanceTimersByTimeAsync(COMPOSER_DRAFT_DEBOUNCE_MS - 1)
    expect(upsert).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(upsert).toHaveBeenCalledTimes(1)
    expect(upsert).toHaveBeenCalledWith({ teamId: `t`, key: `session:s`, text: `hey` })
  })

  it(`keeps the local copy until the write echoes`, async () => {
    writeComposerDraft(`session:s`, `t`, `hey`, { now: true })
    await vi.advanceTimersByTimeAsync(0)
    expect(localComposerDraft(`session:s`)).toBe(`hey`)
    echo.shift()!()
    await vi.advanceTimersByTimeAsync(0)
    expect(localComposerDraft(`session:s`)).toBeNull()
  })

  it(`an emptied field deletes the row`, async () => {
    writeComposerDraft(`comment:i`, `t`, `  `, { now: true })
    await vi.advanceTimersByTimeAsync(0)
    expect(remove).toHaveBeenCalledWith({ key: `comment:i` })
    expect(upsert).not.toHaveBeenCalled()
  })

  it(`text typed during a write is written after it`, async () => {
    writeComposerDraft(`session:s`, `t`, `one`, { now: true })
    await vi.advanceTimersByTimeAsync(0)
    writeComposerDraft(`session:s`, `t`, `one two`, { now: true })
    await vi.advanceTimersByTimeAsync(0)
    expect(upsert).toHaveBeenCalledTimes(1)
    echo.shift()!()
    await vi.advanceTimersByTimeAsync(0)
    expect(upsert).toHaveBeenCalledTimes(2)
    expect(upsert).toHaveBeenLastCalledWith({ teamId: `t`, key: `session:s`, text: `one two` })
  })

  it(`a failed write is silent, keeps the text and retries on the next flush`, async () => {
    fail = true
    writeComposerDraft(`session:s`, `t`, `hey`, { now: true })
    await vi.advanceTimersByTimeAsync(0)
    expect(localComposerDraft(`session:s`)).toBe(`hey`)
    fail = false
    flushAllComposerDrafts()
    await vi.advanceTimersByTimeAsync(0)
    expect(upsert).toHaveBeenCalledTimes(2)
  })

  it(`the page hiding flushes a debounced write at once`, async () => {
    writeComposerDraft(`newChat:t`, `t`, `plan`)
    flushAllComposerDrafts()
    await vi.advanceTimersByTimeAsync(0)
    expect(upsert).toHaveBeenCalledWith({ teamId: `t`, key: `newChat:t`, text: `plan` })
  })
})
