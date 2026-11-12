import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/chats.json"
import type { AgentChatRow, ChatRow as ChatRowModel } from "@exp/domain-contract/chats"
import { CHAT_AGENT_ROW_TEST_ID, ChatList, ChatsHeader } from "./chat-list"
import { PhoneTab, PhoneTabBar } from "./phone-tab-bar"

// Chat 58cb32df (chats.json): the phone Chats pieces paint the contract's
// model: the list case's rows.

const rows = fixture.listCases[0]!.expected.rows as ChatRowModel[]
const agentRows = fixture.agentRowCases[0]!.expected as AgentChatRow[]
const ids = fixture.testIds

describe(`ChatList`, () => {
  it(`paints one row per chat with its lead, time, preview and badge`, () => {
    const onOpen = vi.fn()
    render(<ChatList rows={rows} onOpen={onOpen} />)
    const painted = screen.getAllByTestId(ids.chatRow)
    // #FEED-191: in chatSections order (needs you, working, unread, chats).
    expect(painted.map((row) => row.getAttribute(`data-session-id`))).toEqual([`a`, `b`, `c`, `d`])
    expect(painted.map((row) => row.querySelector(`[data-slot=chat-lead]`)?.getAttribute(`data-lead`))).toEqual([
      `needs_you`,
      `working`,
      `open`,
      `merged`,
    ])
    expect(painted[3]!.textContent).toContain(`Yesterday`)
    expect(painted[0]!.textContent).toContain(`Question: Snapshot?`)
    // #FEED-232 `rowBadge`: the needs-you row counts at least 1 (attention),
    // the unread row its unread (muted); the working row none.
    const badges = painted.map((row) => row.querySelector(`[data-slot=chat-unread]`))
    expect(badges.map((badge) => badge?.textContent ?? null)).toEqual([`1`, null, `1`, null])
    expect(badges[0]!.getAttribute(`data-tone`)).toBe(`attention`)
    expect(badges[2]!.getAttribute(`data-tone`)).toBe(`muted`)
    // The row word (ready to merge) rides line 2.
    expect(painted[2]!.textContent).toContain(`ready to merge`)
    fireEvent.click(painted[2]!)
    expect(onOpen).toHaveBeenCalledWith(`c`)
  })

  it(`opts every row into the chat menu`, () => {
    render(<ChatList rows={rows} />)
    for (const row of screen.getAllByTestId(ids.chatRow)) {
      expect(row.getAttribute(`data-menu`)).toBe(`chat`)
      expect(row.getAttribute(`data-menu-id`)).toBe(row.getAttribute(`data-session-id`))
    }
  })

  it(`ambers the time and the badge while the chat needs you`, () => {
    const row = { ...rows[1]!, unreadCount: 2 }
    render(<ChatList rows={[row]} />)
    const painted = screen.getByTestId(ids.chatRow)
    expect(painted.querySelector(`[data-slot=chat-unread]`)!.className).toContain(`bg-yellow-400`)
    expect(painted.querySelector(`.text-yellow-400`)?.textContent).toBe(`09:10`)
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
    // a needs_you (the glyph + the amber badge), b working, c/d PR glyphs.
    expect(painted[0]!.querySelector(`[data-slot=chat-lead-glyph]`)).toBeTruthy()
    expect(painted[0]!.querySelector(`[data-slot=run-mark-badge]`)).toBeTruthy()
    expect(painted.some((row) => row.querySelector(`svg mask`))).toBe(false)
  })

  it(`S-3: pins the agent rows on top under the section headers`, () => {
    const onOpenAgent = vi.fn()
    render(<ChatList rows={rows} agentRows={agentRows} onOpenAgent={onOpenAgent} />)
    const headers = screen.getAllByRole(`heading`)
    expect(headers.map((h) => h.textContent)).toEqual([
      fixture.copy.sections.agents,
      fixture.copy.sections.needs_you,
      fixture.copy.sections.working,
      fixture.copy.sections.unread,
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

  it(`#FEED-191: the Archived entry leads the Chats section, whose header shows for it alone`, () => {
    render(<ChatList rows={rows.slice(0, 1)} archived={3} />)
    const chats = screen.getByTestId(ids.chatArchived).parentElement!
    expect(chats.getAttribute(`data-section`)).toBe(`chats`)
    expect(chats.firstElementChild!.textContent).toBe(fixture.copy.sections.chats)
  })

  it(`#FEED-191: the Archived page is flat: activity order, no headers`, () => {
    render(<ChatList flat rows={rows} />)
    expect(screen.queryAllByRole(`heading`)).toHaveLength(0)
    expect(screen.getAllByTestId(ids.chatRow).map((row) => row.getAttribute(`data-session-id`))).toEqual([
      `b`,
      `a`,
      `c`,
      `d`,
    ])
  })

  it(`#FEED-191: the entry leads the list, above the agents`, () => {
    render(<ChatList rows={[]} agentRows={agentRows} entry={<div data-testid="entry" />} />)
    const list = screen.getByTestId(`chat-list`)
    expect(list.firstElementChild!.getAttribute(`data-testid`)).toBe(`entry`)
  })

  it(`shows the empty state with no rows`, () => {
    render(<ChatList rows={[]} />)
    expect(screen.queryByTestId(ids.chatRow)).toBeNull()
  })
})

describe(`ChatsHeader`, () => {
  it(`badges the inbox circle with the unread count`, () => {
    const onInbox = vi.fn()
    const view = render(<ChatsHeader onInbox={onInbox} />)
    expect(screen.getByRole(`heading`).textContent).toBe(fixture.copy.title)
    const inbox = screen.getByTestId(ids.inboxButton)
    expect(inbox.querySelector(`[data-slot=inbox-badge]`)).toBeNull()
    fireEvent.click(inbox)
    expect(onInbox).toHaveBeenCalled()
    view.rerender(<ChatsHeader inboxCount={4} />)
    expect(screen.getByTestId(ids.inboxButton).querySelector(`[data-slot=inbox-badge]`)?.textContent).toBe(`4`)
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
            badge={tab === `chats` ? { count: 2, tone: `attention` } : null}
          />
        ))}
      </PhoneTabBar>
    )
    const tabs = screen.getAllByRole(`button`)
    expect(tabs.map((tab) => tab.getAttribute(`aria-label`))).toEqual([`Board`, `Chats`, `Apps`, `Team`])
    expect(tabs[1]!.className).toContain(`bg-glass-active`)
    expect(tabs[1]!.getAttribute(`aria-current`)).toBe(`page`)
    // #FEED-232: THE count badge on the icon's corner.
    expect(tabs[1]!.querySelector(`[data-slot=phone-tab-badge]`)?.textContent).toBe(`2`)
    expect(tabs[0]!.className).not.toContain(`bg-glass-active`)
  })
})
