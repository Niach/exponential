import { describe, expect, it } from "vitest"
import { buildWidgetDescription, stripLegacyWidgetMetadata } from "./metadata"

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

  // EXP-42b: reporter contact + env metadata is PII and must never reach the
  // (potentially public) description — it lives only in widget_submissions.
  it(`never embeds a metadata block`, () => {
    const text = buildWidgetDescription(baseArgs)
    expect(text).not.toContain(`Reported via widget`)
    expect(text).not.toContain(`Reporter:`)
    expect(text).not.toContain(`---`)
  })
})

// The pre-EXP-42b builder appended the metadata as a final `---` section:
// sections joined by blank lines, block opened by the bold header line.
const legacyBlock = [
  `---`,
  ``,
  [
    `**Reported via widget** · Acme Feedback`,
    ``,
    `- Reporter: \`Jane <jane@example.com>\``,
    `- Page: \`https://acme.test/checkout\``,
    `- User agent: \`Mozilla/5.0\``,
  ].join(`\n`),
].join(`\n`)

describe(`stripLegacyWidgetMetadata`, () => {
  it(`removes the legacy block, keeping user text + screenshot`, () => {
    const legacy = `The checkout button is broken\n\n![Screenshot](/api/attachments/123e4567-e89b-42d3-a456-426614174000)\n\n${legacyBlock}`
    expect(stripLegacyWidgetMetadata(legacy)).toBe(
      `The checkout button is broken\n\n![Screenshot](/api/attachments/123e4567-e89b-42d3-a456-426614174000)`
    )
  })

  it(`reduces a metadata-only description to an empty string`, () => {
    expect(stripLegacyWidgetMetadata(legacyBlock)).toBe(``)
  })

  it(`returns null for post-EXP-42b descriptions (nothing to scrub)`, () => {
    expect(
      stripLegacyWidgetMetadata(`Just my feedback text`)
    ).toBeNull()
    // A plain thematic break is not the marker.
    expect(stripLegacyWidgetMetadata(`before\n\n---\n\nafter`)).toBeNull()
  })

  it(`is idempotent — a scrubbed description strips to null`, () => {
    const legacy = `Some text\n\n${legacyBlock}`
    const once = stripLegacyWidgetMetadata(legacy)
    expect(once).toBe(`Some text`)
    expect(stripLegacyWidgetMetadata(once!)).toBeNull()
  })
})
