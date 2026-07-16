/* ─── LoopMovie — the closed-loop product movie embed ───

   CRITICAL: this file is statically imported by HomePage, which is
   renderToString'd under Bun by scripts/prerender.tsx — it must import
   NOTHING from remotion or @video. The Remotion Player (and the composition
   itself) live in the lazy chunk ./LoopMoviePlayer, dynamically imported
   when the section scrolls near (IntersectionObserver, 300px early).

   Pre-mount the wrapper reserves the full 16:9 box (CLS-safe) and shows a
   rendered poster frame; the chapter rail below is real HTML (SEO / no-JS).
   Under prefers-reduced-motion nothing autoplays — a Play button mounts and
   starts the movie on click. */
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react"
import { Play } from "lucide-react"

/* The imperative surface LoopMoviePlayer hands back once mounted. Defined
   here (not in the player chunk) so this file stays remotion-free. */
export type LoopMovieController = {
  seekToChapter: (index: number) => void
  play: () => void
  pause: () => void
}

/* Mirrors CHAPTERS in apps/video/src/closedloop/timeline.ts — same five
   entries, same order. Labels/phrases render statically for SEO; the frame
   numbers stay inside the lazy chunk (the player seeks by index). */
const CHAPTER_META = [
  { id: `feedback`, label: `Feedback`, phrase: `a user reports a bug` },
  { id: `issue`, label: `Issue`, phrase: `lands on the board` },
  { id: `code`, label: `Code`, phrase: `Claude writes the fix` },
  { id: `merge`, label: `Merge`, phrase: `review, merge` },
  { id: `shipped`, label: `Shipped`, phrase: `the reporter hears back` },
] as const

const POSTER_ALT = `The Exponential desktop IDE mid-loop: a bug reported from the feedback widget has become an issue, a Claude coding session is writing the fix, and a pull request is on its way to merge.`

const LoopMoviePlayer = lazy(() => import(`./LoopMoviePlayer`))

export function LoopMovie() {
  const wrapRef = useRef<HTMLDivElement>(null)

  const [load, setLoad] = useState(false)
  const [ready, setReady] = useState(false)
  const [started, setStarted] = useState(false)
  const [reduced, setReduced] = useState(false)
  const [active, setActive] = useState(0)

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
      { rootMargin: `300px` },
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
      { threshold: 0.1 },
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
                onChapterChange={setActive}
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
      <div className={`movie-rail`}>
        {CHAPTER_META.map((chapter, index) => (
          <button
            key={chapter.id}
            type={`button`}
            className={`movie-chip${index === active ? ` is-active` : ``}`}
            onClick={() => handleChapter(index)}
            aria-current={index === active}
          >
            <span className={`movie-chip-label`}>{chapter.label}</span>
            <span className={`movie-chip-phrase`}>{chapter.phrase}</span>
          </button>
        ))}
      </div>
    </div>
  )
}
