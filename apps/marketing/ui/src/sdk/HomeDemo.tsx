/* The home page's live demo: the Exponential app's phone chat in an iPhone
   frame, answering `How did last night's release go?` with an Exponential
   UI surface generated on the fly (PhoneChat.tsx: the prompt typed, the
   answer's words, then the surface node by node), beside the same surface
   photographed on iOS, Android and desktop. The screen is a real `@exp/ui`
   island (src/ui-demos): prerendered at its last frame, then replayed in
   its own shadow root once scrolled into view, looping; under
   prefers-reduced-motion it stays on the last frame. */
import { useEffect, useRef } from "react"
import { UiDemo } from "../../../src/components/UiDemo"
import { PLATFORMS } from "./platforms"
import { ShotFigure } from "./Shot"

const NATIVE = PLATFORMS.filter((p) => p.id !== `web`)
/** How long the finished answer stays before the film replays. */
const HOLD_MS = 6000

export function HomeDemo() {
  const root = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = root.current
    if (!el || window.matchMedia(`(prefers-reduced-motion: reduce)`).matches) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let unmount: (() => void) | undefined
    const start = async () => {
      const [{ createRoot }, { PhoneChat, LAST_STEP, frameMs }] = await Promise.all([import(`react-dom/client`), import(`./PhoneChat`)])
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
        live.render(<PhoneChat step={step} />)
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
    <div className="sdk-home-demo" ref={root}>
      <div className="sdk-phone" aria-label="An Exponential UI surface generated in the Exponential chat">
        <span className="sdk-phone-island" aria-hidden />
        <div className="sdk-phone-status" aria-hidden>
          <span>9:41</span>
        </div>
        <div className="sdk-phone-screen">
          <UiDemo id="ui-phone-chat" />
        </div>
      </div>
      <div className="sdk-home-shots" aria-label="The same surface on the native renderers">
        {NATIVE.map((p) => (
          <ShotFigure key={p.id} viewId="exponential-ui-demo" platform={p} title="The release surface" compact />
        ))}
      </div>
    </div>
  )
}
