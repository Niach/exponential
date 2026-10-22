import { describe, expect, it } from "vitest"
import { sessionsConfigureInputSchema, sessionsSendInputSchema } from "@exp/db-schema/domain"

// EXP-849: `account` on a message (`sessionId`) is an account SWITCH and may
// ride alone; every other message still needs text or attachments.
const SESSION = `3f1c2b7a-5d4e-4f60-8a9b-0c1d2e3f4a5b`

describe(`sessionsSendInputSchema — a message`, () => {
  it(`takes an account alone (a switch, no text)`, () => {
    const parsed = sessionsSendInputSchema.safeParse({ sessionId: SESSION, account: `0a1b2c3d` })
    expect(parsed.success).toBe(true)
  })

  it(`takes an account with text`, () => {
    const parsed = sessionsSendInputSchema.safeParse({
      sessionId: SESSION,
      account: `0a1b2c3d`,
      text: `carry on`,
    })
    expect(parsed.success).toBe(true)
  })

  it(`refuses a message with neither text, attachments, an answer nor an account`, () => {
    const parsed = sessionsSendInputSchema.safeParse({ sessionId: SESSION, text: `   ` })
    expect(parsed.success).toBe(false)
    expect(parsed.error?.issues[0]?.message).toBe(
      `a message needs text, attachments, issues, an answer, an account or a device`
    )
  })

  // NEW-1: the bus's answer op and the watch list.
  it(`takes an answer alone on a message, never on a start`, () => {
    expect(
      sessionsSendInputSchema.safeParse({ sessionId: SESSION, answer: { answers: [`Yes`] } }).success
    ).toBe(true)
    expect(
      sessionsSendInputSchema.safeParse({
        sessionId: SESSION,
        answer: { decision: `approved`, answers: { q1: [`A`] }, feedback: `go` },
      }).success
    ).toBe(true)
    const start = sessionsSendInputSchema.safeParse({ text: `go`, answer: { answer: `x` } })
    expect(start.success).toBe(false)
    expect(start.error?.issues.map((issue) => issue.message)).toContain(
      `an answer goes to a session (sessionId)`
    )
  })

  it(`takes wakeOn alone (a watch), on a message and as a bare call`, () => {
    expect(sessionsSendInputSchema.safeParse({ sessionId: SESSION, wakeOn: [SESSION] }).success).toBe(true)
    expect(sessionsSendInputSchema.safeParse({ wakeOn: [SESSION] }).success).toBe(true)
    expect(sessionsSendInputSchema.safeParse({ sessionId: SESSION, wakeOn: [] }).success).toBe(false)
  })

  it(`takes a device alone (EXP-1252: a move), with an account too`, () => {
    expect(sessionsSendInputSchema.safeParse({ sessionId: SESSION, deviceId: `dev-b` }).success).toBe(true)
    expect(
      sessionsSendInputSchema.safeParse({ sessionId: SESSION, deviceId: `dev-b`, account: `p1` }).success
    ).toBe(true)
  })

  it(`refuses an empty account id`, () => {
    expect(sessionsSendInputSchema.safeParse({ sessionId: SESSION, account: `` }).success).toBe(false)
  })
})

// NEW-1: the `/btw` aside is gone; an unknown key is stripped, never refused.
describe(`sessionsSendInputSchema — no asides`, () => {
  it(`strips a legacy aside flag and starts a plain child`, () => {
    const parsed = sessionsSendInputSchema.safeParse({ parentId: SESSION, text: `why?`, aside: true })
    expect(parsed.success).toBe(true)
    expect(parsed.data && `aside` in parsed.data).toBe(false)
  })
})

describe(`sessionsConfigureInputSchema (EXP-1334)`, () => {
  it(`takes any one option, a null effort included`, () => {
    for (const option of [
      { planMode: true },
      { effort: null },
      { effort: `high` },
      { ultracode: false },
      { computerUse: true },
      { mcpServerIds: [] },
    ]) {
      expect(sessionsConfigureInputSchema.safeParse({ sessionId: SESSION, ...option }).success).toBe(true)
    }
  })

  it(`refuses nothing to configure and a non-uuid MCP pick`, () => {
    expect(sessionsConfigureInputSchema.safeParse({ sessionId: SESSION }).success).toBe(false)
    expect(
      sessionsConfigureInputSchema.safeParse({ sessionId: SESSION, mcpServerIds: [`linear`] }).success
    ).toBe(false)
  })
})

describe(`sessionsConfigureInputSchema model (EXP-1334)`, () => {
  it(`takes a model alone, never a blank one`, () => {
    expect(sessionsConfigureInputSchema.safeParse({ sessionId: SESSION, model: `opus` }).success).toBe(true)
    expect(sessionsConfigureInputSchema.safeParse({ sessionId: SESSION, model: `` }).success).toBe(false)
  })
})

// EXP-1341/NEW-1: `name` names a NEW session; `issueIds` = a child's own rows
// or a message's forward.
describe(`sessionsSendInputSchema — name and issues`, () => {
  const ISSUE = `7e1c2b7a-5d4e-4f60-8a9b-0c1d2e3f4a5b`

  it(`takes a name on a new root and on a child`, () => {
    expect(sessionsSendInputSchema.safeParse({ text: `go`, name: `Perf pass` }).success).toBe(true)
    expect(
      sessionsSendInputSchema.safeParse({ parentId: SESSION, text: `go`, name: `Perf pass` }).success
    ).toBe(true)
  })

  it(`refuses a name on a message, on an agent, and a blank one`, () => {
    const cases: Array<[unknown, string]> = [
      [{ sessionId: SESSION, text: `go`, name: `x` }, `a name is for a new session`],
      [
        { agentId: `11111111-1111-4111-8111-111111111111`, text: `hi`, name: `x` },
        `a message to an agent takes no root facets`,
      ],
    ]
    for (const [input, message] of cases) {
      const parsed = sessionsSendInputSchema.safeParse(input)
      expect(parsed.success).toBe(false)
      expect(parsed.error?.issues.map((issue) => issue.message)).toContain(message)
    }
    expect(sessionsSendInputSchema.safeParse({ text: `go`, name: `   ` }).success).toBe(false)
  })

  it(`takes issueIds on a child (its rows) and on a message (forwarded, even alone)`, () => {
    expect(
      sessionsSendInputSchema.safeParse({ parentId: SESSION, text: `go`, issueIds: [ISSUE] }).success
    ).toBe(true)
    expect(sessionsSendInputSchema.safeParse({ sessionId: SESSION, issueIds: [ISSUE] }).success).toBe(true)
  })
})
