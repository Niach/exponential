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
  applySkinTone,
  pushRecentEmoji,
  readRecentEmoji,
  readSkinTone,
  searchEmoji,
  SKIN_TONES,
  useEmojiData,
  writeSkinTone,
  type EmojiRecord,
  type SkinTone,
} from "@/lib/emoji"
import { cn } from "@/lib/utils"

// EXP-551 — the emoji picker shared by the description editor toolbar and the
// comment composer. Search + skin tone + a "Recent" row + the nine dataset
// groups; a pick hands the caller the UNICODE to insert (tone applied) and
// the base record. Data loads lazily on first open (`useEmojiData`).

/** The swatch glyphs of the skin-tone row: ✋ untoned, then light → dark. */
const TONE_SWATCHES: Record<SkinTone, string> = {
  0: `✋`,
  1: `✋\u{1F3FB}`,
  2: `✋\u{1F3FC}`,
  3: `✋\u{1F3FD}`,
  4: `✋\u{1F3FE}`,
  5: `✋\u{1F3FF}`,
}
const TONE_LABELS: Record<SkinTone, string> = {
  0: `No skin tone`,
  1: `Light skin tone`,
  2: `Medium-light skin tone`,
  3: `Medium skin tone`,
  4: `Medium-dark skin tone`,
  5: `Dark skin tone`,
}

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
  const [tone, setTone] = useState<SkinTone>(() => readSkinTone())
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
    onPick(applySkinTone(emoji, tone), emoji)
  }

  const chooseTone = (next: SkinTone) => {
    setTone(next)
    writeSkinTone(next)
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
        <div
          className="flex shrink-0 items-center gap-px"
          role="radiogroup"
          aria-label="Skin tone"
        >
          {SKIN_TONES.map((option) => (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={option === tone}
              aria-label={TONE_LABELS[option]}
              title={TONE_LABELS[option]}
              onClick={() => chooseTone(option)}
              className={cn(
                `emoji-glyph flex h-6 w-5 items-center justify-center rounded text-sm leading-none hover:bg-accent`,
                option === tone && `bg-accent ring-1 ring-ring`
              )}
            >
              {TONE_SWATCHES[option]}
            </button>
          ))}
        </div>
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
            <EmojiGrid emojis={results} tone={tone} onPick={pick} />
          )
        ) : (
          <>
            {recentRecords.length > 0 && (
              <EmojiSection
                label="Recent"
                emojis={recentRecords}
                tone={tone}
                onPick={pick}
              />
            )}
            {data.groups.map((group) => (
              <EmojiSection
                key={group.index}
                label={group.label}
                emojis={group.emojis}
                tone={tone}
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
  tone,
  onPick,
}: {
  label: string
  emojis: EmojiRecord[]
  tone: SkinTone
  onPick: (emoji: EmojiRecord) => void
}) {
  return (
    <section className="emoji-picker-section" aria-label={label}>
      <h3 className="sticky top-0 z-10 bg-popover/95 px-1 py-1 text-[0.6875rem] font-medium uppercase tracking-wide text-muted-foreground backdrop-blur">
        {label}
      </h3>
      <EmojiGrid emojis={emojis} tone={tone} onPick={onPick} />
    </section>
  )
})

const EmojiGrid = memo(function EmojiGrid({
  emojis,
  tone,
  onPick,
}: {
  emojis: EmojiRecord[]
  tone: SkinTone
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
          {applySkinTone(emoji, tone)}
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
