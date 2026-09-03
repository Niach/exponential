import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import { steerCommandsFor } from "@/lib/steer-commands"

// EXP-724 — the steering composer's `/` menu, driven the way MessageComposer
// drives it: the menu gets every keystroke first and reports whether it
// consumed one; only an unconsumed Enter sends.

function Harness({
  agent = `claude`,
  onSend,
  initial = ``,
}: {
  agent?: string
  onSend: (text: string) => void
  initial?: string
}) {
  const [text, setText] = useState(initial)
  const commands = steerCommandsFor(agent)
  const menu = useSlashCommandMenu({
    text,
    commands,
    onAccept: (next) => setText(next),
  })
  return (
    <div className="relative">
      <textarea
        aria-label="Message"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (menu.handleKeyDown(e)) return
          if (e.key === `Enter` && !e.shiftKey) {
            e.preventDefault()
            onSend(text)
          }
        }}
      />
      {menu.open && (
        <SlashCommandMenu
          commands={menu.candidates}
          active={menu.active}
          onSelect={menu.accept}
          onHover={menu.setActive}
        />
      )}
    </div>
  )
}

function type(el: HTMLTextAreaElement, text: string) {
  fireEvent.change(el, { target: { value: text } })
}

function field() {
  return screen.getByLabelText(`Message`) as HTMLTextAreaElement
}

describe(`SlashCommandMenu`, () => {
  it(`opens on /co and Enter inserts the command instead of sending`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    type(el, `/co`)
    expect(screen.getByText(`/compact`)).toBeTruthy()
    expect(screen.getByText(`Compact the conversation context`)).toBeTruthy()
    // The argument placeholder is shown, muted.
    expect(screen.getByText(`<instructions>`)).toBeTruthy()
    fireEvent.keyDown(el, { key: `Enter` })
    expect(onSend).not.toHaveBeenCalled()
    expect(el.value).toBe(`/compact `)
  })

  it(`Tab accepts too, and never sends`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    type(el, `/co`)
    fireEvent.keyDown(el, { key: `Tab` })
    expect(onSend).not.toHaveBeenCalled()
    expect(el.value).toBe(`/compact `)
  })

  it(`a no-argument command is accepted bare and the menu closes so Enter sends`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    type(el, `/cl`)
    fireEvent.keyDown(el, { key: `Enter` })
    expect(el.value).toBe(`/clear`)
    // The row is gone (its description is the menu's own text, unlike the
    // `/clear` token the textarea now also carries).
    expect(
      screen.queryByText(`Start a fresh conversation (context is discarded)`)
    ).toBeNull()
    fireEvent.keyDown(el, { key: `Enter` })
    expect(onSend).toHaveBeenCalledWith(`/clear`)
  })

  it(`Escape closes the menu and the next Enter sends the draft`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    type(el, `/co`)
    fireEvent.keyDown(el, { key: `Escape` })
    expect(screen.queryByText(`/compact`)).toBeNull()
    fireEvent.keyDown(el, { key: `Enter` })
    expect(onSend).toHaveBeenCalledWith(`/co`)
    // Typing again offers the menu back.
    type(el, `/com`)
    expect(screen.getByText(`/compact`)).toBeTruthy()
  })

  it(`arrows wrap around the candidate list`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    // The bare slash offers every claude command.
    type(el, `/`)
    const claude = steerCommandsFor(`claude`)
    for (const command of claude) {
      expect(screen.getByText(`/${command.name}`)).toBeTruthy()
    }
    fireEvent.keyDown(el, { key: `ArrowDown` })
    fireEvent.keyDown(el, { key: `Enter` })
    expect(el.value).toBe(`/${claude[1].name}${claude[1].argHint ? ` ` : ``}`)
    // ArrowUp from the top wraps to the last row.
    type(el, `/`)
    fireEvent.keyDown(el, { key: `ArrowUp` })
    fireEvent.keyDown(el, { key: `Enter` })
    const last = claude[claude.length - 1]
    expect(el.value).toBe(`/${last.name}${last.argHint ? ` ` : ``}`)
  })

  it(`lets Cmd/Ctrl+Enter fall through to the host`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    type(el, `/co`)
    fireEvent.keyDown(el, { key: `Enter`, metaKey: true })
    // The menu did not eat it: the host saw the keystroke, and nothing was
    // inserted into the draft.
    expect(onSend).toHaveBeenCalledWith(`/co`)
    expect(el.value).toBe(`/co`)
    expect(screen.getByText(`/compact`)).toBeTruthy()
  })

  it(`never opens for a slash that is not the whole draft`, () => {
    render(<Harness onSend={vi.fn()} />)
    const el = field()
    type(el, `hi /co`)
    expect(screen.queryByText(`/compact`)).toBeNull()
    type(el, `/compact now`)
    expect(screen.queryByText(`/compact`)).toBeNull()
  })

  it(`offers only the session agent's commands`, () => {
    render(<Harness agent="codex" onSend={vi.fn()} />)
    const el = field()
    type(el, `/`)
    // The two rows are every agent's; nothing outside the catalog shows.
    expect(screen.getByText(`/compact`)).toBeTruthy()
    expect(screen.getByText(`/clear`)).toBeTruthy()
    expect(screen.queryByText(`/new`)).toBeNull()
  })
})
