/* The push-to-talk overlay on the home page and /talk/ (NEW-15): ONE capsule
   stepping through Listening → Heard → Speaking (src/ui-demos/VoiceOverlay.tsx),
   played by IslandFilm. */
import { IslandFilm, type Film } from "./IslandFilm"

const loadOverlay = async (): Promise<Film> => {
  const { VoiceOverlay, LAST_STEP, frameMs } = await import(`../ui-demos/VoiceOverlay`)
  return { Frame: VoiceOverlay, last: LAST_STEP, frameMs }
}

export function TalkOverlay() {
  return <IslandFilm id={`voice-overlay`} load={loadOverlay} holdMs={2400} />
}
