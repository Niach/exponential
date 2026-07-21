import { describe, expect, it } from "vitest"
import {
  isTrustedSnsUrl,
  parseSesNotification,
} from "@/lib/email-bounces"

describe(`parseSesNotification`, () => {
  it(`parses a bounce with recipients, type/subtype, diagnostic, messageId`, () => {
    const events = parseSesNotification({
      notificationType: `Bounce`,
      bounce: {
        bounceType: `Permanent`,
        bounceSubType: `General`,
        bouncedRecipients: [
          {
            emailAddress: `Bad.User@Example.com`,
            diagnosticCode: `smtp; 550 5.1.1 user unknown`,
          },
          { emailAddress: `other@example.com` },
        ],
      },
      mail: { messageId: `ses-msg-1` },
    })
    expect(events).toHaveLength(2)
    expect(events[0]).toEqual({
      email: `bad.user@example.com`,
      kind: `bounce`,
      bounceType: `Permanent`,
      bounceSubType: `General`,
      diagnostic: `smtp; 550 5.1.1 user unknown`,
      providerMessageId: `ses-msg-1`,
    })
    expect(events[1].diagnostic).toBeNull()
  })

  it(`parses a complaint with the feedback type in bounceSubType`, () => {
    const events = parseSesNotification({
      notificationType: `Complaint`,
      complaint: {
        complaintFeedbackType: `abuse`,
        complainedRecipients: [{ emailAddress: `angry@example.com` }],
      },
      mail: { messageId: `ses-msg-2` },
    })
    expect(events).toEqual([
      {
        email: `angry@example.com`,
        kind: `complaint`,
        bounceType: null,
        bounceSubType: `abuse`,
        diagnostic: null,
        providerMessageId: `ses-msg-2`,
      },
    ])
  })

  it(`accepts the event-publishing eventType field too`, () => {
    const events = parseSesNotification({
      eventType: `Bounce`,
      bounce: { bouncedRecipients: [{ emailAddress: `a@b.com` }] },
      mail: {},
    })
    expect(events).toHaveLength(1)
    expect(events[0].providerMessageId).toBeNull()
  })

  it(`ignores untracked notification types and malformed payloads`, () => {
    expect(parseSesNotification({ notificationType: `Delivery` })).toEqual([])
    expect(parseSesNotification(null)).toEqual([])
    expect(parseSesNotification(`Bounce`)).toEqual([])
    expect(
      parseSesNotification({
        notificationType: `Bounce`,
        bounce: { bouncedRecipients: [{ emailAddress: 42 }] },
      })
    ).toEqual([])
  })
})

describe(`isTrustedSnsUrl`, () => {
  it(`accepts real SNS confirmation endpoints`, () => {
    expect(
      isTrustedSnsUrl(
        `https://sns.eu-north-1.amazonaws.com/?Action=ConfirmSubscription&Token=x`
      )
    ).toBe(true)
  })

  it(`rejects non-SNS hosts, http, and garbage`, () => {
    expect(isTrustedSnsUrl(`https://evil.com/?x=1`)).toBe(false)
    expect(isTrustedSnsUrl(`https://sns.eu-north-1.amazonaws.com.evil.com/`)).toBe(
      false
    )
    expect(isTrustedSnsUrl(`http://sns.eu-north-1.amazonaws.com/`)).toBe(false)
    expect(isTrustedSnsUrl(`not a url`)).toBe(false)
  })
})
