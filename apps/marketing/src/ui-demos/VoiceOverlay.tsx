/* NEW-15: the push-to-talk overlay (#NEW-54) as the home page and /talk/
   show it: THE capsule (the bulk bar's, as the IDE's `voice_overlay::capsule`
   and the styleguide `voice-call-overlay` entry draw it) holding `VoiceWave`
   (md, the brand mark as its child), the state, the muted line and one pill.
   The rows are the fixture's `overlaySpecimens`, copied here because
   marketing does not declare @exp/domain-contract. Geometry = `glass_bar`
   (exponential-ui-gpui controls/rows.rs: gap 4, padding 8/10, radius xl3,
   1px stroke-strong, the opaque popover fill, NO shadow) with the IDE's
   `value` (px 4, 14px semibold) and `item` (32px tall, px 8, 14px muted,
   the one that grows and ellipsizes). */
import { ExponentialLogo, Pill, VoiceWave, type VoiceWaveActivity } from "@exp/ui"

export interface OverlayRow {
  activity: VoiceWaveActivity
  level: number
  bands: number[]
  state: string
  line: string
  pill: string
  primary: boolean
}

export const OVERLAY_ROWS: OverlayRow[] = [
  { activity: `listening`, level: 0.7, bands: [0.9, 0.6, 1, 0.5], state: `Listening`, line: `“create an issue push to talk breaks under Wayland”`, pill: `Studio`, primary: false },
  { activity: `thinking`, level: 0, bands: [], state: `Heard`, line: `“create an issue push to talk breaks under Wayland”`, pill: `Studio`, primary: false },
  { activity: `speaking`, level: 0.5, bands: [0.7, 1, 0.5, 0.8], state: `Speaking`, line: `Created NEW-81 “Push to talk breaks under Wayland”.`, pill: `Stop`, primary: true },
]

export function VoiceCapsule({ row }: { row: OverlayRow }) {
  return (
    <div className="inline-flex max-w-full items-center gap-1 rounded-3xl border border-glass-stroke-strong bg-glass-card-opaque px-2.5 py-2">
      <VoiceWave activity={row.activity} level={row.level} bands={row.bands} size="md" label="Voice">
        <ExponentialLogo variant="light" />
      </VoiceWave>
      <span className="shrink-0 px-1 text-sm font-semibold whitespace-nowrap text-foreground">{row.state}</span>
      <span className="block h-8 min-w-0 flex-1 truncate px-2 text-sm leading-8 text-muted-foreground">{row.line}</span>
      <Pill size="sm" mode="action" primary={row.primary}>
        {row.pill}
      </Pill>
    </div>
  )
}

export const LAST_STEP = OVERLAY_ROWS.length - 1

/** Listening and Heard hold a beat; Speaking holds for the film's own hold. */
export function frameMs(step: number): number {
  return step === 0 ? 2600 : 1600
}

/** One capsule at `step` (0 = Listening, the prerendered resting frame). */
export function VoiceOverlay({ step = 0 }: { step?: number }) {
  return (
    <div className="flex min-w-0 justify-center bg-background p-4 sm:p-6">
      <VoiceCapsule row={OVERLAY_ROWS[Math.min(step, LAST_STEP)]} />
    </div>
  )
}
