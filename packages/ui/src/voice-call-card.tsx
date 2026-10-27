import type { HTMLAttributes } from "react"

import { cn } from "./cn"
import { ExponentialLogo } from "./exponential-logo"
import { GlassCard } from "./glass-card"
import { Pill } from "./pill"
import { VoiceWave, type VoiceWaveActivity } from "./voice-wave"

// NEW-70 (voice V6): THE call card (`fixtures/voice-call.json`, the
// `voice-call` styleguide entry): the centred card a web call draws over
// the agent chat, and the phones' full-screen call (V5) in the same order.
// VoiceWave (xl) with the brand mark, the state, the line (what was heard,
// the queue, or what is said), then End call as the ONE primary pill and the
// mute pill beside it. Exponential-specific, so it is no eUI primitive; the
// desktop draws the same states in its voice capsule (voice-call-overlay).
// The frame is THE `GlassCard` (opaque card fill, so the chat scrolling
// behind never shows through); this adds only the layout.

export const VOICE_CALL_CARD_CLASS = `flex w-[22rem] max-w-full flex-col items-center gap-4 px-6 py-6`

export interface VoiceCallCardProps extends Omit<HTMLAttributes<HTMLDivElement>, `onClick`> {
  activity: VoiceWaveActivity
  level: number
  bands?: number[]
  state: string
  line: string
  /** The mute pill's label (Mute / Unmute). */
  mute: string
  muted?: boolean
  endLabel: string
  onEnd?: () => void
  onMute?: () => void
  endTestId?: string
  muteTestId?: string
}

export function VoiceCallCard({
  activity,
  level,
  bands,
  state,
  line,
  mute,
  muted = false,
  endLabel,
  onEnd,
  onMute,
  endTestId,
  muteTestId,
  className,
  ...props
}: VoiceCallCardProps) {
  return (
    <GlassCard data-slot="voice-call" className={cn(VOICE_CALL_CARD_CLASS, className)} {...props}>
      <VoiceWave activity={activity} level={level} bands={bands} size="xl" label="Voice">
        <ExponentialLogo variant="light" size={56} />
      </VoiceWave>
      <div className="flex min-w-0 max-w-full flex-col items-center gap-1 text-center">
        <span className="text-sm font-medium text-foreground" aria-live="polite">
          {state}
        </span>
        <span className="line-clamp-3 min-h-5 text-sm text-muted-foreground">{line}</span>
      </div>
      <div className="flex items-center gap-2">
        <Pill size="md" mode="action" primary onClick={onEnd} data-testid={endTestId}>
          {endLabel}
        </Pill>
        <Pill size="md" mode="action" onClick={onMute} aria-pressed={muted} data-testid={muteTestId}>
          {mute}
        </Pill>
      </div>
    </GlassCard>
  )
}
