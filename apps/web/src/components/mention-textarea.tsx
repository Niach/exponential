import { useRef, useState } from "react"
import type { User } from "@/db/schema"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Textarea } from "@/components/ui/textarea"

// Matches an in-progress mention `@query` at the caret (after start-of-text or
// whitespace). The query stops at whitespace so it won't swallow the rest.
const MENTION_AT_CARET = /(?:^|\s)@([a-zA-Z0-9._%+-]*)$/

interface MentionTextareaProps
  extends Omit<
    React.ComponentProps<typeof Textarea>,
    `value` | `onChange`
  > {
  value: string
  onValueChange: (next: string) => void
  // Workspace members to offer; agents are filtered out (you mention people).
  users: User[]
}

// A Textarea with @-mention autocomplete. Selecting a member inserts the
// canonical `@<email>` form the server resolves (lib/integrations/mentions.ts).
export function MentionTextarea({
  value,
  onValueChange,
  users,
  onKeyDown,
  ...props
}: MentionTextareaProps) {
  const ref = useRef<HTMLTextAreaElement>(null)
  const [menu, setMenu] = useState<{ query: string; start: number } | null>(
    null
  )
  const [active, setActive] = useState(0)

  const people = users.filter((u) => !u.isAgent)
  const candidates = menu
    ? people
        .filter(
          (u) =>
            u.name.toLowerCase().includes(menu.query) ||
            u.email.toLowerCase().includes(menu.query)
        )
        .slice(0, 6)
    : []

  const sync = (next: string, caret: number) => {
    onValueChange(next)
    const before = next.slice(0, caret)
    const m = before.match(MENTION_AT_CARET)
    if (m) {
      setMenu({ query: m[1].toLowerCase(), start: caret - m[1].length - 1 })
      setActive(0)
    } else {
      setMenu(null)
    }
  }

  const insert = (user: User) => {
    if (!menu) return
    const el = ref.current
    const caret = el?.selectionStart ?? value.length
    const next = `${value.slice(0, menu.start)}@${user.email} ${value.slice(caret)}`
    const nextCaret = menu.start + user.email.length + 2
    onValueChange(next)
    setMenu(null)
    requestAnimationFrame(() => {
      if (el) {
        el.focus()
        el.setSelectionRange(nextCaret, nextCaret)
      }
    })
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (menu && candidates.length > 0) {
      if (e.key === `ArrowDown`) {
        e.preventDefault()
        setActive((a) => (a + 1) % candidates.length)
        return
      }
      if (e.key === `ArrowUp`) {
        e.preventDefault()
        setActive((a) => (a - 1 + candidates.length) % candidates.length)
        return
      }
      if (e.key === `Enter` || e.key === `Tab`) {
        e.preventDefault()
        insert(candidates[active])
        return
      }
      if (e.key === `Escape`) {
        e.preventDefault()
        setMenu(null)
        return
      }
    }
    onKeyDown?.(e)
  }

  return (
    <div className="relative flex-1">
      <Textarea
        {...props}
        ref={ref}
        value={value}
        onChange={(e) => sync(e.target.value, e.target.selectionStart ?? 0)}
        onKeyDown={handleKeyDown}
      />
      {menu && candidates.length > 0 && (
        <div className="absolute bottom-full z-20 mb-1 w-64 overflow-hidden rounded-md border bg-popover shadow-md">
          {candidates.map((u, i) => (
            <button
              key={u.id}
              type="button"
              onMouseDown={(e) => {
                e.preventDefault()
                insert(u)
              }}
              onMouseEnter={() => setActive(i)}
              className={`flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm ${
                i === active ? `bg-accent` : ``
              }`}
            >
              <Avatar className="size-5">
                {u.image && <AvatarImage src={u.image} alt={u.name} />}
                <AvatarFallback className="text-[0.5625rem]">
                  {getInitials(u.name)}
                </AvatarFallback>
              </Avatar>
              <span className="truncate">{u.name}</span>
              <span className="ml-auto truncate text-xs text-muted-foreground">
                {u.email}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
