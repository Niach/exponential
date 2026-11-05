import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ sessionCollection: { get: () => undefined } }))
vi.mock(`@/hooks/use-session-tree`, () => ({}))
vi.mock(`@/components/thread/threads-list`, () => ({}))
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))

const { needsYouDestination } = await import(`./use-needs-you`)

const sessions = new Map<string, { id: string; rootId: string; agentId: string | null; userId: string | null }>([
  [`r`, { id: `r`, rootId: `r`, agentId: null, userId: `me` }],
  [`c`, { id: `c`, rootId: `r`, agentId: null, userId: `me` }],
  [`a`, { id: `a`, rootId: `a`, agentId: `agent`, userId: `me` }],
  [`ac`, { id: `ac`, rootId: `a`, agentId: null, userId: `me` }],
])
const lookup = (id: string) => sessions.get(id)
const go = (row: { sessionId: string; rootId: string; opens: `plan` | `card` | `results` | `chat` }) =>
  needsYouDestination({ messageId: `m`, ...row }, `acme`, lookup, `me`)

describe(`needsYouDestination`, () => {
  it(`a plan = its deep link on the card's own session`, () => {
    expect(go({ sessionId: `c`, rootId: `r`, opens: `plan` })).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId/plan/$messageId`,
      params: { teamSlug: `acme`, sessionId: `c`, messageId: `m` },
    })
  })

  it(`a child's question = the root's Conversation, scrolled to the card`, () => {
    expect(go({ sessionId: `c`, rootId: `r`, opens: `card` })).toEqual({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `r` },
      search: { card: `m` },
    })
  })

  it(`a question in an agent conversation or its child = the agent's chat`, () => {
    const agent = {
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `agent` },
      search: { card: `m` },
    }
    expect(go({ sessionId: `a`, rootId: `a`, opens: `card` })).toEqual(agent)
    expect(go({ sessionId: `ac`, rootId: `a`, opens: `card` })).toEqual(agent)
  })

  it(`a PR = the Results tab; an agent conversation's = its chat`, () => {
    expect(go({ sessionId: `r`, rootId: `r`, opens: `results` })).toEqual({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `r` },
      search: { tab: `results` },
    })
    expect(go({ sessionId: `a`, rootId: `a`, opens: `chat` })).toEqual({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `agent` },
    })
  })
})
