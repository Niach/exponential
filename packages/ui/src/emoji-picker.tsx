import {
  memo,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { Button } from "./button"
import { ListEmpty } from "./empty-state"
import {
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
} from "./mobile-popover"
import { SearchField } from "./search-field"
import { Skeleton } from "./skeleton"
import { useIsMobile } from "./use-mobile"
import { searchEmoji, type EmojiData, type EmojiRecord } from "./emoji-search"

// EXP-551 — the emoji picker shared by the description editor toolbar and the
// comment composer. Search + a "Recent" row + the nine dataset groups; a pick
// hands the caller the UNICODE to insert and the base record (always the base
// yellow glyph — EXP-600 dropped the skin-tone row). The DATA is passed in:
// the app's `components/emoji-picker.tsx` is the binding that lazy-loads the
// dataset (`useEmojiData`) and keeps the per-device recents.

const SEARCH_LIMIT = 64

interface EmojiPickerProps {
  /** The indexed dataset, or null while it loads (the skeleton). */
  data: EmojiData | null
  /** Recently picked BASE unicodes, most recent first — resolved against the
   *  dataset, so an emoji this build no longer ships simply drops out. */
  recent: readonly string[]
  onPick: (unicode: string, emoji: EmojiRecord) => void
  /** Focus the search field on mount (off on phones: the keyboard would
   *  cover the grid). */
  autoFocusSearch?: boolean
}

export function EmojiPicker({
  data,
  recent,
  onPick,
  autoFocusSearch,
}: EmojiPickerProps) {
  const [query, setQuery] = useState(``)
  const deferredQuery = useDeferredValue(query)
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
        <SearchField
          size="md"
          ref={searchRef}
          value={query}
          onValueChange={setQuery}
          onKeyDown={(event) => {
            if (event.key === `Enter`) {
              event.preventDefault()
              if (results[0]) pick(results[0])
            }
          }}
          placeholder="Search emoji…"
          aria-label="Search emoji"
          clearLabel="Clear emoji search"
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
            <ListEmpty>No emoji found</ListEmpty>
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
        // The 36px cell IS the ghost icon button (`size="icon"`), stretched
        // across its grid column: the hover/focus wash comes from the variant,
        // only the colour-emoji face and the glyph size are the picker's own.
        <Button
          key={emoji.u}
          variant="ghost"
          size="icon"
          title={emoji.l}
          aria-label={emoji.l}
          onClick={() => onPick(emoji)}
          className="emoji-glyph w-full text-xl"
        >
          {emoji.u}
        </Button>
      ))}
    </div>
  )
})

interface EmojiPickerPopoverProps {
  /** The trigger element (rendered `asChild`). */
  children: ReactNode
  data: EmojiData | null
  recent: readonly string[]
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
  data,
  recent,
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
            data={data}
            recent={recent}
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
