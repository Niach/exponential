/* The home page's live demo: a prompt, and the answer an agent would stream
   for it — a PRE-RECORDED A2UI replay (createSurface, then updateComponents
   in growing chunks ~120 ms apart) painted by the React renderer as it
   lands, beside the same surface photographed on iOS, Android and desktop.
   The surface is fixtures/demo-surface.json (= specimen exponential-ui-demo).
   Plays once when scrolled into view (never under prefers-reduced-motion)
   and again on Send. */
import { Check } from "lucide-react"
import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react"
import type { IconMap } from "@exponential-at/ui-react"
import demo from "@exponential-at/ui/fixtures/demo-surface.json"
import { flatten, replayChunks, type Message, type Nested } from "./a2ui"
import { PLATFORMS } from "./platforms"
import type { LiveSurfaceProps } from "./runtime"
import { ShotFigure } from "./Shot"

const PROMPT = `How did last night's release go?`
const SURFACE_ID = `release`
const STEP_MS = 120
const COMPONENTS = flatten(demo as unknown as Nested)
const REPLAY = replayChunks(SURFACE_ID, COMPONENTS, 1)
/** The demo's only registry icon; the renderer's own chrome needs none. */
const ICONS: IconMap = { check: Check }
const NATIVE = PLATFORMS.filter((p) => p.id !== `web`)

type Live = (props: LiveSurfaceProps) => ReactNode
let live: Promise<Live> | null = null
const loadLive = () => (live ??= import("./runtime").then((m) => m.LiveSurface))

export function HomeDemo() {
  const [prompt, setPrompt] = useState(PROMPT)
  const [shown, setShown] = useState(0)
  const [playing, setPlaying] = useState(false)
  const [Surface, setSurface] = useState<Live | null>(null)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const played = useRef(false)

  const play = useCallback(async () => {
    played.current = true
    const comp = await loadLive()
    setSurface(() => comp)
    if (timer.current) clearInterval(timer.current)
    if (window.matchMedia(`(prefers-reduced-motion: reduce)`).matches) {
      setShown(REPLAY.length)
      setPlaying(false)
      return
    }
    let n = 1
    setShown(1)
    setPlaying(true)
    timer.current = setInterval(() => {
      n += 1
      setShown(n)
      if (n >= REPLAY.length) {
        clearInterval(timer.current!)
        timer.current = null
        setPlaying(false)
      }
    }, STEP_MS)
  }, [])

  useEffect(() => {
    const el = root.current
    if (!el) return
    // Warm the renderer early; play once in view (motion allowed).
    const reduce = window.matchMedia(`(prefers-reduced-motion: reduce)`).matches
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return
        void loadLive()
        if (!reduce && !played.current) void play()
        io.disconnect()
      },
      { threshold: 0.35 }
    )
    io.observe(el)
    return () => {
      io.disconnect()
      if (timer.current) clearInterval(timer.current)
    }
  }, [play])

  const onSubmit = (e: FormEvent) => {
    e.preventDefault()
    void play()
  }

  const messages: Message[] = REPLAY.slice(0, shown)
  const done = shown >= REPLAY.length

  return (
    <div className="sdk-home-demo" ref={root}>
      <div className="sdk-home-chat">
        <form className="sdk-home-prompt" onSubmit={onSubmit}>
          <input className="sdk-input" value={prompt} onChange={(e) => setPrompt(e.target.value)} aria-label="Ask the agent" spellCheck={false} />
          <button type="submit" className="btn btn-primary btn-sm" disabled={playing}>
            Send
          </button>
        </form>
        <div className="sdk-home-stage" aria-live="polite">
          {Surface && shown > 0 ? (
            <Surface surfaceId={SURFACE_ID} messages={messages} theme="exponential" mode="dark" icons={ICONS} />
          ) : (
            <div className="sdk-home-idle" />
          )}
        </div>
        <div className="sdk-home-status">
          <span className={`sdk-dot${playing ? ` is-live` : ``}`} aria-hidden />
          {shown === 0 ? `Replay` : done ? `Done` : `Streaming`}
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
