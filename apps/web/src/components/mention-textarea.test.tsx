import { createRef, useState } from "react"
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"

// EXP-551 — the comment composer's textarea: `:shortcode` typeahead, the
// send-shortcut guard, and the imperative caret insert the emoji picker uses.

vi.mock(`@/lib/emoji.generated.json`, () => ({
  default: {
    version: `test`,
    groups: [`Smileys & emotion`, `People & body`, `Animals & nature`, `Food & drink`, `Travel & places`, `Activities`, `Objects`, `Symbols`, `Flags`],
    emojis: [
      { u: `🎉`, l: `party popper`, g: 5, s: [`tada`], t: [`celebration`] },
      { u: `😄`, l: `grinning face with smiling eyes`, g: 0, s: [`smile`], t: [] },
    ],
  },
}))

function Harness({
  onKeyDown,
  handle,
  initial = ``,
  onValue,
}: {
  onKeyDown?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void
  handle?: React.Ref<MentionTextareaHandle>
  initial?: string
  onValue?: (v: string) => void
}) {
  // A tiny stateful host, like CommentComposer.
  const [value, setValue] = useState(initial)
  return (
    <MentionTextarea
      ref={handle}
      users={[]}
      value={value}
      onValueChange={(next) => {
        setValue(next)
        onValue?.(next)
      }}
      onKeyDown={onKeyDown}
      aria-label="Reply"
    />
  )
}

function type(el: HTMLTextAreaElement, text: string) {
  fireEvent.change(el, {
    target: { value: text, selectionStart: text.length, selectionEnd: text.length },
  })
}

describe(`MentionTextarea :emoji`, () => {
  it(`opens the emoji menu on :xx and Enter inserts unicode + space`, async () => {
    const values: string[] = []
    render(<Harness onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `Ship it :ta`)
    await waitFor(() => expect(screen.getByText(`:tada:`)).toBeTruthy())
    // The rows show the label too.
    expect(screen.getByText(`party popper`)).toBeTruthy()
    el.setSelectionRange(11, 11)
    fireEvent.keyDown(el, { key: `Enter` })
    expect(values.at(-1)).toBe(`Ship it 🎉 `)
    // Menu gone.
    expect(screen.queryByText(`:tada:`)).toBeNull()
  })

  it(`lets Cmd/Ctrl+Enter through to the host while a menu is open`, async () => {
    const onKeyDown = vi.fn()
    const values: string[] = []
    render(<Harness onKeyDown={onKeyDown} onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `Ship it :ta`)
    await waitFor(() => expect(screen.getByText(`:tada:`)).toBeTruthy())
    fireEvent.keyDown(el, { key: `Enter`, metaKey: true })
    expect(onKeyDown).toHaveBeenCalledTimes(1)
    expect(values.at(-1)).toBe(`Ship it :ta`)
    fireEvent.keyDown(el, { key: `Enter`, ctrlKey: true })
    expect(onKeyDown).toHaveBeenCalledTimes(2)
  })

  it(`auto-commits an exact :shortcode: without a trailing space`, async () => {
    const values: string[] = []
    render(<Harness onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `Ship it :tada:`)
    await waitFor(() => expect(values.at(-1)).toBe(`Ship it 🎉`))
  })

  it(`does not open on 12:30 or :)`, async () => {
    render(<Harness />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `meet at 12:30`)
    type(el, `meet at 12:30 :)`)
    // Give any (wrong) lazy load a tick.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20))
    })
    expect(screen.queryByText(`party popper`)).toBeNull()
  })

  it(`insertText splices at the caret and re-focuses`, async () => {
    const handle = createRef<MentionTextareaHandle>()
    const values: string[] = []
    render(<Harness handle={handle} initial={`hello world`} onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    el.setSelectionRange(5, 5)
    act(() => handle.current!.insertText(`🎉`))
    expect(values.at(-1)).toBe(`hello🎉 world`)
    await waitFor(() => expect(document.activeElement).toBe(el))
    expect(el.selectionStart).toBe(`hello🎉`.length)
  })
})
