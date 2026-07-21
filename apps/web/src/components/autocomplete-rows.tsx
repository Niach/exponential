import type { User } from "@/db/schema"
import { getInitials } from "@/lib/utils"
import { displayUserName } from "@/lib/user-display"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import type { ResolvedIssueRef } from "@/components/issue-ref-provider"

// The candidate rows of the @mention / #issue autocomplete menus — shared
// between the comment composer (mention-textarea.tsx) and the TipTap markdown
// editor (issue-editor/markdown-editor.tsx) so both popups look identical.
// Selection uses onMouseDown+preventDefault so the editor/textarea keeps
// focus through the click.

export function UserCandidateRow({
  user,
  active,
  onSelect,
  onHover,
}: {
  user: User
  active: boolean
  onSelect: () => void
  onHover: () => void
}) {
  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email — don't render it a second time on the trailing line.
  const name = displayUserName(user, user.id)
  return (
    <button
      type="button"
      onMouseDown={(e) => {
        e.preventDefault()
        onSelect()
      }}
      onMouseEnter={onHover}
      className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm ${
        active ? `bg-accent` : ``
      }`}
    >
      <Avatar className="size-5">
        {user.image && <AvatarImage src={user.image} alt={name} />}
        <AvatarFallback className="text-[0.5625rem]">
          {getInitials(name)}
        </AvatarFallback>
      </Avatar>
      <span className="truncate">{name}</span>
      {user.email && user.email !== name && (
        <span className="ml-auto truncate text-xs text-muted-foreground">
          {user.email}
        </span>
      )}
    </button>
  )
}

export function IssueCandidateRow({
  issue,
  active,
  onSelect,
  onHover,
}: {
  issue: ResolvedIssueRef
  active: boolean
  onSelect: () => void
  onHover: () => void
}) {
  return (
    <button
      type="button"
      onMouseDown={(e) => {
        e.preventDefault()
        onSelect()
      }}
      onMouseEnter={onHover}
      className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm ${
        active ? `bg-accent` : ``
      }`}
    >
      <StatusIcon status={issue.status} className="size-4 shrink-0" />
      <span className="shrink-0 font-mono text-xs text-muted-foreground">
        {issue.identifier}
      </span>
      <span className="truncate">{issue.title}</span>
    </button>
  )
}
