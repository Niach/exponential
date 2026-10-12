import { render } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { CodingSession } from "@/db/schema"

// EXP-1284 (review): a session block shows ONLY its own Guide entries
// (`sessionGuideEntries`: stamped by `sessionId`; unstamped legacy entries
// belong to the thread's FIRST session) — never the whole thread Guide again
// in every block. A teammate's block is the status row alone (EXP-312).

const seen = vi.hoisted(() => ({ props: [] as Record<string, unknown>[] }))

vi.mock(`@/components/agent-session`, () => ({
  AgentSessionView: (props: Record<string, unknown>) => {
    seen.props.push(props)
    return <div data-testid="embedded" />
  },
}))
vi.mock(`@/hooks/use-session-device`, () => ({
  useSessionDevice: () => ({ label: `macbook`, online: true }),
}))

import { ThreadRunBody } from "@/components/thread/thread-run-body"

const results = [
  { topic: `Summary`, text: `legacy` },
  { topic: `Plan`, text: `one`, sessionId: `r1` },
  { topic: `Fix`, text: `two`, sessionId: `r2` },
]

const session = (id: string, userId = `me`) =>
  ({
    id,
    userId,
    status: `running`,
    needsInput: false,
    agentBusy: false,
    startedAt: new Date(),
    updatedAt: new Date(),
    results: null,
  }) as unknown as CodingSession

beforeEach(() => {
  seen.props = []
})

describe(`ThreadRunBody`, () => {
  it(`hands the embedded Run face only this session's entries`, () => {
    const identity = { identifier: null, subject: `Chat` }
    render(
      <ThreadRunBody
        session={session(`r2`)}
        currentUserId="me"
        identity={identity}
        prState={null}
        threadResults={results}
        firstSessionId="r1"
      />
    )
    expect(seen.props[0]!.embedded).toBe(true)
    expect(seen.props[0]!.blockResults).toEqual([results[2]])
  })

  it(`gives the first session the unstamped legacy entries`, () => {
    render(
      <ThreadRunBody
        session={session(`r1`)}
        currentUserId="me"
        identity={{ identifier: null, subject: `Chat` }}
        prState={null}
        threadResults={results}
        firstSessionId="r1"
      />
    )
    expect(seen.props[0]!.blockResults).toEqual([results[0], results[1]])
  })

  it(`never mounts the Run face for a teammate's session`, () => {
    const { getByTestId } = render(
      <ThreadRunBody
        session={session(`r3`, `them`)}
        currentUserId="me"
        identity={{ identifier: null, subject: `Chat` }}
        prState={null}
        threadResults={results}
        firstSessionId="r1"
      />
    )
    expect(seen.props).toHaveLength(0)
    expect(getByTestId(`thread-run-body`).getAttribute(`data-owner`)).toBe(`false`)
  })
})
