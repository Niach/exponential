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

  it(`refuses a message with neither text, attachments nor an account`, () => {
    const parsed = sessionsSendInputSchema.safeParse({ sessionId: SESSION, text: `   ` })
    expect(parsed.success).toBe(false)
    expect(parsed.error?.issues[0]?.message).toBe(`a message needs text, attachments, issues, an account or a device`)
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

// EXP-1330: a `/btw` aside = a child (`parentId`) carrying the question.
describe(`sessionsSendInputSchema — an aside`, () => {
  it(`takes parentId + text`, () => {
    expect(
      sessionsSendInputSchema.safeParse({ parentId: SESSION, text: `why?`, aside: true }).success
    ).toBe(true)
  })

  it(`refuses an aside without a parent or a question`, () => {
    for (const input of [
      { text: `why?`, aside: true },
      { parentId: SESSION, text: `  `, aside: true },
      { sessionId: SESSION, text: `why?`, aside: true },
    ]) {
      const parsed = sessionsSendInputSchema.safeParse(input)
      expect(parsed.success).toBe(false)
      expect(parsed.error?.issues.map((issue) => issue.message)).toContain(
        `an aside needs a parentId and a question`
      )
    }
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

// EXP-1341: `name` names a NEW session; `issueIds` assign (child) or forward
// (message).
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
        {
          name: `x`,
          agent: { name: `Bot`, icon: `bot`, color: `#112233` },
        },
        `an agent is named by agent.name`,
      ],
    ]
    for (const [input, message] of cases) {
      const parsed = sessionsSendInputSchema.safeParse(input)
      expect(parsed.success).toBe(false)
      expect(parsed.error?.issues.map((issue) => issue.message)).toContain(message)
    }
    expect(sessionsSendInputSchema.safeParse({ text: `go`, name: `   ` }).success).toBe(false)
  })

  it(`takes issueIds on a child (assigned) and on a message (forwarded, even alone)`, () => {
    expect(
      sessionsSendInputSchema.safeParse({ parentId: SESSION, text: `go`, issueIds: [ISSUE] }).success
    ).toBe(true)
    expect(sessionsSendInputSchema.safeParse({ sessionId: SESSION, issueIds: [ISSUE] }).success).toBe(true)
    expect(
      sessionsSendInputSchema.safeParse({ parentId: SESSION, text: `why?`, aside: true, issueIds: [ISSUE] })
        .success
    ).toBe(false)
  })
})
