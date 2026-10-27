import call from "@exp/domain-contract/fixtures/voice-call.json"
import { ExponentialLogo, Pill, VoiceWave, type VoiceWaveActivity } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-55 — the call surface on phones (full screen, V5) and web (a centred
// card, V6): the #1022 draft promoted. A glass card holding `VoiceWave` (xl,
// the brand mark as its child), the state, the line (what was heard, the
// queue, or what is said), then End call as the ONE primary pill and Mute
// beside it. The words and rows are `fixtures/voice-call.json`
// (`surfaceSpecimens`, `copy`); the desktop shows the same states in THE
// capsule (`voice-call-overlay`).

interface Specimen {
  activity: VoiceWaveActivity
  level: number
  bands: number[]
  state: string
  line: string
  mute: string
}

const SPECIMENS = call.surfaceSpecimens as Specimen[]
const SURFACE = `flex w-[22rem] flex-col items-center gap-4 rounded-xl border border-glass-stroke-card bg-glass-card px-6 py-6`

function Surface({ s }: { s: Specimen }) {
  return (
    <div className={SURFACE} data-slot="voice-call" data-testid={call.testIds.surface}>
      <VoiceWave activity={s.activity} level={s.level} bands={s.bands} size="xl" label="Voice">
        <ExponentialLogo variant="light" size={56} />
      </VoiceWave>
      <div className="flex flex-col items-center gap-1 text-center">
        <span className="text-sm font-medium text-foreground">{s.state}</span>
        <span className="text-sm text-muted-foreground">{s.line}</span>
      </div>
      <div className="flex items-center gap-2">
        <Pill size="md" mode="action" primary data-testid={call.testIds.end}>
          {call.copy.endCall}
        </Pill>
        <Pill size="md" mode="action" data-testid={call.testIds.mute}>
          {s.mute}
        </Pill>
      </div>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `voice-call`,
  section: `special`,
  owner: `NEW-55`,
  title: `Voice call`,
  blurb: `A call with an agent on a phone (full screen) or on web (a centred card), streamed to the member's voice device: VoiceWave (xl) with the brand mark, the state (Connecting · Listening · Thinking · Speaking · Microphone muted), the line (what was heard, "1 queued · …" while the agent's turn runs, or what is said), then End call as the one primary pill and Mute. It opens from the telephone in the agent chat's header and stays until End call. The desktop shows the same states in the voice capsule (voice-call-overlay).`,
  status: {
    web: { state: `n/a`, note: `Coming with V6 (#NEW-70): the centred card over the agent chat.` },
    desktop: { state: `n/a`, note: `The voice device draws a call in the capsule (voice-call-overlay); the twin paints this card statically.` },
    ios: { state: `n/a`, note: `Coming with V5 (#NEW-69): full screen.` },
    android: { state: `n/a`, note: `Coming with V5 (#NEW-69): full screen.` },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-4">
      {SPECIMENS.map((s) => (
        <Surface key={s.state} s={s} />
      ))}
    </div>
  ),
}
