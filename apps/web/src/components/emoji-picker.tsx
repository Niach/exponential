import {
  memo,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "@/components/mobile-popover"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  pushRecentEmoji,
  readRecentEmoji,
  searchEmoji,
  useEmojiData,
  type EmojiRecord,
} from "@/lib/emoji"

// EXP-551 — the emoji picker shared by the description editor toolbar and the
// comment composer. Search + a "Recent" row + the nine dataset groups; a pick
// hands the caller the UNICODE to insert and the base record (always the base
// yellow glyph — EXP-600 dropped the skin-tone row). Data loads lazily on
// first open (`useEmojiData`).

const SEARCH_LIMIT = 64

interface EmojiPickerProps {
  onPick: (unicode: string, emoji: EmojiRecord) => void
  /** Focus the search field on mount (off on phones: the keyboard would
   *  cover the grid). */
  autoFocusSearch?: boolean
}

export function EmojiPicker({ onPick, autoFocusSearch }: EmojiPickerProps) {
  const data = useEmojiData(true)
  const [query, setQuery] = useState(``)
  const deferredQuery = useDeferredValue(query)
  const [recent, setRecent] = useState<string[]>(() => readRecentEmoji())
  const searchRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (autoFocusSearch) {
      // After the popover's own open-autofocus has run.
      const frame = requestAnimationFrame(() => searchRef.current?.focus())
      return () => cancelAnimationFrame(frame)
    }
  }, [autoFocusSearch])

  const results = useMemo(
    () => (data ? searchEmoji(data, deferredQuery, SEARCH_LIMIT) : []),
    [data, deferredQuery]
  )
  const searching = deferredQuery.trim().length > 0

  const recentRecords = useMemo(() => {
    if (!data) return []
    return recent
      .map((u) => data.byUnicode.get(u))
      .filter((e): e is EmojiRecord => Boolean(e))
  }, [data, recent])

  const pick = (emoji: EmojiRecord) => {
    setRecent(pushRecentEmoji(emoji.u))
    onPick(emoji.u, emoji)
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-emoji-picker=""
      role="dialog"
      aria-label="Emoji picker"
    >
      <div className="flex items-center gap-2 px-2 pt-2 pb-1">
        <Input
          ref={searchRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === `Enter`) {
              event.preventDefault()
              if (results[0]) pick(results[0])
            }
          }}
          placeholder="Search emoji…"
          aria-label="Search emoji"
          className="h-8 text-sm"
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-2 pb-2 max-h-[60dvh] md:max-h-72">
        {!data ? (
          <div className="space-y-2 pt-1">
            <Skeleton className="h-4 w-24" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
          </div>
        ) : searching ? (
          results.length === 0 ? (
            <p className="px-1 py-6 text-center text-sm text-muted-foreground">
              No emoji found
            </p>
          ) : (
            <EmojiGrid emojis={results} onPick={pick} />
          )
        ) : (
          <>
            {recentRecords.length > 0 && (
              <EmojiSection
                label="Recent"
                emojis={recentRecords}
                onPick={pick}
              />
            )}
            {data.groups.map((group) => (
              <EmojiSection
                key={group.index}
                label={group.label}
                emojis={group.emojis}
                onPick={pick}
              />
            ))}
          </>
        )}
      </div>
    </div>
  )
}

const EmojiSection = memo(function EmojiSection({
  label,
  emojis,
  onPick,
}: {
  label: string
  emojis: EmojiRecord[]
  onPick: (emoji: EmojiRecord) => void
}) {
  return (
    <section className="emoji-picker-section" aria-label={label}>
      <h3 className="sticky top-0 z-10 bg-popover/95 px-1 py-1 text-[0.6875rem] font-medium uppercase tracking-wide text-muted-foreground backdrop-blur">
        {label}
      </h3>
      <EmojiGrid emojis={emojis} onPick={onPick} />
    </section>
  )
})

const EmojiGrid = memo(function EmojiGrid({
  emojis,
  onPick,
}: {
  emojis: EmojiRecord[]
  onPick: (emoji: EmojiRecord) => void
}) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(2.25rem,1fr))] gap-px">
      {emojis.map((emoji) => (
        <button
          key={emoji.u}
          type="button"
          title={emoji.l}
          aria-label={emoji.l}
          onClick={() => onPick(emoji)}
          className="emoji-glyph flex h-9 items-center justify-center rounded-md text-xl leading-none hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
        >
          {emoji.u}
        </button>
      ))}
    </div>
  )
})

interface EmojiPickerPopoverProps {
  /** The trigger element (rendered `asChild`). */
  children: ReactNode
  onPick: (unicode: string, emoji: EmojiRecord) => void
  align?: `start` | `center` | `end`
  side?: `top` | `bottom`
  /** Mirrors the popover's open state to the host (EXP-568: the formatting
   *  rail must stay alive across the editor blur that opening this causes). */
  onOpenChange?: (open: boolean) => void
}

/**
 * The picker in a popover (bottom sheet on phones). Closes after a pick; the
 * CALLER re-focuses its editor/textarea in `onPick` (this component prevents
 * the popover's close-autofocus so focus does not land on the trigger).
 */
export function EmojiPickerPopover({
  children,
  onPick,
  align = `start`,
  side,
  onOpenChange,
}: EmojiPickerPopoverProps) {
  const [open, setOpen] = useState(false)
  const isMobile = useIsMobile()
  const setOpenState = (next: boolean) => {
    setOpen(next)
    onOpenChange?.(next)
  }
  return (
    <MobilePopover open={open} onOpenChange={setOpenState}>
      <MobilePopoverTrigger asChild>{children}</MobilePopoverTrigger>
      <MobilePopoverContent
        className="w-[21rem] max-w-[calc(100vw-1rem)] p-0"
        align={align}
        side={side}
        mobileTitle="Emoji"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        {open && (
          <EmojiPicker
            autoFocusSearch={!isMobile}
            onPick={(unicode, emoji) => {
              onPick(unicode, emoji)
              setOpenState(false)
            }}
          />
        )}
      </MobilePopoverContent>
    </MobilePopover>
  )
}
