import { afterEach, describe, expect, it } from "vitest"
import { isNewTabChord } from "@/components/team/work-tabs-sync"

// Review #22: Alt/Option+T opens the New session tab, but never eats a
// keystroke meant for a field, an IME composition or an open dialog.

function press(target: EventTarget, init: KeyboardEventInit = {}): KeyboardEvent {
  let seen: KeyboardEvent | null = null
  const listener = (event: Event) => {
    seen = event as KeyboardEvent
  }
  window.addEventListener(`keydown`, listener)
  target.dispatchEvent(
    new KeyboardEvent(`keydown`, { code: `KeyT`, altKey: true, bubbles: true, ...init })
  )
  window.removeEventListener(`keydown`, listener)
  return seen!
}

afterEach(() => {
  document.body.innerHTML = ``
})

describe(`isNewTabChord`, () => {
  it(`fires for Alt+T on the page`, () => {
    expect(isNewTabChord(press(document.body))).toBe(true)
  })

  it(`ignores other chords`, () => {
    expect(isNewTabChord(press(document.body, { altKey: false }))).toBe(false)
    expect(isNewTabChord(press(document.body, { metaKey: true }))).toBe(false)
    expect(isNewTabChord(press(document.body, { shiftKey: true }))).toBe(false)
    expect(isNewTabChord(press(document.body, { code: `KeyR` }))).toBe(false)
  })

  it(`skips an editable target`, () => {
    const input = document.createElement(`input`)
    const textarea = document.createElement(`textarea`)
    const editor = document.createElement(`div`)
    editor.setAttribute(`contenteditable`, `true`)
    const inner = document.createElement(`p`)
    editor.append(inner)
    document.body.append(input, textarea, editor)
    expect(isNewTabChord(press(input))).toBe(false)
    expect(isNewTabChord(press(textarea))).toBe(false)
    expect(isNewTabChord(press(inner))).toBe(false)
  })

  it(`skips an IME composition`, () => {
    expect(isNewTabChord(press(document.body, { isComposing: true }))).toBe(false)
  })

  it(`skips while a dialog is open`, () => {
    const dialog = document.createElement(`div`)
    dialog.setAttribute(`role`, `dialog`)
    document.body.append(dialog)
    expect(isNewTabChord(press(document.body))).toBe(false)
  })
})
