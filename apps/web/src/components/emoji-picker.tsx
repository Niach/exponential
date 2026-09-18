import { useState, type ReactNode } from "react"
import { EmojiPickerPopover as EmojiPickerPopoverView } from "@exp/ui"
import {
  pushRecentEmoji,
  readRecentEmoji,
  useEmojiData,
  type EmojiRecord,
} from "@/lib/emoji"

// EXP-551 — the emoji picker's DATA half. The grid, the search and the
// popover are `EmojiPickerPopover` in @exp/ui; this file is what only the app
// can do: lazy-load the dataset on first open (`useEmojiData`, the ~245KB
// chunk) and keep the per-device recents.

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

export function EmojiPickerPopover({
  children,
  onPick,
  align,
  side,
  onOpenChange,
}: EmojiPickerPopoverProps) {
  // The popover owns its own open state; this mirror is what gates the load.
  const [open, setOpen] = useState(false)
  const data = useEmojiData(open)
  const [recent, setRecent] = useState<string[]>(() => readRecentEmoji())
  return (
    <EmojiPickerPopoverView
      data={data}
      recent={recent}
      align={align}
      side={side}
      onOpenChange={(next) => {
        setOpen(next)
        onOpenChange?.(next)
      }}
      onPick={(unicode, emoji) => {
        setRecent(pushRecentEmoji(emoji.u))
        onPick(unicode, emoji)
      }}
    >
      {children}
    </EmojiPickerPopoverView>
  )
}
