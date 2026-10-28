import type { ComponentProps, ReactNode } from "react"
import {
  VOICE_CALL_COPY,
  VOICE_CALL_ICONS,
  VOICE_CALL_PILL,
  VOICE_CALL_TEST_IDS,
  type CallActivity,
  type CallButton as CallButtonId,
} from "@exp/domain-contract/voice-call"
import type { IconConcept } from "@exp/icons"

import { Button } from "./button"
import { cn } from "./cn"
import { IconTooltip } from "./icon-tooltip"
import { conceptIcon } from "./icons.generated"
import { VoiceWave, type VoiceWaveActivity } from "./voice-wave"

// S-3 (voice-call.json PHONE-CALL UI, the `call-controls` styleguide entry):
// THE round icon-only call buttons (WhatsApp / Teams style) and the rows
// they sit in. `CallButton` is the shadcn Button at the eUI round size:
// `neutral` = the muted surface fill (`pressed` = inverted, the toggled
// mute), `destructive` = the red end button, its label as the tooltip and
// the accessibility label, or written UNDER it (`labelUnder`, the ring's
// Cancel). `CallControls` lays out `callView`'s `buttons` in order;
// `VoiceCallPill` is the compact row (the desktop's floating pill: a drag
// handle, the avatar small, the name over the timer, VoiceWave sm, mute and
// end); `CallAvatar` is the agent's disc with the ring of its activity.

export type CallButtonTone = `neutral` | `destructive`
/** `md` = 56 px (the call surface), `sm` = 36 px (the pill). */
export type CallButtonSize = `md` | `sm`

const CALL_BUTTON_TONE: Record<CallButtonTone, string> = {
  neutral: `bg-secondary text-foreground hover:bg-secondary/80 aria-pressed:bg-foreground aria-pressed:text-background aria-pressed:hover:bg-foreground/90`,
  // The red fill at full strength in dark too (the Button's destructive
  // variant dims it to 60% there): an end button reads as THE red button.
  destructive: `text-white dark:bg-destructive dark:hover:bg-destructive/90`,
}

export interface CallButtonProps extends Omit<ComponentProps<`button`>, `children`> {
  icon: IconConcept
  /** The tooltip and the accessibility label (or the words under it). */
  label: string
  tone?: CallButtonTone
  /** A toggle (mute): true paints it inverted; undefined = no toggle. */
  pressed?: boolean
  size?: CallButtonSize
  /** Write the label under the button instead of a tooltip (ringing's Cancel). */
  labelUnder?: boolean
}

export function CallButton({
  icon,
  label,
  tone = `neutral`,
  pressed,
  size = `md`,
  labelUnder = false,
  className,
  ...props
}: CallButtonProps) {
  const Icon = conceptIcon(icon)
  const button = (
    <Button
      type="button"
      variant={tone === `destructive` ? `destructive` : `secondary`}
      size={size === `md` ? `icon-lg` : `icon`}
      aria-label={label}
      aria-pressed={pressed}
      data-slot="call-button"
      data-tone={tone}
      className={cn(CALL_BUTTON_TONE[tone], className)}
      {...props}
    >
      <Icon />
    </Button>
  )
  if (labelUnder) {
    return (
      <span data-slot="call-button-labelled" className="inline-flex flex-col items-center gap-1.5">
        {button}
        <span aria-hidden className="text-xs text-muted-foreground">
          {label}
        </span>
      </span>
    )
  }
  return <IconTooltip label={label}>{button}</IconTooltip>
}

export interface CallControlsProps {
  /** `callView`'s buttons, in order. */
  buttons: readonly CallButtonId[]
  muted: boolean
  speaker?: boolean
  /** `callView`'s `end` (Cancel while ringing, written under it). */
  end: string
  /** Ringing: the end button's label shows under it. */
  ringing?: boolean
  size?: CallButtonSize
  onMute?: () => void
  onSpeaker?: () => void
  onEnd?: () => void
  className?: string
}

/** One call button per `buttons` entry: the icons, labels and testIds are
 *  voice-call.json's. */
export function CallControls({
  buttons,
  muted,
  speaker = true,
  end,
  ringing = false,
  size = `md`,
  onMute,
  onSpeaker,
  onEnd,
  className,
}: CallControlsProps) {
  const copy = VOICE_CALL_COPY
  const icons = VOICE_CALL_ICONS
  const ids = VOICE_CALL_TEST_IDS
  return (
    <div
      data-slot="call-controls"
      className={cn(`flex items-start justify-center`, size === `md` ? `gap-8` : `gap-1.5`, className)}
    >
      {buttons.map((button) => {
        if (button === `mute`) {
          return (
            <CallButton
              key={button}
              icon={(muted ? icons.unmute : icons.mute) as IconConcept}
              label={muted ? copy.unmute : copy.mute}
              pressed={muted}
              size={size}
              onClick={onMute}
              data-testid={ids.mute}
            />
          )
        }
        if (button === `speaker`) {
          return (
            <CallButton
              key={button}
              icon={(speaker ? icons.speaker : icons.speakerOff) as IconConcept}
              label={speaker ? copy.speaker : copy.speakerOff}
              size={size}
              onClick={onSpeaker}
              data-testid={ids.speaker}
            />
          )
        }
        return (
          <CallButton
            key={button}
            icon={icons.end as IconConcept}
            label={end}
            tone="destructive"
            size={size}
            labelUnder={ringing}
            onClick={onEnd}
            data-testid={ids.end}
          />
        )
      })}
    </div>
  )
}

const ACTIVITY_WORD: Record<CallActivity, string> = {
  ringing: VOICE_CALL_COPY.calling,
  listening: VOICE_CALL_COPY.listening,
  thinking: VOICE_CALL_COPY.thinking,
  speaking: VOICE_CALL_COPY.speaking,
  muted: VOICE_CALL_COPY.muted,
}

export interface CallAvatarProps {
  /** The agent's name (with the activity, the accessibility label). */
  agent: string
  activity: CallActivity
  level?: number
  bands?: number[]
  /** The agent's mark (the Assistant: the Exponential logo). */
  children: ReactNode
  /** `lg` = the call surface (a 96 px disc), `sm` = the pill (28 px). */
  size?: `lg` | `sm`
  className?: string
}

/** The agent's disc with the ring of `activity`: ringing a soft halo pulse,
 *  listening the success ring glowing with the level, speaking the wave,
 *  thinking dim, muted none. */
export function CallAvatar({ agent, activity, level = 0, bands, children, size = `lg`, className }: CallAvatarProps) {
  const lg = size === `lg`
  const glow = Math.round(6 + Math.min(1, Math.max(0, level)) * 22)
  return (
    <span
      data-slot="call-avatar"
      data-activity={activity}
      role="img"
      aria-label={`${agent}, ${ACTIVITY_WORD[activity]}`}
      className={cn(`relative inline-flex shrink-0 items-center justify-center`, lg ? `size-50` : `size-7`, className)}
    >
      {lg && activity === `ringing` ? (
        <>
          <span aria-hidden className="absolute size-44 rounded-full bg-foreground/[0.04] motion-safe:animate-agent-pulse" />
          <span aria-hidden className="absolute size-34 rounded-full bg-foreground/[0.07] motion-safe:animate-agent-pulse" />
        </>
      ) : null}
      {lg && activity === `speaking` ? (
        <VoiceWave activity={`speaking` as VoiceWaveActivity} level={level} bands={bands} size="xl" className="absolute" />
      ) : null}
      <span
        aria-hidden
        data-slot="call-avatar-disc"
        className={cn(
          `relative flex items-center justify-center overflow-hidden rounded-full bg-card text-foreground transition-[opacity,box-shadow] duration-standard`,
          lg ? `size-24 [&_svg]:size-14` : `size-7 [&_svg]:size-4`,
          activity === `listening` && `ring-emerald-500/80`,
          activity === `listening` && (lg ? `ring-4` : `ring-2`),
          activity === `thinking` && `opacity-60`
        )}
        style={activity === `listening` && lg ? { boxShadow: `0 0 ${glow}px 2px rgb(16 185 129 / 0.35)` } : undefined}
      >
        {children}
      </span>
    </span>
  )
}

/** The pill's drag handle: two columns of dots (no glyph: it is a texture). */
function DragHandle() {
  return (
    <span aria-hidden data-slot="call-pill-handle" className="grid shrink-0 grid-cols-2 gap-0.5 px-0.5">
      {Array.from({ length: 6 }, (_, k) => (
        <span key={k} className="size-[3px] rounded-full bg-muted-foreground/70" />
      ))}
    </span>
  )
}

export interface VoiceCallPillProps extends Omit<ComponentProps<`div`>, `children`> {
  agent: string
  avatar: ReactNode
  /** The timer (or, ringing, the status line). */
  status: string
  activity: CallActivity
  level?: number
  bands?: number[]
  muted: boolean
  /** `callView`'s buttons; the pill shows only mute and end. */
  buttons: readonly CallButtonId[]
  end: string
  onMute?: () => void
  onEnd?: () => void
}

/** The compact row: the desktop's floating call pill (`pill.width` ×
 *  `pill.height`, fixed: the name and the status truncate). */
export function VoiceCallPill({
  agent,
  avatar,
  status,
  activity,
  level = 0,
  bands,
  muted,
  buttons,
  end,
  onMute,
  onEnd,
  className,
  style,
  ...props
}: VoiceCallPillProps) {
  const wave: VoiceWaveActivity = activity === `ringing` ? `idle` : activity
  return (
    <div
      data-slot="call-pill"
      data-testid={VOICE_CALL_TEST_IDS.pill}
      className={cn(
        `flex shrink-0 items-center gap-2 rounded-full border border-glass-stroke-card bg-glass-card pr-2.5 pl-3 shadow-lg`,
        className
      )}
      style={{ width: VOICE_CALL_PILL.width, height: VOICE_CALL_PILL.height, ...style }}
      {...props}
    >
      <DragHandle />
      <CallAvatar agent={agent} activity={activity} size="sm">
        {avatar}
      </CallAvatar>
      <span className="flex min-w-0 flex-1 flex-col leading-tight">
        <span className="truncate text-sm font-medium text-foreground">{agent}</span>
        <span data-testid={VOICE_CALL_TEST_IDS.timer} className="truncate text-xs text-muted-foreground tabular-nums">
          {status}
        </span>
      </span>
      <VoiceWave activity={wave} level={level} bands={bands} size="sm" />
      <CallControls
        buttons={buttons.filter((button) => button !== `speaker`)}
        muted={muted}
        end={end}
        size="sm"
        onMute={onMute}
        onEnd={onEnd}
      />
    </div>
  )
}
