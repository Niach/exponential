import type { User } from "@/db/schema"
import { displayUserName } from "@/lib/user-display"
import { getActionIcon, TypeaheadRow, UserAvatar } from "@exp/ui"
import type { EmojiRecord } from "@/lib/emoji"
import type { SlashPrompt, SteerCommand } from "@/lib/steer-commands"

// The candidate rows of the @mention / :emoji autocomplete menus — shared
// between the comment composer (mention-textarea.tsx) and the TipTap markdown
// editor (issue-editor/markdown-editor.tsx) so both popups look identical.
// SLOP-28: `#` is not a menu any more — it opens THE search list
// (`issue-ref-search.tsx`), whose rows are the issue picker's.
//
// EXP-941: the row CHROME is `TypeaheadRow` from @exp/ui — it owns the one row
// recipe (`TYPEAHEAD_ROW_CLASS` + the `bg-glass-active` highlight the package
// menus use), `role="option"`/`aria-selected`, and the mousedown
// preventDefault that keeps the editor/textarea's caret through the click.
// Everything below is just the CONTENT of a row.

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
    <TypeaheadRow active={active} onSelect={onSelect} onMouseEnter={onHover}>
      <UserAvatar size={20} user={{ id: user.id, name, image: user.image }} />
      <span className="truncate">{name}</span>
      {user.email && user.email !== name && (
        <span className="ml-auto truncate text-xs text-muted-foreground">
          {user.email}
        </span>
      )}
    </TypeaheadRow>
  )
}

/** EXP-551: a `:shortcode` candidate — the glyph, the shortcode it completes
 *  and the emoji's label. `unicode` is what the pick inserts (skin tone
 *  already applied by the caller). */
export function EmojiCandidateRow({
  emoji,
  unicode,
  query,
  active,
  onSelect,
  onHover,
}: {
  emoji: EmojiRecord
  unicode: string
  /** The typed query — the shortcode that matches it is the one shown. */
  query?: string
  active: boolean
  onSelect: () => void
  onHover: () => void
}) {
  const q = query?.toLowerCase() ?? ``
  const shortcode =
    emoji.s.find((code) => code.toLowerCase().startsWith(q)) ?? emoji.s[0]
  return (
    <TypeaheadRow active={active} onSelect={onSelect} onMouseEnter={onHover}>
      <span className="emoji-glyph w-6 shrink-0 text-center text-base leading-none">
        {unicode}
      </span>
      {shortcode && (
        <span className="shrink-0 font-mono text-xs text-muted-foreground">
          :{shortcode}:
        </span>
      )}
      <span className="truncate text-muted-foreground">{emoji.l}</span>
    </TypeaheadRow>
  )
}

/** EXP-724: a steering slash-command candidate — the mono `/name`, what it
 *  does, and a muted `<hint>` when it takes an argument. */
export function CommandCandidateRow({
  command,
  active,
  onSelect,
  onHover,
}: {
  command: SteerCommand
  active: boolean
  onSelect: () => void
  onHover: () => void
}) {
  return (
    <TypeaheadRow active={active} onSelect={onSelect} onMouseEnter={onHover}>
      <span className="shrink-0 font-mono text-xs">/{command.name}</span>
      <span className="truncate text-muted-foreground">
        {command.description}
      </span>
      {command.argHint && (
        <span className="ml-auto shrink-0 font-mono text-xs text-muted-foreground/70">
          {`<${command.argHint}>`}
        </span>
      )}
    </TypeaheadRow>
  )
}

/** NEW-2: a team prompt in the `/` menu — its icon and name, after the
 *  builtin commands. */
export function PromptCandidateRow({
  prompt,
  active,
  onSelect,
  onHover,
}: {
  prompt: SlashPrompt
  active: boolean
  onSelect: () => void
  onHover: () => void
}) {
  const Icon = getActionIcon(prompt)
  return (
    <TypeaheadRow active={active} onSelect={onSelect} onMouseEnter={onHover}>
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <span className="truncate">{prompt.name}</span>
    </TypeaheadRow>
  )
}
