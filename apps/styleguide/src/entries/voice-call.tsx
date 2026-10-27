import call from "@exp/domain-contract/fixtures/voice-call.json"
import { VoiceCallCard, type VoiceWaveActivity } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// NEW-55 — the call surface on phones (full screen, V5 #NEW-69) and web (a centred
// card, V6): the #1022 draft promoted. A glass card holding `VoiceWave` (xl,
// the brand mark as its child), the state, the line (what was heard, the
// queue, or what is said), then End call as the ONE primary pill and Mute
// beside it. The words and rows are `fixtures/voice-call.json`
// (`surfaceSpecimens`, `copy`); the desktop shows the same states in THE
// capsule (`voice-call-overlay`). NEW-70: the card is `@exp/ui`'s
// `VoiceCallCard`, the one web's call draws.

interface Specimen {
  activity: VoiceWaveActivity
  level: number
  bands: number[]
  state: string
  line: string
  mute: string
}

const SPECIMENS = call.surfaceSpecimens as Specimen[]

export const entry: StyleguideEntry = {
  id: `voice-call`,
  section: `special`,
  owner: `NEW-55`,
  title: `Voice call`,
  blurb: `A call with an agent on a phone (full screen) or on web (a centred card), streamed to the member's voice device: VoiceWave (xl) with the brand mark, the state (Connecting · Listening · Thinking · Speaking · Microphone muted), the line (what was heard, "1 queued · …" while the agent's turn runs, or what is said), then End call as the one primary pill and Mute. It opens from the telephone in the agent chat's header and stays until End call. The desktop shows the same states in the voice capsule (voice-call-overlay).`,
  status: {
    web: { state: `ok`, symbol: `VoiceCallCard`, file: `packages/ui/src/voice-call-card.tsx` },
    desktop: { state: `n/a`, note: `The voice device draws a call in the capsule (voice-call-overlay); the twin paints this card statically.` },
    ios: {
      state: `ok`,
      symbol: `VoiceCallCard`,
      file: `apps/ios/Exponential/UI/Voice/VoiceCallScreen.swift`,
      note: `Full screen over the app background (#NEW-69).`,
    },
    android: {
      state: `ok`,
      symbol: `VoiceCallCard`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/call/VoiceCallSurface.kt`,
      note: `Full screen over the app background (#NEW-69).`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-4">
      {SPECIMENS.map((s) => (
        <VoiceCallCard
          key={s.state}
          activity={s.activity}
          level={s.level}
          bands={s.bands}
          state={s.state}
          line={s.line}
          mute={s.mute}
          muted={s.activity === `muted`}
          endLabel={call.copy.endCall}
          data-testid={call.testIds.surface}
          endTestId={call.testIds.end}
          muteTestId={call.testIds.mute}
        />
      ))}
    </div>
  ),
}
