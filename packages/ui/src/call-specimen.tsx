import type { ReactNode } from "react"
import fixture from "@exp/etalk/fixtures/voice-call.json"
import voice from "@exp/etalk/fixtures/voice.json"
import type { CallPlatform, CallView } from "@exp/etalk/voice-call"
import type { IconConcept } from "@exp/icons"

import { CallButton, CallControls, VoiceCallPill } from "./call-controls"
import { ExponentialLogo } from "./exponential-logo"
import { Pill } from "./pill"
import { VoiceWave, type VoiceWaveActivity } from "./voice-wave"
import { ChatTranscriptSpecimen } from "./chat-specimens"
import { CallTabs, VoiceCallSurface } from "./voice-call-card"

// S-3: the call specimens (voice-call.json `surfaceSpecimens`), ONE source
// for the styleguide entries `voice-call`, `voice-call-overlay` (the pill)
// and `call-controls`, and the web app's DEV-ONLY `?dev=` captures of the
// same ids (the view catalog's web shots).

interface CallSpecimenRow extends CallView {
  name: string
  agent: string
  timer: string | null
  level: number
  bands: number[]
}

export const CALL_SPECIMENS = fixture.surfaceSpecimens as CallSpecimenRow[]

/** A row as `platform` shows it: off a phone there is no speaker button. */
function onPlatform(row: CallSpecimenRow, platform: CallPlatform): CallSpecimenRow {
  return platform === `phone` ? row : { ...row, buttons: row.buttons.filter((button) => button !== `speaker`) }
}

const assistant = <ExponentialLogo variant="light" />

/** The call surfaces side by side, each in a phone-shaped frame. */
export function VoiceCallSpecimen({
  platform = `phone`,
  names,
  frameClassName = `h-[600px] w-[300px]`,
  className = `flex flex-wrap items-start gap-4`,
}: {
  platform?: CallPlatform
  /** The specimen rows to show (default: all). */
  names?: readonly string[]
  frameClassName?: string
  /** The gallery's layout (default: a wrapping row). */
  className?: string
}) {
  const rows = CALL_SPECIMENS.filter((row) => !names || names.includes(row.name)).map((row) => onPlatform(row, platform))
  return (
    <div className={className} data-testid="voice-call-specimen">
      {rows.map((row) => (
        <figure key={row.name} className="grid min-w-0 gap-2">
          <div className={`overflow-hidden rounded-[28px] border border-glass-stroke-card ${frameClassName}`}>
            <VoiceCallSurface
              agent={row.agent}
              avatar={assistant}
              view={row}
              timer={row.timer ?? ``}
              level={row.level}
              bands={row.bands}
              className="pt-20"
            />
          </div>
          <figcaption className="text-center text-xs text-muted-foreground">{row.name}</figcaption>
        </figure>
      ))}
    </div>
  )
}

/** FEED-272: the call's two tabs side by side ("in call, speaking"): Chat =
 *  the tabs, the pill's row docked, the agent chat under it; Voice = the
 *  tabs over the call surface. */
export function VoiceCallTabsSpecimen({
  platform = `phone`,
  frameClassName = `h-[600px] w-[300px]`,
  className = `flex flex-wrap items-start gap-4`,
}: {
  platform?: CallPlatform
  frameClassName?: string
  className?: string
}) {
  const row = onPlatform(CALL_SPECIMENS.find((one) => one.name === `in call, speaking`)!, platform)
  const noop = () => {}
  return (
    <div className={className} data-testid="voice-call-tabs-specimen">
      <figure className="grid min-w-0 gap-2">
        <div className={`flex flex-col overflow-hidden rounded-[28px] border border-glass-stroke-card bg-background ${frameClassName}`}>
          <div className="flex shrink-0 flex-col gap-3 px-3 pt-3">
            <CallTabs tab="chat" onTab={noop} />
            <VoiceCallPill
              docked
              agent={row.agent}
              avatar={assistant}
              status={row.timer ?? ``}
              activity={row.activity}
              level={row.level}
              bands={row.bands}
              muted={row.muted}
              buttons={row.buttons}
              end={row.end}
            />
          </div>
          <div className="min-h-0 flex-1 overflow-hidden px-3 pt-3">
            <ChatTranscriptSpecimen single />
          </div>
        </div>
        <figcaption className="text-center text-xs text-muted-foreground">Chat tab</figcaption>
      </figure>
      <figure className="grid min-w-0 gap-2">
        <div className={`flex flex-col overflow-hidden rounded-[28px] border border-glass-stroke-card bg-background ${frameClassName}`}>
          <div className="shrink-0 px-3 pt-3">
            <CallTabs tab="voice" onTab={noop} />
          </div>
          <div className="flex min-h-0 flex-1">
            <VoiceCallSurface
              agent={row.agent}
              avatar={assistant}
              view={row}
              timer={row.timer ?? ``}
              level={row.level}
              bands={row.bands}
            />
          </div>
        </div>
        <figcaption className="text-center text-xs text-muted-foreground">Voice tab</figcaption>
      </figure>
    </div>
  )
}

interface OverlayCapsule {
  activity: VoiceWaveActivity
  level?: number
  bands?: number[]
  state: string
  line: string
  pill: string
  primary?: boolean
}

/** NEW-77: the push-to-talk overlay's capsule, one per voice.json
 *  `overlaySpecimens` row: THE bar (the bulk bar's capsule) holding
 *  VoiceWave md with the brand mark, the state, the muted line, one pill.
 *  The styleguide entry's capsules, as the IDE draws them. */
export function VoiceOverlayCapsuleSpecimen() {
  return (
    <div className="grid justify-items-start gap-4" data-testid="voice-overlay-capsule-specimen">
      {(voice.overlaySpecimens as OverlayCapsule[]).map((row) => (
        <div
          key={row.state}
          data-slot="voice-overlay"
          className="flex min-w-0 max-w-full items-center gap-1 rounded-3xl border border-glass-stroke-strong bg-glass-card-opaque px-2.5 py-2"
        >
          <VoiceWave activity={row.activity} level={row.level} bands={row.bands} size="md" label="Voice">
            {assistant}
          </VoiceWave>
          <span className="flex-none whitespace-nowrap px-1 text-sm font-semibold text-foreground">{row.state}</span>
          <span className="min-w-0 flex-1 truncate px-2 text-sm text-muted-foreground">{row.line}</span>
          <Pill size="sm" mode="action" primary={row.primary}>
            {row.pill}
          </Pill>
        </div>
      ))}
    </div>
  )
}

/** The desktop's floating call pill, one per specimen row. */
export function VoiceCallPillSpecimen() {
  return (
    <div className="grid justify-items-start gap-4" data-testid="voice-call-pill-specimen">
      {CALL_SPECIMENS.map((row) => (
        <VoiceCallPill
          key={row.name}
          agent={row.agent}
          avatar={assistant}
          status={row.status ?? row.timer ?? ``}
          activity={row.activity}
          level={row.level}
          bands={row.bands}
          muted={row.muted}
          buttons={row.buttons}
          end={row.end}
        />
      ))}
    </div>
  )
}

function Labelled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <figure className="grid justify-items-center gap-3">
      <div className="flex min-h-20 items-start">{children}</div>
      <figcaption className="text-xs text-muted-foreground">{label}</figcaption>
    </figure>
  )
}

/** The round call buttons in every tone, their rows, the compact row. */
export function CallControlsSpecimen() {
  const icons = fixture.icons as Record<keyof typeof fixture.icons, IconConcept>
  const copy = fixture.copy
  const inCall = CALL_SPECIMENS.find((row) => row.name === `in call, speaking`)!
  return (
    <div className="grid gap-8" data-testid="call-controls-specimen">
      <div className="flex flex-wrap items-start gap-8">
        <Labelled label="neutral">
          <CallButton icon={icons.mute} label={copy.mute} pressed={false} />
        </Labelled>
        <Labelled label="pressed">
          <CallButton icon={icons.unmute} label={copy.unmute} pressed />
        </Labelled>
        <Labelled label="speaker">
          <CallButton icon={icons.speaker} label={copy.speaker} />
        </Labelled>
        <Labelled label="speaker off">
          <CallButton icon={icons.speakerOff} label={copy.speakerOff} />
        </Labelled>
        <Labelled label="destructive">
          <CallButton icon={icons.end} label={copy.endCall} tone="destructive" />
        </Labelled>
        <Labelled label="ringing: label under">
          <CallButton icon={icons.end} label={copy.cancel} tone="destructive" labelUnder />
        </Labelled>
      </div>
      <div className="flex flex-wrap items-start gap-12">
        <Labelled label="phones in call">
          <CallControls buttons={[`mute`, `speaker`, `end`]} muted={false} end={copy.endCall} />
        </Labelled>
        <Labelled label="web + desktop in call, muted">
          <CallControls buttons={[`mute`, `end`]} muted end={copy.endCall} />
        </Labelled>
        <Labelled label="ringing">
          <CallControls buttons={[`end`]} muted={false} end={copy.cancel} ringing />
        </Labelled>
      </div>
      <Labelled label="compact row: the floating call pill">
        <VoiceCallPill
          agent={inCall.agent}
          avatar={assistant}
          status={inCall.timer ?? ``}
          activity={inCall.activity}
          level={inCall.level}
          bands={inCall.bands}
          muted={false}
          buttons={[`mute`, `end`]}
          end={copy.endCall}
        />
      </Labelled>
    </div>
  )
}
