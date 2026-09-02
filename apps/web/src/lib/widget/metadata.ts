import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"

// EXP-42b (privacy): a widget issue's description carries ONLY the reporter's
// own text plus the screenshot image — reporter contact, page URL, and
// device/env metadata are PII that must never land in a description (feedback
// boards render them publicly). The structured copy lives server-only in
// `widget_submissions`, surfaced to members via `widgets.submissionForIssue`.
export function buildWidgetDescription(args: {
  userText: string
  screenshotAttachmentId: string | null
  // Reporter-attached pictures (FEED-5), after the screenshot. The alt text
  // is a constant — reporter filenames are not markdown-safe.
  imageAttachmentIds?: string[]
}): string {
  const sections: string[] = []

  const userText = args.userText.trim()
  if (userText) sections.push(userText)

  if (args.screenshotAttachmentId) {
    sections.push(
      `![Screenshot](${buildAttachmentUrl(args.screenshotAttachmentId)})`
    )
  }

  for (const imageId of args.imageAttachmentIds ?? []) {
    sections.push(`![Image](${buildAttachmentUrl(imageId)})`)
  }

  return sections.join(`\n\n`)
}
