import { createRef } from "react"
import { describe, expect, it } from "vitest"
import { act, fireEvent, render, waitFor } from "@testing-library/react"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"

// The editor's presentation knobs (appearance, aria label). Read-only text is
// genUI's Markdown since #VAPP-258 (`components/read-markdown.tsx`).

const mountedContent = async (container: HTMLElement) => {
  await waitFor(() => {
    expect(container.querySelector(`.tiptap-content`)).toBeTruthy()
  })
}

describe(`MarkdownEditor appearance`, () => {
  it(`leaves the document defaults untouched`, async () => {
    const { container } = render(
      <MarkdownEditor
        markdown={`bare https://example.dev/x here\nsecond line`}
        editable={false}
        onChange={() => {}}
      />
    )
    await mountedContent(container)
    expect(container.querySelector(`.tiptap-chat`)).toBeFalsy()
    expect(
      container.querySelector(`[aria-label="Issue description"]`)
    ).toBeTruthy()
    expect(container.querySelector(`a`)).toBeFalsy()
    expect(container.querySelector(`br`)).toBeFalsy()
  })
})

// EXP-568: an editable instance mounts the floating selection rail (a TipTap
// BubbleMenu portaled to <body>). Its plugin registers against the live
// editor, so a wiring mistake here breaks every editable editor at mount.
describe(`MarkdownEditor formatting rail`, () => {
  it(`mounts an editable editor with the selection rail attached`, async () => {
    const { container, unmount } = render(
      <MarkdownEditor markdown={`hello`} onChange={() => {}} />
    )
    await mountedContent(container)
    // The rail's element stays DETACHED until a selection shows it (the
    // BubbleMenu plugin appends it to <body> on show), so the assertion here
    // is simply that registering the plugin against a live editor works and
    // the editable content came up.
    expect(
      container.querySelector(`.tiptap-content[contenteditable="true"]`)
    ).toBeTruthy()
    expect(document.querySelector(`[data-editor-rail]`)).toBeNull()
    unmount()
  })
})

// EXP-726: a GFM table must RENDER as a grid on every read-only surface
// (comment bodies, the agent feed, the description preview) — before this the
// `<table>` was dropped and the cells flattened into paragraphs.
describe(`MarkdownEditor tables`, () => {
  it(`renders a read-only table with header cells and alignment`, async () => {
    const { container } = render(
      <MarkdownEditor
        markdown={`| l | c |\n| :--- | :---: |\n| 1 | 2 |`}
        editable={false}
        onChange={() => {}}
      />
    )
    await mountedContent(container)
    await waitFor(() => {
      expect(container.querySelector(`table`)).toBeTruthy()
    })
    expect(container.querySelectorAll(`table th`).length).toBe(2)
    expect(container.querySelectorAll(`table td`).length).toBe(2)
    expect(
      container.querySelector(`table th[style*="text-align"]`)
    ).toBeTruthy()
    expect(
      container.querySelector(`table td[style*="text-align: center"]`)
    ).toBeTruthy()
    // The scroll container the wide-table rule hangs off.
    expect(container.querySelector(`.tableWrapper`)).toBeTruthy()
  })
})

// The issue detail view reuses ONE editor across issue switches and applies
// synced descriptions through setMarkdown. That replace used to be an undo
// step: Ctrl+Z brought back the previous document (another issue's, often
// empty) and the next blur saved it over the real description.
describe(`MarkdownEditor programmatic setMarkdown`, () => {
  const undo = (container: HTMLElement) =>
    act(() => {
      fireEvent.keyDown(container.querySelector(`.ProseMirror`)!, {
        key: `z`,
        code: `KeyZ`,
        ctrlKey: true,
      })
    })

  it(`is not undoable and drops the replaced document's history`, async () => {
    const ref = createRef<MarkdownEditorRef>()
    const { container } = render(
      <MarkdownEditor ref={ref} markdown="" onChange={() => {}} />
    )
    await waitFor(() => expect(ref.current?.getMarkdown()).toBe(``))
    act(() => ref.current!.insertImage({ alt: `a`, src: `/api/attachments/a` }))
    act(() => ref.current!.setMarkdown(`The long description`))

    undo(container)
    undo(container)

    expect(ref.current!.getMarkdown()).toBe(`The long description`)
  })

  it(`keeps later local edits undoable`, async () => {
    const ref = createRef<MarkdownEditorRef>()
    const { container } = render(
      <MarkdownEditor ref={ref} markdown="" onChange={() => {}} />
    )
    await waitFor(() => expect(ref.current?.getMarkdown()).toBe(``))
    act(() => ref.current!.setMarkdown(`The long description`))
    act(() => ref.current!.insertImage({ alt: `b`, src: `/api/attachments/b` }))
    expect(ref.current!.getMarkdown()).toContain(`/api/attachments/b`)

    undo(container)

    expect(ref.current!.getMarkdown()).toBe(`The long description`)
  })
})
