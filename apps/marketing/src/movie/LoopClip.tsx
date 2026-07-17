/* ─── LoopClip — a small looping chapter excerpt of the product film ───

   Same SSR contract as LoopMovie: this file is statically imported by home
   sections and renderToString'd under Bun by scripts/prerender.tsx, so it
   must import NOTHING from remotion — the ONE allowed @video import is the
   remotion-free chapters module. The Player (and the chapter-id → frame
   lookup) lives in the lazy chunk ./LoopClipPlayer, mounted when the clip
   scrolls near.

   Decorative only: aria-hidden, non-interactive, pauses off-screen, and
   never loads under prefers-reduced-motion (the placeholder stage stays). */
import { lazy, Suspense, useEffect, useRef, useState } from "react"
import { CHAPTER_INFO } from "@video/closedloop/chapters"

const LoopClipPlayer = lazy(() => import(`./LoopClipPlayer`))

export function LoopClip({
  chapter,
  className,
}: {
  chapter: string
  className?: string
}) {
  const wrapRef = useRef<HTMLDivElement>(null)
  const [load, setLoad] = useState(false)
  const [playing, setPlaying] = useState(false)

  const known = CHAPTER_INFO.some((c) => c.id === chapter)

  useEffect(() => {
    const el = wrapRef.current
    if (!known || !el || typeof IntersectionObserver === `undefined`) return
    if (window.matchMedia(`(prefers-reduced-motion: reduce)`).matches) return

    const approach = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setLoad(true)
          approach.disconnect()
        }
      },
      { rootMargin: `300px` },
    )
    approach.observe(el)

    const visibility = new IntersectionObserver(
      (entries) => setPlaying(entries.some((e) => e.isIntersecting)),
      { threshold: 0.1 },
    )
    visibility.observe(el)

    return () => {
      approach.disconnect()
      visibility.disconnect()
    }
  }, [known])

  return (
    <div
      className={`clip${className ? ` ${className}` : ``}`}
      ref={wrapRef}
      aria-hidden
    >
      {load && known ? (
        <Suspense fallback={null}>
          <LoopClipPlayer chapter={chapter} playing={playing} />
        </Suspense>
      ) : null}
    </div>
  )
}
