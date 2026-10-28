import type { HTMLAttributes, ReactNode } from "react"
import { VOICE_CALL_TEST_IDS, type CallView } from "@exp/domain-contract/voice-call"

import { CallAvatar, CallControls } from "./call-controls"
import { cn } from "./cn"

// S-3 (voice-call.json PHONE-CALL UI, the `voice-call` styleguide entry):
// THE call surface, WhatsApp / Teams style, the same on web (a full-height
// overlay over the agent chat) and in the phones' full screen. Top to
// bottom: the agent's avatar large with the ring of its activity
// (`CallAvatar`), the agent's name, the status line (null = the call's
// TIMER, testId `timer`), the caption (centred, at most 4 lines), and at the
// bottom the round call buttons in `callView`'s order (`CallControls`, the
// `call-controls` entry). No text pills and no state word: the activity is
// the avatar's accessibility value. Exponential-specific (the copy is the
// fixture's), so it is no eUI primitive; the round button is.

export interface VoiceCallSurfaceProps extends Omit<HTMLAttributes<HTMLDivElement>, `onClick`> {
  agent: string
  /** The agent's mark (the Assistant: the Exponential logo). */
  avatar: ReactNode
  /** `callView(state, platform)`. */
  view: CallView
  /** `callTimer(ms since answered)`, shown while `view.status` is null. */
  timer: string
  level?: number
  bands?: number[]
  onMute?: () => void
  onSpeaker?: () => void
  onEnd?: () => void
}

export function VoiceCallSurface({
  agent,
  avatar,
  view,
  timer,
  level = 0,
  bands,
  onMute,
  onSpeaker,
  onEnd,
  className,
  ...props
}: VoiceCallSurfaceProps) {
  const ringing = view.activity === `ringing`
  return (
    <div
      data-slot="voice-call"
      data-testid={VOICE_CALL_TEST_IDS.surface}
      className={cn(
        `flex h-full min-h-0 w-full flex-col items-center justify-between gap-6 bg-background px-6 pt-[12vh] pb-12`,
        className
      )}
      {...props}
    >
      <div className="flex min-h-0 w-full max-w-sm flex-col items-center text-center">
        <CallAvatar agent={agent} activity={view.activity} level={level} bands={bands}>
          {avatar}
        </CallAvatar>
        <h2 className="mt-2 max-w-full truncate text-2xl font-semibold tracking-tight text-foreground">{agent}</h2>
        {view.status === null ? (
          <span data-testid={VOICE_CALL_TEST_IDS.timer} className="mt-1 text-sm text-muted-foreground tabular-nums">
            {timer}
          </span>
        ) : (
          <span className="mt-1 text-sm text-muted-foreground">{view.status}</span>
        )}
        <p aria-live="polite" className="mt-8 line-clamp-4 min-h-6 text-base leading-relaxed text-foreground/90">
          {view.caption}
        </p>
      </div>
      <CallControls
        buttons={view.buttons}
        muted={view.muted}
        speaker={view.speaker}
        end={view.end}
        ringing={ringing}
        onMute={onMute}
        onSpeaker={onSpeaker}
        onEnd={onEnd}
      />
    </div>
  )
}
