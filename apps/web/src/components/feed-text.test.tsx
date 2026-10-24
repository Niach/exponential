import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { FeedText, splitMentions } from "@/components/feed-text"

// Thread e4a04d22 (owner 2026-10-24: "the mentions should also get a proper
// ui everywhere"): a chat's plain text path paints a resolved `@<email>` as
// THE round mention pill; an unresolved one stays text.

vi.mock(`@/components/issue-ref-provider`, () => ({ useIssueRefs: () => null }))
vi.mock(`@/components/issue-editor/markdown-editor`, () => ({ MarkdownEditor: () => null }))
vi.mock(`@/hooks/use-app-link-click`, () => ({ useAppLinkClick: () => () => {} }))
const members: Record<string, string> = { "ada@exp.dev": `Ada Lovelace` }
vi.mock(`@/components/mention-provider`, () => ({
  useMentions: () => ({
    resolve: (email: string) => (members[email.toLowerCase()] ? { name: members[email.toLowerCase()]! } : null),
    search: () => [],
  }),
}))

const resolver = { resolve: (email: string) => (members[email.toLowerCase()] ? { name: members[email.toLowerCase()]! } : null) }

describe(`splitMentions`, () => {
  it(`cuts around resolved mentions only`, () => {
    expect(splitMentions(`ping @Ada@exp.dev and @x@y.com now`, resolver)).toEqual([
      { text: `ping ` },
      { text: `@Ada@exp.dev`, email: `Ada@exp.dev`, member: { name: `Ada Lovelace` } },
      { text: ` and @x@y.com now` },
    ])
  })
  it(`no @ or no resolver = one piece`, () => {
    expect(splitMentions(`plain`, resolver)).toEqual([{ text: `plain` }])
    expect(splitMentions(`@ada@exp.dev`, null)).toEqual([{ text: `@ada@exp.dev` }])
  })
})

describe(`FeedText mentions`, () => {
  it(`paints a member's pill and leaves an unknown email as text`, () => {
    const { container } = render(<FeedText text="ask @ada@exp.dev or @x@y.com" ariaLabel="message" />)
    const pill = screen.getByText(`@Ada Lovelace`)
    expect(pill.className).toContain(`mention-pill`)
    expect(pill.getAttribute(`title`)).toBe(`ada@exp.dev`)
    expect(container.textContent).toBe(`ask @Ada Lovelace or @x@y.com`)
    expect(container.querySelectorAll(`.mention-pill`)).toHaveLength(1)
  })
})
