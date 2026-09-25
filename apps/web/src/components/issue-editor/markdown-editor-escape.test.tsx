import { createRef } from "react"
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"
import { act, render, waitFor } from "@testing-library/react"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"

// EXP-966: inside a dialog, Radix's document-level CAPTURE Escape listener
// preventDefaults the key while the @/#/: menu is open (so the dialog stays),
// and ProseMirror ignores every defaultPrevented event — the menu used to stay
// open. The editor must close it from the skipped Escape itself.

vi.mock(`@/components/mention-provider`, () => {
  const member = {
    id: `u1`,
    name: `Ada Lovelace`,
    email: `ada@example.dev`,
    image: null,
  }
  const value = {
    resolve: () => null,
    search: () => [member],
  }
  return { useMentions: () => value }
})

const MENU = `[data-editor-autocomplete]`
const rect = { top: 0, bottom: 10, left: 0, right: 10, width: 10, height: 10 }
const originalRects = Range.prototype.getClientRects
const originalBox = Range.prototype.getBoundingClientRect

beforeAll(() => {
  // jsdom has no layout; coordsAtPos needs a caret rect to anchor the menu.
  Range.prototype.getClientRects = () =>
    [rect] as unknown as DOMRectList
  Range.prototype.getBoundingClientRect = () => rect as DOMRect
})

afterAll(() => {
  Range.prototype.getClientRects = originalRects
  Range.prototype.getBoundingClientRect = originalBox
})

const openMenu = async () => {
  const ref = createRef<MarkdownEditorRef>()
  const { container } = render(
    <MarkdownEditor ref={ref} markdown="" editable onChange={() => {}} />
  )
  await waitFor(() => {
    expect(container.querySelector(`.ProseMirror`)).toBeTruthy()
  })
  const dom = container.querySelector(`.ProseMirror`) as HTMLElement & {
    editor?: { commands: { insertContent: (text: string) => boolean } }
  }
  // Tiptap hangs the editor on its DOM node; insert like typing would.
  act(() => {
    ref.current?.focus()
    dom.editor?.commands.insertContent(`Ping @`)
  })
  await waitFor(() => {
    expect(document.querySelector(MENU)).toBeTruthy()
  })
  return dom
}

const pressEscape = (target: HTMLElement, prevented: boolean) => {
  const event = new KeyboardEvent(`keydown`, {
    key: `Escape`,
    bubbles: true,
    cancelable: true,
  })
  if (prevented) event.preventDefault()
  act(() => {
    target.dispatchEvent(event)
  })
}

describe(`MarkdownEditor autocomplete Escape`, () => {
  it(`closes the menu on a plain Escape`, async () => {
    const dom = await openMenu()
    pressEscape(dom, false)
    await waitFor(() => {
      expect(document.querySelector(MENU)).toBeNull()
    })
  })

  it(`closes the menu on an Escape a dialog already preventDefaulted`, async () => {
    const dom = await openMenu()
    pressEscape(dom, true)
    await waitFor(() => {
      expect(document.querySelector(MENU)).toBeNull()
    })
  })
})
