import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"

export interface WidgetEnvMeta {
  pageUrl?: string | null
  userAgent?: string | null
  viewportWidth?: number | null
  viewportHeight?: number | null
  screenWidth?: number | null
  screenHeight?: number | null
  devicePixelRatio?: number | null
}

// Inline metadata values are untrusted single-line strings rendered inside
// backtick code spans: strip newlines (no block injection), drop backticks
// (no span breakout), and truncate.
function sanitizeInline(value: string, maxLength: number): string {
  return value
    .replace(/[\r\n]+/g, ` `)
    .replace(/`/g, `'`)
    .trim()
    .slice(0, maxLength)
}

function formatReporter(
  name: string | null,
  email: string | null
): string {
  if (!name && !email) return `anonymous`
  const safeName = name ? sanitizeInline(name, 255) : null
  const safeEmail = email ? sanitizeInline(email, 320) : null
  if (safeName && safeEmail) return `${safeName} <${safeEmail}>`
  return safeName ?? safeEmail ?? `anonymous`
}

const maxCustomDataChars = 8 * 1024

// The issue description is the human-readable copy of a widget submission;
// the structured copy lives in `widget_submissions` (it survives description
// edits and is what a helpdesk reply flow would read).
export function buildWidgetDescription(args: {
  userText: string
  screenshotAttachmentId: string | null
  widgetName: string
  reporterName: string | null
  reporterEmail: string | null
  meta: WidgetEnvMeta
  customData: Record<string, unknown> | null
}): string {
  const sections: string[] = []

  const userText = args.userText.trim()
  if (userText) sections.push(userText)

  if (args.screenshotAttachmentId) {
    sections.push(
      `![Screenshot](${buildAttachmentUrl(args.screenshotAttachmentId)})`
    )
  }

  const lines: string[] = [
    `**Reported via widget** · ${sanitizeInline(args.widgetName, 255)}`,
    ``,
    `- Reporter: \`${formatReporter(args.reporterName, args.reporterEmail)}\``,
  ]

  if (args.meta.pageUrl) {
    lines.push(`- Page: \`${sanitizeInline(args.meta.pageUrl, 2048)}\``)
  }

  const viewport =
    args.meta.viewportWidth && args.meta.viewportHeight
      ? `${args.meta.viewportWidth}×${args.meta.viewportHeight}` +
        (args.meta.devicePixelRatio ? ` @${args.meta.devicePixelRatio}x` : ``)
      : null
  const screen =
    args.meta.screenWidth && args.meta.screenHeight
      ? `${args.meta.screenWidth}×${args.meta.screenHeight}`
      : null
  if (viewport || screen) {
    const parts = []
    if (viewport) parts.push(`Viewport: \`${viewport}\``)
    if (screen) parts.push(`Screen: \`${screen}\``)
    lines.push(`- ${parts.join(` · `)}`)
  }

  if (args.meta.userAgent) {
    lines.push(`- User agent: \`${sanitizeInline(args.meta.userAgent, 512)}\``)
  }

  if (args.customData && Object.keys(args.customData).length > 0) {
    let json = JSON.stringify(args.customData, null, 2)
    if (json.length > maxCustomDataChars) {
      json = `${json.slice(0, maxCustomDataChars)}\n… (truncated)`
    }
    // Fence with four backticks so embedded ``` in values can't break out.
    lines.push(``, `\`\`\`\`json\n${json}\n\`\`\`\``)
  }

  sections.push(`---`, lines.join(`\n`))
  return sections.join(`\n\n`)
}
