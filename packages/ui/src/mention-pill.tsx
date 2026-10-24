import { cn } from "./cn"

// Thread e4a04d22 (owner 2026-10-24: "the mentions should also get a proper
// ui everywhere") — THE mention pill, presentational half. ONE round pill for
// every surface that names a member by `@<email>`: the comment and
// description editors (a ProseMirror DECORATION, the app's
// `lib/mention-pill-extension.ts`, because the document text stays the bare
// token) and every chat's plain text path (`IssueRefText` in the app's
// feed-text.tsx). Both carry `MENTION_PILL_CLASS`, whose box lives in this
// package's styles.css (`.mention-pill`), so they cannot drift.
//
// A pill renders only for an email the caller already resolved to a member
// the viewer can see; an unresolved `@x@y.com` stays plain text.

/** The class both the component and the editor decoration carry. */
export const MENTION_PILL_CLASS = `mention-pill`

export interface MentionPillProps {
  /** The member's display name (the pill reads `@Name`). */
  name: string
  /** The mention's email (tooltip + `data-mention-email`). */
  email: string
  className?: string
}

export function MentionPill({ name, email, className }: MentionPillProps) {
  return (
    <span className={cn(MENTION_PILL_CLASS, className)} title={email} data-mention-email={email}>
      @{name}
    </span>
  )
}
