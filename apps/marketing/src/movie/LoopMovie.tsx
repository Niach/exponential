/* ─── LoopMovie — the closed-loop product movie embed ───

   CRITICAL: this file is statically imported by HomePage, which is
   renderToString'd under Bun by scripts/prerender.tsx — it must import
   NOTHING from remotion. The ONE allowed composition import is the
   remotion-free data module ./closedloop/chapters (chapter ids/labels shared
   with the composition so the rail can't drift); the Remotion Player and the
   composition itself live in the lazy chunk ./LoopMoviePlayer, dynamically
   imported when the section scrolls near (IntersectionObserver, 300px
   early).

   Pre-mount the wrapper reserves the full 16:9 box (CLS-safe) and shows a
   rendered poster frame; the flow stepper ABOVE the frame is real HTML
   (SEO / no-JS; EXP-337 — orca-style slim segmented bar at every width).
   Under prefers-reduced-motion nothing autoplays — a Play button mounts and
   starts the movie on click. */
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react"
import { Play } from "lucide-react"
import { FLOW_INFO } from "./closedloop/chapters"

/* The imperative surface LoopMoviePlayer hands back once mounted. Defined
   here (not in the player chunk) so this file stays remotion-free. */
export type LoopMovieController = {
  seekToChapter: (index: number) => void
  play: () => void
  pause: () => void
}

/* The composition's own flow metadata (remotion-free module) — same list
   CHAPTERS is built from, so the stepper and the film can't drift.
   Labels render statically for SEO; the frame numbers stay inside
   the lazy chunk (the player seeks by index). */
const FLOW_META = FLOW_INFO

const POSTER_ALT = `The Exponential desktop IDE beside the mobile app: the team's live issue board and a bug report open as an issue on both screens. The whole team collaborates here in realtime, and coding runs start from any device.`

const LoopMoviePlayer = lazy(() => import(`./LoopMoviePlayer`))

export function LoopMovie() {
  const wrapRef = useRef<HTMLDivElement>(null)

  const [load, setLoad] = useState(false)
  const [ready, setReady] = useState(false)
  const [started, setStarted] = useState(false)
  const [reduced, setReduced] = useState(false)
  const [active, setActive] = useState(0)
  /* Playback progress through the active chapter, integer percent — drives
     the stepping fill in the rail (percent-quantized in LoopMoviePlayer). */
  const [progress, setProgress] = useState(0)

  const controllerRef = useRef<LoopMovieController | null>(null)
  const reducedRef = useRef(false)
  const startedRef = useRef(false)
  const pendingSeekRef = useRef<number | null>(null)
  const pendingPlayRef = useRef(false)

  /* Reduced-motion probe runs post-hydration (no SSR mismatch). Declared
     first so reducedRef is settled before the observers below register. */
  useEffect(() => {
    const mq = window.matchMedia(`(prefers-reduced-motion: reduce)`)
    const apply = () => {
      reducedRef.current = mq.matches
      setReduced(mq.matches)
    }
    apply()
    mq.addEventListener(`change`, apply)
    return () => mq.removeEventListener(`change`, apply)
  }, [])

  /* Approach observer — kick off the lazy chunk 300px before the section
     scrolls in. Reduced-motion users never preload; they get the poster and
     an explicit Play button instead. */
  useEffect(() => {
    const el = wrapRef.current
    if (!el || typeof IntersectionObserver === `undefined`) return
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting) && !reducedRef.current) {
          setLoad(true)
          observer.disconnect()
        }
      },
      { rootMargin: `300px` }
    )
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  /* Visibility observer — pause when the movie scrolls out of view, resume
     when it comes back (only resumes what autoplay or the user started). */
  useEffect(() => {
    const el = wrapRef.current
    if (!el || typeof IntersectionObserver === `undefined`) return
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.some((e) => e.isIntersecting)
        const controller = controllerRef.current
        if (!controller) return
        if (!visible) {
          controller.pause()
        } else if (startedRef.current || !reducedRef.current) {
          controller.play()
        }
      },
      { threshold: 0.1 }
    )
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  const handleController = useCallback((controller: LoopMovieController) => {
    controllerRef.current = controller
    if (pendingSeekRef.current !== null) {
      controller.seekToChapter(pendingSeekRef.current)
      pendingSeekRef.current = null
    }
    if (pendingPlayRef.current) {
      pendingPlayRef.current = false
      controller.play()
    }
  }, [])

  const handleReady = useCallback(() => setReady(true), [])

  const handleChapterProgress = useCallback(
    (index: number, percent: number) => {
      setActive(index)
      setProgress(percent)
    },
    []
  )

  const handlePlayingChange = useCallback((playing: boolean) => {
    if (playing) {
      startedRef.current = true
      setStarted(true)
    }
  }, [])

  const handlePlayClick = () => {
    startedRef.current = true
    setStarted(true)
    const controller = controllerRef.current
    if (controller) {
      controller.play()
    } else {
      pendingPlayRef.current = true
      setLoad(true)
    }
  }

  const handleChapter = (index: number) => {
    setActive(index)
    setProgress(0)
    const controller = controllerRef.current
    if (controller) {
      controller.seekToChapter(index)
      controller.play()
      startedRef.current = true
      setStarted(true)
    } else {
      pendingSeekRef.current = index
      pendingPlayRef.current = true
      setLoad(true)
    }
  }

  return (
    <div className={`movie`} ref={wrapRef}>
      {/* The flow stepper sits ABOVE the frame (EXP-337, orca-style): five
          slim glass segments with a per-flow progress fill. On desktop each
          segment shows its label above the pill; on phones the labels go
          visually hidden and .movie-rail-now narrates instead. */}
      <div className={`movie-stepper`}>
        {FLOW_META.map((flow, index) => (
          <button
            key={flow.id}
            type={`button`}
            className={`movie-step${index === active ? ` is-active` : ``}`}
            style={
              index === active
                ? ({ "--chip-progress": `${progress}%` } as CSSProperties)
                : undefined
            }
            onClick={() => handleChapter(index)}
            aria-current={index === active}
          >
            <span className={`movie-step-label`}>{flow.label}</span>
            <span className={`movie-step-track`} aria-hidden />
          </button>
        ))}
      </div>
      {/* Narrates the active flow on phones (where the step labels go
          visually hidden); the steps keep their text, so this stays
          aria-hidden. Desktop hides it entirely — the steps show labels. */}
      <p className={`movie-rail-now`} aria-hidden>
        <span className={`movie-rail-now-label`}>
          {FLOW_META[active].label}
        </span>
      </p>
      <div className={`movie-stage`}>
        <img
          className={`movie-poster${ready ? ` is-hidden` : ``}`}
          src={`/posters/loop-poster.webp`}
          alt={POSTER_ALT}
          width={1920}
          height={1080}
          loading={`lazy`}
          decoding={`async`}
        />
        {load && (
          <div className={`movie-layer`} aria-hidden>
            <Suspense fallback={null}>
              <LoopMoviePlayer
                autoPlay={!reduced}
                onController={handleController}
                onChapterProgress={handleChapterProgress}
                onReady={handleReady}
                onPlayingChange={handlePlayingChange}
              />
            </Suspense>
          </div>
        )}
        {reduced && !started && (
          <button
            type={`button`}
            className={`movie-playbtn`}
            onClick={handlePlayClick}
          >
            <Play size={15} strokeWidth={2} />
            Play the demo
          </button>
        )}
      </div>
    </div>
  )
}
