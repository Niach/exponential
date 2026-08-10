import { describe, expect, it } from "vitest"
import { deliveryRowFromResult } from "./email"

describe(`deliveryRowFromResult`, () => {
  it(`maps a delivered send to a sent row with subject and no error`, () => {
    const row = deliveryRowFromResult({
      delivered: true,
      provider: `ses`,
      messageId: `msg-1`,
      subject: `Reset your Exponential password`,
    })
    expect(row.status).toBe(`sent`)
    expect(row.provider).toBe(`ses`)
    expect(row.providerMessageId).toBe(`msg-1`)
    expect(row.subject).toBe(`Reset your Exponential password`)
    expect(row.sentAt).toBeInstanceOf(Date)
    expect(row.error).toBeNull()
  })

  it(`maps a suppressed send to a suppressed row with the suppression error`, () => {
    const row = deliveryRowFromResult({
      delivered: false,
      provider: null,
      messageId: null,
      subject: `Verify your email for Exponential`,
      suppressed: true,
    })
    expect(row.status).toBe(`suppressed`)
    expect(row.sentAt).toBeNull()
    expect(row.error).toBe(`recipient suppressed (bounce/complaint on record)`)
    expect(row.subject).toBe(`Verify your email for Exponential`)
  })

  it(`maps a no-transport send to a failed row with the transport error`, () => {
    const row = deliveryRowFromResult({
      delivered: false,
      provider: null,
      messageId: null,
      subject: `Contact form: someone`,
    })
    expect(row.status).toBe(`failed`)
    expect(row.provider).toBeNull()
    expect(row.providerMessageId).toBeNull()
    expect(row.sentAt).toBeNull()
    expect(row.error).toBe(`no email transport configured`)
  })
})
