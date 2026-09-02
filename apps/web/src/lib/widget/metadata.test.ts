import { describe, expect, it } from "vitest"
import { buildWidgetDescription } from "./metadata"

const baseArgs = {
  userText: `The checkout button is broken`,
  screenshotAttachmentId: null,
}

describe(`buildWidgetDescription`, () => {
  it(`renders only the user's text`, () => {
    const text = buildWidgetDescription(baseArgs)
    expect(text).toBe(`The checkout button is broken`)
  })

  it(`embeds the screenshot as a canonical relative attachment image`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      screenshotAttachmentId: `123e4567-e89b-42d3-a456-426614174000`,
    })
    expect(text).toBe(
      `The checkout button is broken\n\n![Screenshot](/api/attachments/123e4567-e89b-42d3-a456-426614174000)`
    )
  })

  it(`renders just the screenshot when the user text is empty`, () => {
    const text = buildWidgetDescription({
      userText: `  `,
      screenshotAttachmentId: `123e4567-e89b-42d3-a456-426614174000`,
    })
    expect(text).toBe(
      `![Screenshot](/api/attachments/123e4567-e89b-42d3-a456-426614174000)`
    )
  })

  it(`returns an empty string when there is nothing to render`, () => {
    expect(
      buildWidgetDescription({ userText: ``, screenshotAttachmentId: null })
    ).toBe(``)
  })

  // FEED-5: reporter-attached pictures follow the screenshot, one image line
  // each, in submission order.
  it(`embeds reporter pictures after the screenshot`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      screenshotAttachmentId: `123e4567-e89b-42d3-a456-426614174000`,
      imageAttachmentIds: [
        `223e4567-e89b-42d3-a456-426614174000`,
        `323e4567-e89b-42d3-a456-426614174000`,
      ],
    })
    expect(text).toBe(
      [
        `The checkout button is broken`,
        `![Screenshot](/api/attachments/123e4567-e89b-42d3-a456-426614174000)`,
        `![Image](/api/attachments/223e4567-e89b-42d3-a456-426614174000)`,
        `![Image](/api/attachments/323e4567-e89b-42d3-a456-426614174000)`,
      ].join(`\n\n`)
    )
  })

  it(`embeds pictures without a screenshot`, () => {
    const text = buildWidgetDescription({
      userText: ``,
      screenshotAttachmentId: null,
      imageAttachmentIds: [`223e4567-e89b-42d3-a456-426614174000`],
    })
    expect(text).toBe(
      `![Image](/api/attachments/223e4567-e89b-42d3-a456-426614174000)`
    )
  })

  // EXP-42b: reporter contact + env metadata is PII and must never reach the
  // (potentially public) description — it lives only in widget_submissions.
  it(`never embeds a metadata block`, () => {
    const text = buildWidgetDescription(baseArgs)
    expect(text).not.toContain(`Reported via widget`)
    expect(text).not.toContain(`Reporter:`)
    expect(text).not.toContain(`---`)
  })
})
