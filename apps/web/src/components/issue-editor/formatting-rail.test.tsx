import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Editor } from "@tiptap/react"
import {
  FormattingRail,
  issueRefInsertionText,
} from "@/components/issue-editor/formatting-rail"

// EXP-568 — the rail replaces the always-on toolbar on all four clients, so
// its button ORDER is a cross-client contract, not a styling detail. These
// tests lock the canonical order per platform, the mode transitions, and the
// `#` token rule the autocomplete depends on.

interface Recorder {
  commands: string[]
  inserted: string[]
}

function makeEditor(options?: {
  active?: Record<string, boolean>
  /** The character immediately before the caret; undefined = paragraph start. */
  charBefore?: string
  linkHref?: string
}) {
  const rec: Recorder = { commands: [], inserted: [] }
  const tr = {
    insertText: (text: string) => {
      rec.inserted.push(text)
    },
  }
  const chain: Record<string, unknown> = {}
  const record =
    (name: string) =>
    (..._args: unknown[]) => {
      rec.commands.push(name)
      return chain
    }
  for (const name of [
    `focus`,
    `toggleBulletList`,
    `toggleOrderedList`,
    `toggleTaskList`,
    `toggleBlockquote`,
    `toggleCode`,
    `setParagraph`,
    `toggleHeading`,
    `toggleBold`,
    `toggleItalic`,
    `toggleStrike`,
    `unsetAllMarks`,
    `clearNodes`,
    `extendMarkRange`,
    `setLink`,
    `unsetLink`,
  ]) {
    chain[name] = record(name)
  }
  chain.command = (fn: (props: { tr: typeof tr }) => boolean) => {
    fn({ tr })
    rec.commands.push(`command`)
    return chain
  }
  chain.run = () => true

  const active = options?.active ?? {}
  const charBefore = options?.charBefore
  const editor = {
    state: {
      selection: {
        $from: {
          parent: { isTextblock: true, textBetween: () => charBefore ?? `` },
          parentOffset: charBefore === undefined ? 0 : 3,
        },
      },
    },
    chain: () => chain,
    isActive: (name: string, attrs?: { level?: number }) =>
      Boolean(active[attrs?.level ? `${name}${attrs.level}` : name]),
    getAttributes: () => ({ href: options?.linkHref ?? `` }),
    commands: {
      blur: () => {
        rec.commands.push(`blur`)
        return true
      },
    },
  } as unknown as Editor

  return { editor, rec }
}

const imageUpload = {
  enabled: true,
  onFiles: async () => {},
  onOtherFiles: () => {},
}

function railLabels(container: HTMLElement) {
  return Array.from(container.querySelectorAll(`button`)).map((button) =>
    button.getAttribute(`aria-label`)
  )
}

describe(`FormattingRail button order`, () => {
  it(`draws the canonical desktop main row`, () => {
    const { editor } = makeEditor()
    const { container } = render(
      <FormattingRail
        editor={editor}
        imageUpload={imageUpload}
        platform="desktop"
        mode="main"
        onModeChange={vi.fn()}
      />
    )
    expect(railLabels(container)).toEqual([
      `Insert emoji`,
      `Insert image`,
      `Attach file`,
      `Insert issue reference`,
      `Link`,
      `Text formatting`,
      `Lists`,
      `Quote`,
      `Code`,
    ])
  })

  it(`collapses image+file into one menu on mobile and adds keyboard-down`, () => {
    const { editor } = makeEditor()
    const { container } = render(
      <FormattingRail
        editor={editor}
        imageUpload={imageUpload}
        platform="mobile"
        mode="main"
        onModeChange={vi.fn()}
        onDismissKeyboard={vi.fn()}
      />
    )
    expect(railLabels(container)).toEqual([
      `Insert emoji`,
      `Insert image or file`,
      `Insert issue reference`,
      `Link`,
      `Text formatting`,
      `Lists`,
      `Quote`,
      `Code`,
      `Hide keyboard`,
    ])
  })

  it(`drops the attach button when the host has no Files destination`, () => {
    const { editor } = makeEditor()
    const { container } = render(
      <FormattingRail
        editor={editor}
        imageUpload={{ enabled: true, onFiles: async () => {} }}
        platform="desktop"
        mode="main"
        onModeChange={vi.fn()}
      />
    )
    expect(railLabels(container)).not.toContain(`Attach file`)
    expect(railLabels(container)).toContain(`Insert image`)
  })

  it(`drops both pickers when uploads are off`, () => {
    const { editor } = makeEditor()
    const { container } = render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="main"
        onModeChange={vi.fn()}
      />
    )
    const labels = railLabels(container)
    expect(labels).not.toContain(`Insert image`)
    expect(labels).not.toContain(`Attach file`)
  })

  it(`has no keyboard-down button on desktop`, () => {
    const { editor } = makeEditor()
    const { container } = render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="main"
        onModeChange={vi.fn()}
      />
    )
    expect(railLabels(container)).not.toContain(`Hide keyboard`)
  })
})

describe(`FormattingRail modes`, () => {
  it(`asks the host for the text mode, and back again`, () => {
    const onModeChange = vi.fn()
    const { editor } = makeEditor()
    const { rerender, container } = render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="main"
        onModeChange={onModeChange}
      />
    )
    fireEvent.click(screen.getByLabelText(`Text formatting`))
    expect(onModeChange).toHaveBeenCalledWith(`text`)

    rerender(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="text"
        onModeChange={onModeChange}
      />
    )
    expect(railLabels(container)).toEqual([
      `Back`,
      `Text`,
      `Heading 1`,
      `Heading 2`,
      `Heading 3`,
      `Bold`,
      `Italic`,
      `Strikethrough`,
      `Clear formatting`,
    ])

    fireEvent.click(screen.getByLabelText(`Back`))
    expect(onModeChange).toHaveBeenCalledWith(`main`)
  })

  it(`marks the paragraph button active only outside headings and lists`, () => {
    const { editor } = makeEditor({ active: { paragraph: true } })
    render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="text"
        onModeChange={vi.fn()}
      />
    )
    expect(screen.getByLabelText(`Text`).className).toContain(`is-active`)

    const listy = makeEditor({ active: { paragraph: true, bulletList: true } })
    render(
      <FormattingRail
        editor={listy.editor}
        platform="desktop"
        mode="text"
        onModeChange={vi.fn()}
      />
    )
    expect(screen.getAllByLabelText(`Text`)[1].className).not.toContain(
      `is-active`
    )
  })

  it(`edits a link and hands the mode back on apply`, () => {
    const onModeChange = vi.fn()
    const { editor, rec } = makeEditor({ linkHref: `https://old.example` })
    render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="link"
        onModeChange={onModeChange}
      />
    )
    const input = screen.getByLabelText(`Link URL`) as HTMLInputElement
    expect(input.value).toBe(`https://old.example`)

    fireEvent.change(input, { target: { value: ` https://new.example ` } })
    fireEvent.click(screen.getByLabelText(`Apply link`))
    expect(rec.commands).toContain(`setLink`)
    expect(onModeChange).toHaveBeenCalledWith(`main`)
  })

  it(`unsets the link when the URL is cleared`, () => {
    const { editor, rec } = makeEditor({ linkHref: `https://old.example` })
    render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="link"
        onModeChange={vi.fn()}
      />
    )
    fireEvent.change(screen.getByLabelText(`Link URL`), {
      target: { value: `` },
    })
    fireEvent.keyDown(screen.getByLabelText(`Link URL`), { key: `Enter` })
    expect(rec.commands).toContain(`unsetLink`)
    expect(rec.commands).not.toContain(`setLink`)
  })

  it(`leaves the document alone when the link edit is cancelled`, () => {
    const onModeChange = vi.fn()
    const { editor, rec } = makeEditor()
    render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="link"
        onModeChange={onModeChange}
      />
    )
    fireEvent.keyDown(screen.getByLabelText(`Link URL`), { key: `Escape` })
    expect(onModeChange).toHaveBeenCalledWith(`main`)
    expect(rec.commands).not.toContain(`setLink`)
    expect(rec.commands).not.toContain(`unsetLink`)
  })
})

describe(`issueRefInsertionText`, () => {
  it(`inserts a bare # at a paragraph start`, () => {
    expect(issueRefInsertionText(undefined)).toBe(`#`)
  })

  it(`inserts a bare # after whitespace`, () => {
    expect(issueRefInsertionText(` `)).toBe(`#`)
    expect(issueRefInsertionText(`\n`)).toBe(`#`)
  })

  it(`prepends a space mid-word, so the autocomplete still triggers`, () => {
    expect(issueRefInsertionText(`x`)).toBe(` #`)
    expect(issueRefInsertionText(`)`)).toBe(` #`)
  })
})

describe(`FormattingRail issue-ref button`, () => {
  it(`inserts a bare # at a paragraph start`, () => {
    const { editor, rec } = makeEditor()
    render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="main"
        onModeChange={vi.fn()}
      />
    )
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(rec.inserted).toEqual([`#`])
  })

  it(`prepends a space when the caret sits against a word`, () => {
    const { editor, rec } = makeEditor({ charBefore: `x` })
    render(
      <FormattingRail
        editor={editor}
        platform="desktop"
        mode="main"
        onModeChange={vi.fn()}
      />
    )
    fireEvent.click(screen.getByLabelText(`Insert issue reference`))
    expect(rec.inserted).toEqual([` #`])
  })
})

describe(`FormattingRail keyboard dismissal`, () => {
  it(`blurs the editor through the host callback`, () => {
    const onDismissKeyboard = vi.fn()
    const { editor } = makeEditor()
    render(
      <FormattingRail
        editor={editor}
        platform="mobile"
        mode="main"
        onModeChange={vi.fn()}
        onDismissKeyboard={onDismissKeyboard}
      />
    )
    fireEvent.click(screen.getByLabelText(`Hide keyboard`))
    expect(onDismissKeyboard).toHaveBeenCalled()
  })
})
