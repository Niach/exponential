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
    expect(parsed.error?.issues[0]?.message).toBe(`a message needs text, attachments or an account`)
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
