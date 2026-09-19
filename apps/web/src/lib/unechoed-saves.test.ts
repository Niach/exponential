import { describe, expect, it } from "vitest"
import {
  resolveIncomingDescription,
  UnechoedSaves,
  UNECHOED_SAVE_TTL_MS,
} from "@/lib/unechoed-saves"

// EXP-928 — the web twin of the desktop `UnechoedSaves`
// (`crates/ui/src/issue_detail.rs`): until a save's Electric echo lands, the
// synced row still reads what the save replaced, and every reader that trusts
// it reverts the save.
describe(`UnechoedSaves`, () => {
  it(`tracks the row until it catches up`, () => {
    const saves = new UnechoedSaves()
    // Nothing recorded: the row is the truth.
    expect(saves.resolve(`a`, `old`)).toBeNull()

    // Two saves before either echo: the pre-save row AND the first save are
    // both stale while the second is in flight.
    saves.record(`a`, `one`, [`old`])
    saves.record(`a`, `two`, [`one`])
    expect(saves.resolve(`a`, `old`)).toBe(`two`)
    expect(saves.resolve(`a`, `one`)).toBe(`two`)
    // …and once the row reached `one`, it can never go back to `old`.
    expect(saves.resolve(`a`, `old`)).toBeNull()

    // The echo retires the entry.
    saves.record(`a`, `three`, [`two-ish`])
    expect(saves.resolve(`a`, `three`)).toBeNull()
    expect(saves.resolve(`a`, `two-ish`)).toBeNull()
  })

  it(`needs no entry when the save changes nothing, and a remote write retires one`, () => {
    const saves = new UnechoedSaves()
    saves.record(`b`, `same`, [`same`, `same`])
    expect(saves.resolve(`b`, `same`)).toBeNull()
    expect(saves.resolve(`b`, `old`)).toBeNull()

    saves.record(`c`, `mine`, [`old`])
    // Somebody else wrote something newer than either text: the row wins, for
    // good.
    expect(saves.resolve(`c`, `theirs`)).toBeNull()
    expect(saves.resolve(`c`, `old`)).toBeNull()
  })

  it(`takes the view's baseline as stale too, not just the row`, () => {
    const saves = new UnechoedSaves()
    // The blur save records the synced row AND the editor baseline it sat on
    // (an applied remote value the row has not echoed back yet, say).
    saves.record(`a`, `saved`, [`row`, `baseline`])
    expect(saves.resolve(`a`, `baseline`)).toBe(`saved`)
    expect(saves.resolve(`a`, `row`)).toBeNull()
  })

  it(`retires a failed save's own entry but never a newer one`, () => {
    const saves = new UnechoedSaves()
    saves.record(`a`, `mine`, [`old`])
    expect(saves.resolve(`a`, `old`)).toBe(`mine`)
    saves.retire(`a`, `mine`)
    expect(saves.resolve(`a`, `old`)).toBeNull()

    // A second save landed before the first one's failure came back: the
    // failure retires NOTHING (the entry belongs to the newer write).
    saves.record(`b`, `one`, [`old`])
    saves.record(`b`, `two`, [`old`])
    saves.retire(`b`, `one`)
    expect(saves.resolve(`b`, `old`)).toBe(`two`)

    // Retiring an issue with nothing in flight is a no-op.
    saves.retire(`c`, `whatever`)
    expect(saves.resolve(`c`, `old`)).toBeNull()
  })

  it(`expires an un-echoed save after its TTL`, () => {
    const saves = new UnechoedSaves()
    const start = 1_000_000
    saves.record(`a`, `mine`, [`old`], start)
    // Well inside the window: still the in-flight truth.
    expect(saves.resolve(`a`, `old`, start + 59_000)).toBe(`mine`)
    // Past it: the row wins again, and the entry is gone for good.
    expect(saves.resolve(`a`, `old`, start + UNECHOED_SAVE_TTL_MS)).toBeNull()
    expect(saves.resolve(`a`, `old`, start)).toBeNull()

    // Each save re-stamps the entry, so a steady stream never expires
    // mid-flight.
    saves.record(`b`, `one`, [`old`], start)
    saves.record(`b`, `two`, [`one`], start + 59_000)
    expect(saves.resolve(`b`, `old`, start + 90_000)).toBe(`two`)
    // A retry of the same save re-stamps it too.
    saves.record(`b`, `two`, [`one`], start + 110_000)
    expect(saves.resolve(`b`, `old`, start + 150_000)).toBe(`two`)
  })
})

// The one decision the detail view makes out of the store: what text to treat
// as this issue's synced description.
describe(`resolveIncomingDescription`, () => {
  it(`hands back the row when nothing is in flight`, () => {
    const saves = new UnechoedSaves()
    expect(resolveIncomingDescription(saves, `i1`, `  row text  `)).toBe(
      `  row text  `
    )
  })

  it(`hands back the un-echoed save while the row is still stale`, () => {
    const saves = new UnechoedSaves()
    saves.record(`i1`, `new text`, [`old text`])
    // The row (still the pre-save text, however it is spaced) resolves to the
    // save, normalized like every save.
    expect(resolveIncomingDescription(saves, `i1`, `\nold text\n`)).toBe(
      `new text`
    )
  })

  it(`falls back to the row on the echo and on a newer remote write`, () => {
    const saves = new UnechoedSaves()
    saves.record(`i1`, `new text`, [`old text`])
    expect(resolveIncomingDescription(saves, `i1`, `new text`)).toBe(`new text`)
    // Retired by that echo: the row is the truth again.
    expect(resolveIncomingDescription(saves, `i1`, `old text`)).toBe(`old text`)

    saves.record(`i2`, `mine`, [`old`])
    expect(resolveIncomingDescription(saves, `i2`, `theirs`)).toBe(`theirs`)
  })

  it(`keeps its issues apart`, () => {
    const saves = new UnechoedSaves()
    saves.record(`i1`, `mine`, [`old`])
    expect(resolveIncomingDescription(saves, `i2`, `old`)).toBe(`old`)
    expect(resolveIncomingDescription(saves, `i1`, `old`)).toBe(`mine`)
  })
})
