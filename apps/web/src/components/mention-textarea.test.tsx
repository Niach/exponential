import { createRef, useState } from "react"
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"

// EXP-551 — the comment composer's textarea: `:shortcode` typeahead, the
// send-shortcut guard, and the imperative caret insert the emoji picker uses.
// SLOP-28 — and the `#` trigger that opens THE search list (stubbed here: its
// own suite is @exp/ui's search-list.test.tsx), with the host's three exits.

const refs = vi.hoisted(() => ({ value: null as unknown }))
vi.mock(`@/components/issue-ref-provider`, () => ({
  useIssueRefs: () => refs.value,
}))
vi.mock(`@/components/issue-ref-search`, () => ({
  IssueRefSearch: (props: {
    request: { start: number; query: string } | null
    onPick: (issue: { identifier: string }) => void
    onDismiss: () => void
    onBackspaceEmpty: () => void
  }) =>
    props.request ? (
      <div
        data-testid="ref-search"
        data-start={props.request.start}
        data-query={props.request.query}
      >
        <button type="button" onClick={() => props.onPick({ identifier: `EXP-7` })}>
          pick
        </button>
        <button type="button" onClick={props.onDismiss}>
          dismiss
        </button>
        <button type="button" onClick={props.onBackspaceEmpty}>
          backspace
        </button>
      </div>
    ) : null,
}))

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

  // EXP-827: a caret offset INTO the inserted text — the chat suggestions
  // park the caret behind their `#` so the issue-ref menu opens at once.
  it(`insertText with a caret offset parks the caret inside the insert`, async () => {
    const handle = createRef<MentionTextareaHandle>()
    const values: string[] = []
    render(<Harness handle={handle} onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    const text = `Start a session for # on my other machine`
    act(() => handle.current!.insertText(text, text.indexOf(`#`) + 1))
    expect(values.at(-1)).toBe(text)
    await waitFor(() => expect(document.activeElement).toBe(el))
    expect(el.selectionStart).toBe(text.indexOf(`#`) + 1)
    // The issue-ref menu is what that caret is for: the token detector sees
    // an open `#` (no provider here, so no rows — just no crash).
    expect(el.selectionEnd).toBe(el.selectionStart)
  })
})

describe(`MentionTextarea #issue search`, () => {
  const withRefs = () => {
    refs.value = { teamId: `t`, rows: [] }
  }

  it(`opens THE search list on # with the token as its prefill; a pick writes #IDENT + space`, () => {
    withRefs()
    const values: string[] = []
    render(<Harness onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `see (#ab`)
    const search = screen.getByTestId(`ref-search`)
    expect(search.getAttribute(`data-start`)).toBe(`5`)
    expect(search.getAttribute(`data-query`)).toBe(`ab`)
    fireEvent.click(screen.getByText(`pick`))
    expect(values.at(-1)).toBe(`see (#EXP-7 `)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
  })

  it(`Escape keeps the text and the dismissed # stays closed while typing on`, () => {
    withRefs()
    const values: string[] = []
    render(<Harness onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `see #ab`)
    fireEvent.click(screen.getByText(`dismiss`))
    expect(values.at(-1)).toBe(`see #ab`)
    type(el, `see #abc`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
    type(el, `see #abc #`)
    expect(screen.getByTestId(`ref-search`).getAttribute(`data-start`)).toBe(`9`)
  })

  it(`Backspace in the empty input deletes the # token`, () => {
    withRefs()
    const values: string[] = []
    render(<Harness onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `see #`)
    fireEvent.click(screen.getByText(`backspace`))
    expect(values.at(-1)).toBe(`see `)
  })

  it(`an inserted # (the composer's # button) opens the search and leaves the focus to it`, async () => {
    withRefs()
    const handle = createRef<MentionTextareaHandle>()
    render(<Harness handle={handle} initial="see " />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    act(() => el.blur())
    el.setSelectionRange(4, 4)
    act(() => handle.current!.insertText(`#`))
    expect(screen.getByTestId(`ref-search`).getAttribute(`data-start`)).toBe(`4`)
    await act(() => new Promise((resolve) => requestAnimationFrame(resolve)))
    expect(document.activeElement).not.toBe(el)
    expect(el.selectionStart).toBe(5)
  })

  it(`a Backspace that lands after an existing #IDENT never opens it`, () => {
    withRefs()
    render(<Harness initial="see #EXP-7 " />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `see #EXP-7`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
  })

  // Review #5 (search-list.json): a line-start `#` waits for a token, so a
  // typed `# Title` stays a heading; `#E` opens with `E`.
  it(`a line-start # waits for a token character`, () => {
    withRefs()
    render(<Harness />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `#`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
    type(el, `# `)
    type(el, `# Title`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
    type(el, `Line one\n#`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
    type(el, `Line one\n#E`)
    const search = screen.getByTestId(`ref-search`)
    expect(search.getAttribute(`data-start`)).toBe(`9`)
    expect(search.getAttribute(`data-query`)).toBe(`E`)
  })

  it(`punctuation ends the token`, () => {
    withRefs()
    render(<Harness />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `see #a.`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
  })

  it(`a pick and a Backspace replace exactly # + the recorded token, never text after the caret`, () => {
    withRefs()
    const values: string[] = []
    render(<Harness onValue={(v) => values.push(v)} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    fireEvent.change(el, {
      target: { value: `see #ab docs`, selectionStart: 7, selectionEnd: 7 },
    })
    expect(screen.getByTestId(`ref-search`).getAttribute(`data-query`)).toBe(`ab`)
    fireEvent.click(screen.getByText(`pick`))
    expect(values.at(-1)).toBe(`see #EXP-7  docs`)

    fireEvent.change(el, { target: { value: ``, selectionStart: 0, selectionEnd: 0 } })
    fireEvent.change(el, {
      target: { value: `see #ab docs`, selectionStart: 7, selectionEnd: 7 },
    })
    fireEvent.click(screen.getByText(`backspace`))
    expect(values.at(-1)).toBe(`see  docs`)
  })

  it(`the # button opens the search on a bare line-start #`, () => {
    withRefs()
    const handle = createRef<MentionTextareaHandle>()
    render(<Harness handle={handle} />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    act(() => el.blur())
    act(() => handle.current!.insertText(`#`))
    expect(screen.getByTestId(`ref-search`).getAttribute(`data-start`)).toBe(`0`)
  })

  it(`a # glued to a word is no trigger`, () => {
    withRefs()
    render(<Harness />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `C#sharp`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
  })

  it(`stays inert outside a team`, () => {
    refs.value = null
    render(<Harness />)
    const el = screen.getByLabelText(`Reply`) as HTMLTextAreaElement
    type(el, `see #ab`)
    expect(screen.queryByTestId(`ref-search`)).toBeNull()
  })
})
