import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import type { AgentBubble, AgentChatRow, ChatRow as ChatRowModel } from "@exp/domain-contract/chats"
import { AgentBubbles, CHAT_AGENT_ROW_TEST_ID, ChatFilterChips, ChatList, ChatsHeader } from "./chat-list"
import { PhoneTab, PhoneTabBar } from "./phone-tab-bar"

// Thread 58cb32df (chats.json): the phone Chats pieces paint the contract's
// model — the list case's rows, the bubble case's rings.

const rows = fixture.listCases[0]!.expected.rows as ChatRowModel[]
const bubbles = fixture.bubbleCases[0]!.expected as AgentBubble[]
const agentRows = fixture.agentRowCases[0]!.expected as AgentChatRow[]
const ids = fixture.testIds

describe(`ChatList`, () => {
  it(`paints one row per chat with its lead, time, preview and badge`, () => {
    const onOpen = vi.fn()
    render(<ChatList rows={rows} onOpen={onOpen} />)
    const painted = screen.getAllByTestId(ids.chatRow)
    expect(painted.map((row) => row.getAttribute(`data-session-id`))).toEqual([`b`, `a`, `c`, `d`])
    expect(painted.map((row) => row.querySelector(`[data-slot=chat-lead]`)?.getAttribute(`data-lead`))).toEqual([
      `working`,
      `needs_you`,
      `open`,
      `merged`,
    ])
    expect(painted[3]!.textContent).toContain(`Yesterday`)
    expect(painted[1]!.textContent).toContain(`Question: Snapshot?`)
    // Only the unread row wears the badge (foreground: it does not need you).
    const badges = painted.map((row) => row.querySelector(`[data-slot=chat-unread]`))
    expect(badges.map((badge) => badge?.textContent ?? null)).toEqual([null, null, `1`, null])
    expect(badges[2]!.className).toContain(`bg-foreground`)
    // The row word (ready to merge) rides line 2.
    expect(painted[2]!.textContent).toContain(`ready to merge`)
    fireEvent.click(painted[2]!)
    expect(onOpen).toHaveBeenCalledWith(`c`)
  })

  it(`opts every row into the thread menu`, () => {
    render(<ChatList rows={rows} />)
    for (const row of screen.getAllByTestId(ids.chatRow)) {
      expect(row.getAttribute(`data-menu`)).toBe(`thread`)
      expect(row.getAttribute(`data-menu-id`)).toBe(row.getAttribute(`data-session-id`))
    }
  })

  it(`ambers the time and the badge while the chat needs you`, () => {
    const row = { ...rows[1]!, unreadCount: 2 }
    render(<ChatList rows={[row]} />)
    const painted = screen.getByTestId(ids.chatRow)
    expect(painted.querySelector(`[data-slot=chat-unread]`)!.className).toContain(`bg-amber-400`)
    expect(painted.querySelector(`.text-amber-400`)?.textContent).toBe(`09:10`)
  })

  it(`S-3: dims a done chat as a whole at shell.dimOpacity`, () => {
    render(<ChatList rows={rows} />)
    const painted = screen.getAllByTestId(ids.chatRow)
    // listCases[0]: d's PRs all merged and nothing works.
    expect(painted.map((row) => row.getAttribute(`data-dim`))).toEqual([null, null, null, `true`])
    expect(painted[3]!.style.opacity).toBe(String(fixture.shell.dimOpacity))
    expect(painted[0]!.style.opacity).toBe(``)
  })

  it(`S-3: a chat's lead never draws the brand mark: the chat glyph`, () => {
    render(<ChatList rows={rows} />)
    const painted = screen.getAllByTestId(ids.chatRow)
    // b working, a needs_you (the glyph + the amber badge), c/d PR glyphs.
    expect(painted[1]!.querySelector(`[data-slot=chat-lead-glyph]`)).toBeTruthy()
    expect(painted[1]!.querySelector(`[data-slot=run-mark-badge]`)).toBeTruthy()
    expect(painted.some((row) => row.querySelector(`svg mask`))).toBe(false)
  })

  it(`S-3: pins the agent rows on top under the section headers`, () => {
    const onOpenAgent = vi.fn()
    render(<ChatList rows={rows} agentRows={agentRows} onOpenAgent={onOpenAgent} />)
    const headers = screen.getAllByRole(`heading`)
    expect(headers.map((h) => h.textContent)).toEqual([
      fixture.copy.sections.agents,
      fixture.copy.sections.chats,
    ])
    const painted = screen.getAllByTestId(CHAT_AGENT_ROW_TEST_ID)
    expect(painted.map((row) => row.getAttribute(`data-agent-id`))).toEqual(
      agentRows.map((row) => row.agentId)
    )
    // The Assistant (first) wears the brand mark, never selectable / dim.
    expect(painted[0]!.getAttribute(`data-assistant`)).toBe(`true`)
    expect(painted[0]!.querySelector(`svg mask`)).toBeTruthy()
    expect(painted.every((row) => row.getAttribute(`data-menu`) === `agent`)).toBe(true)
    expect(painted.some((row) => row.getAttribute(`data-dim`))).toBe(false)
    fireEvent.click(painted[1]!)
    expect(onOpenAgent).toHaveBeenCalledWith(agentRows[1]!.agentId)
  })

  it(`S-3: no section headers without an agent row`, () => {
    render(<ChatList rows={rows} agentRows={[]} />)
    expect(screen.queryAllByRole(`heading`)).toHaveLength(0)
  })

  it(`shows the empty state with no rows`, () => {
    render(<ChatList rows={[]} />)
    expect(screen.queryByTestId(ids.chatRow)).toBeNull()
  })
})

describe(`AgentBubbles`, () => {
  it(`rings amber for needs you, white for unread, none for read`, () => {
    const onOpen = vi.fn()
    render(<AgentBubbles bubbles={bubbles} onOpen={onOpen} />)
    const painted = screen.getAllByTestId(ids.agentBubble)
    expect(painted.map((b) => b.getAttribute(`data-ring`))).toEqual([`needs_you`, `unread`, `none`])
    expect(painted.map((b) => b.getAttribute(`data-menu`))).toEqual([`agent`, `agent`, `agent`])
    expect(painted[0]!.getAttribute(`data-menu-id`)).toBe(`ag1`)
    fireEvent.click(painted[1]!)
    expect(onOpen).toHaveBeenCalledWith(`ag2`)
  })
})

describe(`ChatFilterChips`, () => {
  it(`labels the chips with their counts and selects one`, () => {
    const onChange = vi.fn()
    render(
      <ChatFilterChips value="all" counts={{ needs_you: 1, running: 2, unread: 0 }} onChange={onChange} />
    )
    const chips = screen.getAllByTestId(ids.chatFilter)
    expect(chips.map((chip) => chip.textContent)).toEqual([`All`, `Needs you 1`, `Working 2`, `Unread`])
    expect(chips[0]!.getAttribute(`data-selected`)).toBe(`true`)
    fireEvent.click(chips[2]!)
    expect(onChange).toHaveBeenCalledWith(`running`)
  })
})

describe(`ChatsHeader`, () => {
  it(`dots the inbox circle only while a notification is unread`, () => {
    const onInbox = vi.fn()
    const view = render(<ChatsHeader onInbox={onInbox} />)
    expect(screen.getByRole(`heading`).textContent).toBe(fixture.copy.title)
    const inbox = screen.getByTestId(ids.inboxButton)
    expect(inbox.querySelector(`[data-slot=inbox-dot]`)).toBeNull()
    fireEvent.click(inbox)
    expect(onInbox).toHaveBeenCalled()
    view.rerender(<ChatsHeader inboxUnread />)
    expect(screen.getByTestId(ids.inboxButton).querySelector(`[data-slot=inbox-dot]`)).toBeTruthy()
  })
})

describe(`PhoneTabBar`, () => {
  it(`paints each tab as icon over label, the selected one on the active pill`, () => {
    render(
      <PhoneTabBar>
        {fixture.shell.tabs.map((tab) => (
          <PhoneTab
            key={tab}
            label={fixture.copy.tabs[tab as keyof typeof fixture.copy.tabs]}
            icon={<svg />}
            selected={tab === `chats`}
            dot={tab === `chats` ? `bg-yellow-400` : null}
          />
        ))}
      </PhoneTabBar>
    )
    const tabs = screen.getAllByRole(`button`)
    expect(tabs.map((tab) => tab.textContent)).toEqual([`Board`, `Chats`, `Apps`, `Team`])
    expect(tabs[1]!.className).toContain(`bg-glass-active`)
    expect(tabs[1]!.getAttribute(`aria-current`)).toBe(`page`)
    expect(tabs[1]!.querySelector(`.bg-yellow-400`)).toBeTruthy()
    expect(tabs[0]!.className).not.toContain(`bg-glass-active`)
  })
})
