import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import { steerCommandsFor, type SlashPrompt } from "@/lib/steer-commands"

// EXP-724 — the steering composer's `/` menu, driven the way MessageComposer
// drives it: the menu gets every keystroke first and reports whether it
// consumed one; only an unconsumed Enter sends. NEW-2: the builtins
// (compact, model, effort) and then the team's prompts, in ONE menu.

const PROMPTS: SlashPrompt[] = [
  { id: `p1`, name: `Fix flaky tests`, icon: null },
  { id: `p2`, name: `Release train`, icon: `rocket` },
]

function Harness({
  agent = `claude`,
  onSend,
  initial = ``,
  prompts,
  onPickPrompt,
}: {
  agent?: string
  onSend: (text: string) => void
  initial?: string
  prompts?: SlashPrompt[]
  onPickPrompt?: (prompt: SlashPrompt) => void
}) {
  const [text, setText] = useState(initial)
  const commands = steerCommandsFor(agent)
  const menu = useSlashCommandMenu({
    text,
    commands,
    prompts,
    onAccept: (next) => setText(next),
    onPickPrompt,
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
          candidates={menu.candidates}
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

  it(`an accepted command closes the menu so the next Enter sends`, () => {
    const onSend = vi.fn()
    render(<Harness onSend={onSend} />)
    const el = field()
    type(el, `/mo`)
    fireEvent.keyDown(el, { key: `Enter` })
    expect(el.value).toBe(`/model `)
    expect(screen.queryByText(`Switch the model for this session`)).toBeNull()
    type(el, `/model opus`)
    fireEvent.keyDown(el, { key: `Enter` })
    expect(onSend).toHaveBeenCalledWith(`/model opus`)
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

  it(`offers only the contract builtins, never the CLI's own`, () => {
    render(<Harness agent="codex" onSend={vi.fn()} />)
    const el = field()
    type(el, `/`)
    expect(screen.getByText(`/compact`)).toBeTruthy()
    expect(screen.getByText(`/model`)).toBeTruthy()
    expect(screen.getByText(`/effort`)).toBeTruthy()
    // Thread 58cb32df: `/new` (a fresh conversation) is a builtin.
    expect(screen.getByText(`/new`)).toBeTruthy()
    // `/clear` and `/btw` are gone; nothing outside the catalog shows.
    expect(screen.queryByText(`/clear`)).toBeNull()
    expect(screen.queryByText(`/btw`)).toBeNull()
  })

  it(`lists the team's prompts after the builtins`, () => {
    render(<Harness onSend={vi.fn()} prompts={PROMPTS} onPickPrompt={vi.fn()} />)
    type(field(), `/`)
    const rows = screen.getAllByRole(`option`).map((row) => row.textContent ?? ``)
    expect(rows.slice(0, 4).map((text) => text.match(/^\/[a-z]+/)?.[0])).toEqual([
      `/compact`,
      `/new`,
      `/model`,
      `/effort`,
    ])
    expect(rows.slice(4)).toEqual([`Fix flaky tests`, `Release train`])
  })

  it(`filters prompts by name prefix, spaces as dashes`, () => {
    render(<Harness onSend={vi.fn()} prompts={PROMPTS} onPickPrompt={vi.fn()} />)
    type(field(), `/fix-fl`)
    expect(screen.getByText(`Fix flaky tests`)).toBeTruthy()
    expect(screen.queryByText(`Release train`)).toBeNull()
    expect(screen.queryByText(`/compact`)).toBeNull()
  })

  it(`Enter on a prompt hands it to the host and never sends`, () => {
    const onSend = vi.fn()
    const onPickPrompt = vi.fn()
    render(<Harness onSend={onSend} prompts={PROMPTS} onPickPrompt={onPickPrompt} />)
    const el = field()
    type(el, `/rel`)
    fireEvent.keyDown(el, { key: `Enter` })
    expect(onPickPrompt).toHaveBeenCalledWith(PROMPTS[1])
    expect(onSend).not.toHaveBeenCalled()
    // The menu stays shut on the draft it was picked on.
    expect(screen.queryByText(`Release train`)).toBeNull()
  })

  it(`offers no prompts without a pick handler`, () => {
    render(<Harness onSend={vi.fn()} prompts={PROMPTS} />)
    type(field(), `/`)
    expect(screen.queryByText(`Fix flaky tests`)).toBeNull()
  })
})
