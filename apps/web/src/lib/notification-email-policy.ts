// Pure email-notification policy — no DB, no transport. Answers "should this
// user get an immediate email for this notification?" from their (possibly
// missing) user_notification_prefs row, plus the one-way helpdesk resolution
// guards. Unit-tested in notification-email-policy.test.ts.

import type { IssueStatus, NotificationType } from "@/lib/domain"

// Digest cadence. Only `off` sends immediately; anything else is left for a
// future digest cron (the pref + skip-immediate branch ship now, the cron
// later — no schema change needed).
export const digestValues = [`off`, `daily`] as const
export type DigestCadence = (typeof digestValues)[number]

// The prefs shape the policy functions consume. A missing row (`null` /
// `undefined`) means ALL defaults: email on, every type on, no digest.
export interface EmailPrefsLike {
  emailEnabled: boolean
  // Per-type opt-outs; a type absent from the map defaults to ON.
  typePrefs: Partial<Record<NotificationType, boolean>>
  digest: string
}

export function defaultEmailPrefs(): EmailPrefsLike {
  return { emailEnabled: true, typePrefs: {}, digest: `off` }
}

// Is email allowed at all for this notification type (ignoring digest)?
// Missing row → defaults → allowed.
export function emailTypeAllowed(
  prefs: EmailPrefsLike | null | undefined,
  type: NotificationType
): boolean {
  if (!prefs) return true
  if (!prefs.emailEnabled) return false
  return prefs.typePrefs[type] !== false
}

// Should deliver() send the IMMEDIATE email? Digest != 'off' opts out of
// immediate sends (the rows stay unread for the future digest cron).
export function shouldSendImmediateEmail(
  prefs: EmailPrefsLike | null | undefined,
  type: NotificationType
): boolean {
  if (!emailTypeAllowed(prefs, type)) return false
  const digest = prefs?.digest ?? `off`
  return digest === `off`
}

// ---------------------------------------------------------------------------
// One-way helpdesk (widget reporter) resolution guards
// ---------------------------------------------------------------------------

// Statuses that count as "closed" for the reporter resolution email.
export function isResolutionStatus(status: string): status is IssueStatus {
  return status === `done` || status === `cancelled`
}

// Exactly-once semantics: send only on a close transition when the submission
// was never notified before. resolvedNotifiedAt is set-once and never cleared
// on reopen, so a reopen→re-close does NOT re-email.
export function shouldSendReporterResolution(args: {
  toStatus: string
  resolvedNotifiedAt: Date | null | undefined
}): boolean {
  if (!isResolutionStatus(args.toStatus)) return false
  return args.resolvedNotifiedAt == null
}

// ---------------------------------------------------------------------------
// URL builders
// ---------------------------------------------------------------------------

export function appBaseUrl(): string {
  return (process.env.BETTER_AUTH_URL ?? `http://localhost:5173`).replace(
    /\/$/,
    ``
  )
}

export function buildUnsubscribeUrl(baseUrl: string, token: string): string {
  return `${baseUrl.replace(/\/$/, ``)}/api/email/unsubscribe?token=${encodeURIComponent(token)}`
}

// The stable issue deep link shared by push (identifier) and email (URL):
// /w/{workspaceSlug}/projects/{projectSlug}/issues/{identifier}
export function buildIssueDeepLinkPath(args: {
  workspaceSlug: string
  projectSlug: string
  identifier: string
}): string {
  return `/w/${encodeURIComponent(args.workspaceSlug)}/projects/${encodeURIComponent(args.projectSlug)}/issues/${encodeURIComponent(args.identifier)}`
}
