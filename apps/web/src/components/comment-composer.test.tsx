import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { CommentComposer } from "@/components/comment-composer"

// EXP-568 — the composer grew the same four insert affordances the editor rail
// carries, so the action row is a cross-client contract now. The `#` button in
// particular has to respect the autocomplete's token-start rule, or it inserts
// a dead character instead of opening the picker.

function renderComposer(props?: Partial<{ onEmptyBlur: () => void }>) {
  return render(
    <CommentComposer
      issueId="issue-1"
      users={[]}
      onSubmit={vi.fn().mockResolvedValue(undefined)}
      {...props}
    />
  )
}

describe(`CommentComposer action row`, () => {
  it(`offers image, file, issue-ref, emoji and send`, () => {
    renderComposer()
    for (const label of [
      `Add image`,
      `Attach files`,
      `Insert issue reference`,
      `Insert emoji`,
      `Send comment`,
    ]) {
      expect(screen.getByLabelText(label)).toBeTruthy()
    }
  })

  it(`disables send while there is nothing to post`, () => {
    renderComposer()
    const send = screen.getByLabelText(`Send comment`) as HTMLButtonElement
    expect(send.disabled).toBe(true)

    fireEvent.change(screen.getByPlaceholderText(`Leave a reply…`), {
      target: { value: `hi` },
    })
    expect(
      (screen.getByLabelText(`Send comment`) as HTMLButtonElement).disabled
    ).toBe(false)
  })

  it(`inserts a bare # into an empty composer`, () => {
    renderComposer()
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(
      (screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement)
        .value
    ).toBe(`#`)
  })

  it(`prepends a space when the caret sits against a word`, () => {
    renderComposer()
    const textarea = screen.getByPlaceholderText(
      `Leave a reply…`
    ) as HTMLTextAreaElement
    fireEvent.change(textarea, { target: { value: `see` } })
    textarea.setSelectionRange(3, 3)
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(
      (screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement)
        .value
    ).toBe(`see #`)
  })

  it(`does not prepend a space after existing whitespace`, () => {
    renderComposer()
    const textarea = screen.getByPlaceholderText(
      `Leave a reply…`
    ) as HTMLTextAreaElement
    fireEvent.change(textarea, { target: { value: `see ` } })
    textarea.setSelectionRange(4, 4)
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(
      (screen.getByPlaceholderText(`Leave a reply…`) as HTMLTextAreaElement)
        .value
    ).toBe(`see #`)
  })
})

describe(`CommentComposer onEmptyBlur`, () => {
  it(`fires when focus leaves an empty card`, () => {
    const onEmptyBlur = vi.fn()
    const { container } = renderComposer({ onEmptyBlur })
    const outside = document.createElement(`button`)
    document.body.appendChild(outside)

    fireEvent.blur(screen.getByPlaceholderText(`Leave a reply…`), {
      relatedTarget: outside,
    })
    expect(onEmptyBlur).toHaveBeenCalledTimes(1)
    expect(container).toBeTruthy()
    outside.remove()
  })

  it(`stays put while the card holds text`, () => {
    const onEmptyBlur = vi.fn()
    renderComposer({ onEmptyBlur })
    const textarea = screen.getByPlaceholderText(`Leave a reply…`)
    fireEvent.change(textarea, { target: { value: `draft` } })

    const outside = document.createElement(`button`)
    document.body.appendChild(outside)
    fireEvent.blur(textarea, { relatedTarget: outside })
    expect(onEmptyBlur).not.toHaveBeenCalled()
    outside.remove()
  })

  it(`ignores focus moves inside the card`, () => {
    const onEmptyBlur = vi.fn()
    renderComposer({ onEmptyBlur })
    fireEvent.blur(screen.getByPlaceholderText(`Leave a reply…`), {
      relatedTarget: screen.getByLabelText(`Insert emoji`),
    })
    expect(onEmptyBlur).not.toHaveBeenCalled()
  })

  it(`ignores the blur a file chooser causes`, () => {
    const onEmptyBlur = vi.fn()
    renderComposer({ onEmptyBlur })
    fireEvent.click(screen.getByLabelText(`Attach files`))

    const outside = document.createElement(`button`)
    document.body.appendChild(outside)
    fireEvent.blur(screen.getByPlaceholderText(`Leave a reply…`), {
      relatedTarget: outside,
    })
    expect(onEmptyBlur).not.toHaveBeenCalled()
    outside.remove()
  })
})
