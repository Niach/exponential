import { describe, expect, it } from "vitest"
import { buildWidgetDescription } from "./metadata"

const baseArgs = {
  userText: `The checkout button is broken`,
  screenshotAttachmentId: null,
  widgetName: `Acme App`,
  reporterName: null,
  reporterEmail: null,
  meta: {},
  customData: null,
}

describe(`buildWidgetDescription`, () => {
  it(`renders user text, divider, and anonymous reporter`, () => {
    const text = buildWidgetDescription(baseArgs)
    expect(text).toContain(`The checkout button is broken`)
    expect(text).toContain(`---`)
    expect(text).toContain(`**Reported via widget** · Acme App`)
    expect(text).toContain(`- Reporter: \`anonymous\``)
  })

  it(`embeds the screenshot as a canonical relative attachment image`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      screenshotAttachmentId: `123e4567-e89b-42d3-a456-426614174000`,
    })
    expect(text).toContain(
      `![Screenshot](/api/attachments/123e4567-e89b-42d3-a456-426614174000)`
    )
  })

  it(`formats reporter name and email`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      reporterName: `Jane`,
      reporterEmail: `jane@acme.com`,
    })
    expect(text).toContain(`- Reporter: \`Jane <jane@acme.com>\``)
  })

  it(`renders viewport, screen, and user agent when present`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      meta: {
        pageUrl: `https://acme.com/checkout`,
        userAgent: `Mozilla/5.0 Test`,
        viewportWidth: 1280,
        viewportHeight: 720,
        screenWidth: 2560,
        screenHeight: 1440,
        devicePixelRatio: 2,
      },
    })
    expect(text).toContain(`- Page: \`https://acme.com/checkout\``)
    expect(text).toContain(`Viewport: \`1280×720 @2x\``)
    expect(text).toContain(`Screen: \`2560×1440\``)
    expect(text).toContain(`- User agent: \`Mozilla/5.0 Test\``)
  })

  it(`sanitizes newlines and backticks out of inline values`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      reporterName: `evil\`\n# injected heading`,
      meta: { pageUrl: `https://a.com/\`x\`\r\nmore` },
    })
    expect(text).not.toContain(`# injected heading\n`)
    expect(text).toContain(`- Reporter: \`evil' # injected heading\``)
    expect(text).toContain(`- Page: \`https://a.com/'x' more\``)
  })

  it(`renders custom data as a fenced json block`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      customData: { plan: `pro`, userId: 42 },
    })
    expect(text).toContain(`\`\`\`\`json`)
    expect(text).toContain(`"plan": "pro"`)
    expect(text).toContain(`"userId": 42`)
  })

  it(`truncates oversized custom data`, () => {
    const text = buildWidgetDescription({
      ...baseArgs,
      customData: { blob: `x`.repeat(20_000) },
    })
    expect(text).toContain(`… (truncated)`)
    expect(text.length).toBeLessThan(12_000)
  })

  it(`omits the screenshot line and empty meta lines when absent`, () => {
    const text = buildWidgetDescription(baseArgs)
    expect(text).not.toContain(`![Screenshot]`)
    expect(text).not.toContain(`- Page:`)
    expect(text).not.toContain(`- User agent:`)
    expect(text).not.toContain(`\`\`\`\`json`)
  })
})
