import { describe, expect, it } from "vitest"
import { sessionsSendInputSchema } from "@exp/db-schema/domain"

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
