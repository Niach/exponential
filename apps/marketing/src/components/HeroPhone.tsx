/* The home hero's phone (NEW-15): an agent's chat in an iPhone frame
   (src/ui-demos/AgentChat.tsx: the prompt typed, the answer's words, its
   Exponential UI surface node by node, the tap and the session it starts).
   The screen is a real `@exp/ui` island, prerendered at its last frame, then
   replayed in its own shadow root once in view, looping; under
   prefers-reduced-motion it stays on the last frame. The /ui/ home's
   HomeDemo does the same with its own film. */
import { useEffect, useRef } from "react"
import { UiDemo } from "./UiDemo"

/** How long the finished chat stays before the film replays. */
const HOLD_MS = 6000

export function HeroPhone() {
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = root.current
    if (!el || window.matchMedia(`(prefers-reduced-motion: reduce)`).matches) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let unmount: (() => void) | undefined
    const start = async () => {
      const [{ createRoot }, { AgentChat, LAST_STEP, frameMs }] = await Promise.all([import(`react-dom/client`), import(`../ui-demos/AgentChat`)])
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
        live.render(<AgentChat step={step} />)
        timer = setTimeout(() => show(step >= LAST_STEP ? 0 : step + 1), step >= LAST_STEP ? HOLD_MS : frameMs(step))
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
  }, [])

  return (
    <div className={`home-phone`} ref={root} aria-label={`An agent's chat on iPhone`}>
      <span className={`home-phone-island`} aria-hidden />
      <div className={`home-phone-status`} aria-hidden>
        <span>9:41</span>
      </div>
      <div className={`home-phone-screen`}>
        <UiDemo id={`home-agent-chat`} />
      </div>
    </div>
  )
}
