/* ─── useScenePlayer — shared driver for the scripted home-page scenes ───
   A finite list of beat durations looped forever: beat 0 is the SSR-rendered
   resting state (no browser APIs run during render), an IntersectionObserver
   pauses the clock while the stage is scrolled out of view (re-entering
   restarts the CURRENT beat's full duration, same contract as LoopMovie),
   and prefers-reduced-motion freezes the player entirely so callers can
   render a static composite instead. */
import { useEffect, useRef, useState } from "react"

export function useScenePlayer(durations: number[]): {
  ref: React.RefObject<HTMLDivElement | null>
  beat: number
  reduced: boolean
} {
  const ref = useRef<HTMLDivElement>(null)
  const [beat, setBeat] = useState(0)
  const [reduced, setReduced] = useState(false)
  const [visible, setVisible] = useState(false)

  /* Reduced-motion probe runs post-hydration (no SSR mismatch). */
  useEffect(() => {
    if (typeof window.matchMedia !== `function`) return
    const mq = window.matchMedia(`(prefers-reduced-motion: reduce)`)
    const apply = () => setReduced(mq.matches)
    apply()
    mq.addEventListener(`change`, apply)
    return () => mq.removeEventListener(`change`, apply)
  }, [])

  /* Visibility — the beat clock only ticks while the stage is on screen. */
  useEffect(() => {
    const el = ref.current
    if (!el || typeof IntersectionObserver === `undefined`) return
    const observer = new IntersectionObserver(
      (entries) => setVisible(entries.some((e) => e.isIntersecting)),
      { threshold: 0.15 }
    )
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  /* The beat clock: one timeout per beat, cleared whenever the stage
     leaves the viewport (re-entry restarts the current beat). */
  useEffect(() => {
    if (reduced || !visible) return
    const id = window.setTimeout(
      () => setBeat((b) => (b + 1) % durations.length),
      durations[beat]
    )
    return () => window.clearTimeout(id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [beat, reduced, visible])

  return { ref, beat, reduced }
}
