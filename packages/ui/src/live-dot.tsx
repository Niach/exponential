import { cn } from "./cn"

// EXP-887 — the ONE status dot. Half a dozen surfaces drew the same 8px disc
// (and the same two-span emerald ping halo) inline; this is that markup once,
// keyed by what the dot MEANS rather than by a colour.
//
// The tones are the session-dot vocabulary (`session-dot.ts`, hand-mirrored
// with the desktop and both natives) plus the two neutral discs the timeline
// and the inbox draw: a run in flight is emerald, one waiting on a human
// amber, a finished one sky, an unread row the primary accent, a parked or
// ended one muted.
//
// `ping` is the ATTENTION halo — reserve it for something actually happening
// right now (an agent mid-turn); a live-but-idle run draws the steady disc.

/** Exported so the app can LOCK it against `SESSION_DOT_CLASS` — the session
 *  tones are the ×4 table (`session-dot.ts`), and a dot drawn here has to keep
 *  painting exactly what the desktop and the natives paint. */
export const LIVE_DOT_TONE = {
  live: { core: `bg-emerald-500`, halo: `bg-emerald-400` },
  attention: { core: `bg-amber-500`, halo: `bg-amber-400` },
  done: { core: `bg-sky-500`, halo: `bg-sky-400` },
  unread: { core: `bg-primary`, halo: `bg-primary` },
  idle: { core: `bg-muted-foreground/40`, halo: `bg-muted-foreground` },
  muted: { core: `bg-muted-foreground`, halo: `bg-muted-foreground` },
} as const

export type LiveDotTone = keyof typeof LIVE_DOT_TONE

export function LiveDot({
  tone,
  ping = false,
  label,
  className,
}: {
  tone: LiveDotTone
  /** The pulsing halo behind the disc. */
  ping?: boolean
  /** An accessible name for a dot that CARRIES meaning on its own (the
   *  helpdesk's "Awaiting reply"). A decorative dot beside its own label
   *  leaves this off. */
  label?: string
  /** Sizing and layout (`size-1.5`, `shrink-0`, …). Defaults to 8px. */
  className?: string
}) {
  const { core, halo } = LIVE_DOT_TONE[tone]
  if (!ping) {
    return (
      <span
        data-slot="live-dot"
        role={label ? `img` : undefined}
        aria-label={label}
        className={cn(`inline-flex size-2 rounded-full`, core, className)}
      />
    )
  }
  return (
    <span
      data-slot="live-dot"
      data-ping="true"
      role={label ? `img` : undefined}
      aria-label={label}
      className={cn(`relative flex size-2`, className)}
    >
      <span
        className={cn(
          `absolute inline-flex h-full w-full animate-ping rounded-full opacity-60`,
          halo
        )}
      />
      <span
        className={cn(`relative inline-flex size-full rounded-full`, core)}
      />
    </span>
  )
}
