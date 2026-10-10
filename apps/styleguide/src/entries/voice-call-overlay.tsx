import { ExponentialLogo, Pill } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1268 DRAFT — the voice overlay, the one face for push-to-talk, the wake
// word and a call: the brand mark in the middle, the microphone's level as a
// WAVE AROUND it (a polar waveform, never a bar meter), one line for the
// state, one for what was heard, and the actions as pills. A confirm card is
// the same surface with a question and two pills. Nothing is built; the
// geometry below is the proposal (`VoiceWave` = a new `@exp/ui` primitive).

/* A deterministic polar waveform: radius R plus a few harmonics of the
   microphone's bands. Two rings of different phase read as the Siri wave bent
   into a circle; the inner disc is the halo. */
function ring(base: number, amp: number, harmonics: number[], phase: number): string {
  const steps = 180
  const parts: string[] = []
  for (let i = 0; i <= steps; i += 1) {
    const t = (i / steps) * Math.PI * 2
    let r = base
    harmonics.forEach((k, index) => {
      r += (amp / (index + 1)) * Math.sin(k * t + phase * (index + 1))
    })
    const x = 100 + r * Math.cos(t)
    const y = 100 + r * Math.sin(t)
    parts.push(`${i === 0 ? `M` : `L`}${x.toFixed(2)} ${y.toFixed(2)}`)
  }
  return `${parts.join(` `)}Z`
}

const OUTER = ring(66, 7, [11, 17, 23], 0.4)
const MID = ring(60, 6, [9, 15, 21], 1.9)
const INNER = ring(52, 3, [7, 13], 3.1)

function VoiceWave({ size = 200 }: { size?: number }) {
  return (
    <div className="relative shrink-0" data-slot="voice-wave">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width={size}
        height={size}
        viewBox="0 0 200 200"
        aria-hidden="true"
      >
        <path d={OUTER} className="fill-sky-400/10 stroke-sky-400/70" strokeWidth="1.5" />
        <path d={MID} className="fill-primary/10 stroke-primary/80" strokeWidth="1.5" />
        <path d={INNER} className="fill-primary/15 stroke-primary/40" strokeWidth="1" />
      </svg>
      <div className="absolute inset-0 flex items-center justify-center text-foreground">
        <ExponentialLogo variant="light" size={44} />
      </div>
    </div>
  )
}

const SURFACE = `flex flex-col items-center gap-4 rounded-xl border border-glass-stroke-card bg-glass-card px-6 py-6`

export const entry: StyleguideEntry = {
  id: `voice-call-overlay`,
  section: `special`,
  owner: `EXP-1268`,
  title: `Voice call overlay`,
  blurb: `DRAFT for EXP-1268. A transparent always-on-top window on the WORK screen (never the one with a fullscreen game): the brand mark in the middle, the microphone drawn as a wave around it, the state as one line (Listening · Thinking · Speaking) and what Whistle heard under it, then the pills: End call (the one primary), Mute, and where the assistant runs. The same surface asks before a destructive call (the confirm card): the question, Stop as the primary pill, Cancel beside it. Push-to-talk and the wake word show the same overlay and fade after the reply; a call keeps it until End call.`,
  status: {
    web: { state: `n/a`, note: `Draft (EXP-1268): web has no microphone path.` },
    desktop: { state: `n/a`, note: `Draft (EXP-1268): the IDE draws it as a gpui overlay window later.` },
    ios: { state: `n/a`, note: `Draft (EXP-1268): phones later, same surface.` },
    android: { state: `n/a`, note: `Draft (EXP-1268): phones later, same surface.` },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-4">
      <div className={`${SURFACE} w-[22rem]`}>
        <VoiceWave />
        <div className="flex flex-col items-center gap-1">
          <span className="text-sm font-medium text-foreground">Listening</span>
          <span className="text-sm text-muted-foreground">
            “stop the EXP-1242 run and show me the diff”
          </span>
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
      <div className={`${SURFACE} w-[18rem]`}>
        <VoiceWave size={96} />
        <div className="flex flex-col items-center gap-1">
          <span className="text-sm font-medium text-foreground">Stop the run on EXP-1242?</span>
          <span className="text-xs text-muted-foreground">Say yes, or tap</span>
        </div>
        <div className="flex items-center gap-2">
          <Pill size="md" mode="action" primary>
            Stop
          </Pill>
          <Pill size="md" mode="action">
            Cancel
          </Pill>
        </div>
      </div>
    </div>
  ),
}
