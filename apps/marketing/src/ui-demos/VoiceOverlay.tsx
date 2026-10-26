/* NEW-15: the push-to-talk overlay (#NEW-54) as the home page and /voice/
   show it: THE capsule (the bulk bar's, as the IDE's `voice_overlay::capsule`
   and the styleguide `voice-call-overlay` entry draw it) holding `VoiceWave`
   (md, the brand mark as its child), the state, the muted line and one pill.
   The rows are the fixture's `overlaySpecimens`, copied here because
   marketing does not declare @exp/domain-contract. */
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
    <div className="inline-flex max-w-full items-center gap-1 rounded-3xl border border-glass-stroke-strong bg-glass-card-opaque px-2.5 py-2 shadow-lg shadow-black/40">
      <VoiceWave activity={row.activity} level={row.level} bands={row.bands} size="md" label="Voice">
        <ExponentialLogo variant="light" />
      </VoiceWave>
      <span className="shrink-0 px-1 text-sm font-semibold whitespace-nowrap text-foreground">{row.state}</span>
      <span className="min-w-0 truncate px-2 text-sm text-muted-foreground">{row.line}</span>
      <Pill size="sm" mode="action" primary={row.primary}>
        {row.pill}
      </Pill>
    </div>
  )
}

export function VoiceOverlay() {
  return (
    <div className="flex min-w-0 flex-col items-start gap-3 bg-background p-4 sm:p-6">
      {OVERLAY_ROWS.map((row) => (
        <VoiceCapsule key={row.activity} row={row} />
      ))}
    </div>
  )
}
