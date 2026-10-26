/* A real `@exp/ui` island that plays a film (NEW-15): prerendered at its
   resting frame by scripts/ui-demos.tsx, then, once scrolled into view,
   replayed frame by frame in its own shadow root and looped. Under
   prefers-reduced-motion it stays on the resting frame. The /ui/ home's
   HomeDemo does the same for its phone. */
import { useEffect, useRef, type ComponentType } from "react"
import { UiDemo } from "./UiDemo"

export interface Film {
  Frame: ComponentType<{ step: number }>
  /** The last frame; the film loops after holding it. */
  last: number
  frameMs: (step: number) => number
}

export function IslandFilm({ id, load, holdMs }: { id: string; load: () => Promise<Film>; holdMs: number }) {
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = root.current
    if (!el || window.matchMedia(`(prefers-reduced-motion: reduce)`).matches) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let unmount: (() => void) | undefined
    const start = async () => {
      const [{ createRoot }, { Frame, last, frameMs }] = await Promise.all([import(`react-dom/client`), load()])
      /* The island's own wrapper inside its shadow root (the compiled
         @exp/ui sheet stays beside it). */
      const shadow = el.querySelector(`[data-ui-island]`)?.shadowRoot
      const screen = Array.from(shadow?.children ?? []).find((child): child is HTMLElement => child instanceof HTMLDivElement)
      if (stopped || !screen) return
      screen.replaceChildren()
      const live = createRoot(screen)
      unmount = () => live.unmount()
      const show = (step: number) => {
        if (stopped) return
        live.render(<Frame step={step} />)
        timer = setTimeout(() => show(step >= last ? 0 : step + 1), step >= last ? holdMs : frameMs(step))
      }
      show(0)
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return
        io.disconnect()
        void start()
      },
      { threshold: 0.35 }
    )
    io.observe(el)
    return () => {
      stopped = true
      io.disconnect()
      clearTimeout(timer)
      unmount?.()
    }
  }, [load, holdMs])

  return (
    <div ref={root} inert>
      <UiDemo id={id} />
    </div>
  )
}
