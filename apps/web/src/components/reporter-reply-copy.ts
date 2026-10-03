import fixture from "@exp/domain-contract/fixtures/reporter-reply.json"
import { toast } from "@exp/ui"

// SLOP-4: the ONE pinned copy set for reporter comments and the "Reply to
// reporter" toggle (`packages/domain-contract/fixtures/reporter-reply.json`,
// mirrored on desktop/iOS/Android). Web reads the json itself; this file is
// the only place the strings are touched.

export const REPORTER_REPLY_COPY = fixture.copy

/** The name a reporter's words carry: the submission's `reporterName`, else
 *  the fixture's anonymous label. */
export function reporterDisplayName(
  reporterName: string | null | undefined
): string {
  const trimmed = reporterName?.trim()
  return trimmed || REPORTER_REPLY_COPY.anonymousName
}

/** The composer placeholder while the toggle is ON. */
export function reporterReplyPlaceholder(name: string): string {
  return REPORTER_REPLY_COPY.placeholderOn.replace(`{name}`, name)
}

/** `comments.create`'s `reporterEmailed`: true → emailed, false → saved but
 *  no mail went out, null → a team comment (nothing to say). */
export function toastReporterReply(
  reporterEmailed: boolean | null | undefined
): void {
  if (reporterEmailed === true) toast.success(REPORTER_REPLY_COPY.sentToast)
  else if (reporterEmailed === false) {
    toast.message(REPORTER_REPLY_COPY.notSentToast)
  }
}
