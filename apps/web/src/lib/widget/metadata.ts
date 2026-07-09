import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"

// EXP-42b (privacy): a widget issue's description carries ONLY the reporter's
// own text plus the screenshot image — reporter contact, page URL, and
// device/env metadata are PII that must never land in a description (feedback
// boards render them publicly). The structured copy lives server-only in
// `widget_submissions`, surfaced to members via `widgets.submissionForIssue`.
// Pre-existing issues are scrubbed by the one-off backfill below
// (`bun run backfill:widget-descriptions`, apps/web).
export function buildWidgetDescription(args: {
  userText: string
  screenshotAttachmentId: string | null
}): string {
  const sections: string[] = []

  const userText = args.userText.trim()
  if (userText) sections.push(userText)

  if (args.screenshotAttachmentId) {
    sections.push(
      `![Screenshot](${buildAttachmentUrl(args.screenshotAttachmentId)})`
    )
  }

  return sections.join(`\n\n`)
}

// Pre-EXP-42b descriptions ended in a `---`-separated metadata section opening
// with this bold header line; everything from the marker on is the legacy
// block (reporter contact, page URL, viewport/UA, custom-data fence).
const legacyMetadataMarker = `---\n\n**Reported via widget**`

// EXP-42b backfill helper (scripts/scrub-widget-descriptions.ts): strip the
// legacy PII metadata block from a pre-EXP-42b widget-issue description.
// Returns the scrubbed description (may be empty — store null then, like
// buildWidgetDescription callers do), or null when there is no legacy block
// so callers can skip the write. The structured copy in `widget_submissions`
// is untouched, so nothing is lost.
export function stripLegacyWidgetMetadata(description: string): string | null {
  const index = description.indexOf(legacyMetadataMarker)
  if (index === -1) return null
  return description.slice(0, index).trimEnd()
}
