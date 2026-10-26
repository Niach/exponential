import { ExponentialLogo, Pill, VoiceWave, type VoiceWaveActivity } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-77 — the voice overlay: the one face for push to talk (#NEW-54), the
// wake word and, later, a call. The compact capsule is the spec: `VoiceWave`
// (the Exponential UI native, md) with the brand mark as its child, one line
// for the state, one for what was heard or said, and one pill. The call card
// under it stays the EXP-1268 draft until caller mode (V4) builds it.

interface Capsule {
  activity: VoiceWaveActivity
  level?: number
  bands?: number[]
  state: string
  line: string
  pill: string
  primary?: boolean
}

const CAPSULES: Capsule[] = [
  { activity: `listening`, level: 0.7, bands: [0.9, 0.6, 1, 0.5], state: `Listening`, line: `“create an issue push to talk breaks under Wayland”`, pill: `Studio` },
  { activity: `thinking`, state: `Heard`, line: `“create an issue push to talk breaks under Wayland”`, pill: `Studio` },
  { activity: `speaking`, level: 0.5, bands: [0.7, 1, 0.5, 0.8], state: `Speaking`, line: `Created NEW-81 “Push to talk breaks under Wayland”.`, pill: `Stop`, primary: true },
  { activity: `muted`, state: `Microphone muted`, line: `Unmute in the tray to talk`, pill: `Unmute` },
]

const CAPSULE = `flex w-[26.25rem] items-center gap-3 rounded-full border border-glass-stroke-strong bg-popover py-2 pr-3.5 pl-2 shadow-lg`
const CARD = `flex flex-col items-center gap-4 rounded-xl border border-glass-stroke-card bg-glass-card px-6 py-6`

function VoiceCapsule({ capsule }: { capsule: Capsule }) {
  return (
    <div className={CAPSULE} data-slot="voice-overlay">
      <VoiceWave activity={capsule.activity} level={capsule.level} bands={capsule.bands} size="md" label="Voice">
        <ExponentialLogo variant="light" />
      </VoiceWave>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="text-sm font-medium text-foreground">{capsule.state}</span>
        <span className="truncate text-sm text-muted-foreground">{capsule.line}</span>
      </div>
      <Pill size="sm" mode="action" primary={capsule.primary}>
        {capsule.pill}
      </Pill>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `voice-call-overlay`,
  section: `special`,
  owner: `NEW-77`,
  title: `Voice overlay`,
  blurb: `The voice device's one face while it listens or speaks (push to talk #NEW-54, the wake word later): a transparent, always-on-top, never-focused capsule at the bottom centre of the display under the pointer, 24 px above the edge. VoiceWave (md) with the brand mark, the state as one line (Listening · Heard · Speaking · Microphone muted), what was heard or said under it, and one pill: the agent while it listens or thinks, Stop while it speaks, Unmute while the tray mutes the microphone. It appears on key-down and fades 1.5 s after the reply. The call card under it is the EXP-1268 draft for caller mode (V4): the same wave at xl, End call as the one primary pill.`,
  status: {
    web: { state: `n/a`, note: `The voice device is a desktop; web streams a call to it later (V6).` },
    desktop: {
      state: `ok`,
      symbol: `voice_overlay::capsule`,
      file: `apps/desktop/crates/ui/src/voice_overlay.rs`,
    },
    ios: { state: `n/a`, note: `Phones stream a call to the voice device later (V5); no push to talk.` },
    android: { state: `n/a`, note: `Phones stream a call to the voice device later (V5); no push to talk.` },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <div className="flex flex-col gap-3">
        {CAPSULES.map((capsule) => (
          <VoiceCapsule key={capsule.state} capsule={capsule} />
        ))}
      </div>
      <div className={`${CARD} w-[22rem]`}>
        <VoiceWave activity="listening" level={0.5} bands={[0.8, 0.5, 1, 0.6]} size="xl" label="Voice">
          <ExponentialLogo variant="light" />
        </VoiceWave>
        <div className="flex flex-col items-center gap-1">
          <span className="text-sm font-medium text-foreground">Listening</span>
          <span className="text-sm text-muted-foreground">“stop the EXP-1242 session and show me the diff”</span>
        </div>
        <div className="flex items-center gap-2">
          <Pill size="md" mode="action" primary>
            End call
          </Pill>
          <Pill size="md" mode="action">
            Mute
          </Pill>
          <Pill size="md">Studio · Haiku</Pill>
        </div>
      </div>
    </div>
  ),
}
