import { useState } from "react"
import { Button, conceptIcon, Input } from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

// EXP-1293: a thread's title, renamed in place (`sessions.setTitle`; an
// empty title clears it back to the derived one), with the lock glyph when
// the thread is private.

const PrivateIcon = conceptIcon(`ui-private`)

export function ThreadTitle({
  sessionId,
  title,
  stored,
  isPrivate,
  canRename,
  className,
}: {
  sessionId: string
  /** The shown title (`threadTitle`, stored or derived). */
  title: string
  /** The stored one (null = derived). */
  stored: string | null
  isPrivate: boolean
  canRename: boolean
  className?: string
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(``)

  const save = async () => {
    setEditing(false)
    const next = draft.trim()
    if (next === (stored ?? ``).trim() || (!stored && next === title)) return
    try {
      const result = await trpc.sessions.setTitle.mutate(
        { sessionId, title: next || null },
        SKIP_ERROR_TOAST
      )
      await sessionCollection.utils.awaitTxId(result.txid)
    } catch (error) {
      failedAction(`rename`, `the session`, error)
    }
  }

  const lock = isPrivate ? (
    <PrivateIcon
      aria-label="Private"
      data-testid="thread-private"
      className="size-4 shrink-0 text-muted-foreground"
    />
  ) : null

  if (editing) {
    return (
      <Input
        autoFocus
        value={draft}
        aria-label="Title"
        className="h-9 text-lg font-semibold md:text-2xl"
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => void save()}
        onKeyDown={(event) => {
          if (event.key === `Enter`) (event.target as HTMLInputElement).blur()
          if (event.key === `Escape`) setEditing(false)
        }}
      />
    )
  }

  return (
    <div className={cn(`flex min-w-0 items-center gap-2`, className)}>
      <h1 className="min-w-0 truncate text-lg font-semibold tracking-tight text-foreground md:text-2xl">
        {canRename ? (
          <Button
            type="button"
            variant="ghost"
            title="Rename"
            data-testid="thread-title"
            className="h-auto max-w-full justify-start truncate rounded-sm px-0 py-0 text-left text-[length:inherit] font-[inherit] hover:bg-accent/40"
            onClick={() => {
              setDraft(stored ?? title)
              setEditing(true)
            }}
          >
            {title}
          </Button>
        ) : (
          <span data-testid="thread-title">{title}</span>
        )}
      </h1>
      {lock}
    </div>
  )
}
