/* ─── HelpdeskChatDemo — chat + details rail only (EXP-602) ───
   The home page's helpdesk subsection shows just the conversation view of
   the Support inbox: chathead, bubbles, composer and the details rail — no
   thread list, no widget flow. Static (decorative, rendered inert) and
   context-free: it composes the presentational pieces exported by
   SupportInbox (Bubble, SupportRail) inside the house fixed-canvas +
   useDemoScale pattern. Phones drop the rail and shrink the canvas so the
   chat stays readable instead of scaling the full 880px down. */
import { useEffect, useState } from "react"
import { useDemoScale } from "../lib/use-demo-scale"
import {
  Bubble,
  SupportChatHead,
  SupportComposer,
  SupportRail,
} from "./SupportInbox"
import { SUPPORT_THREADS } from "./data"

const BASE_W = 880
const DEMO_H = 700
/* Chat-only phone canvas — narrower base width means a bigger scale, and
   the taller box absorbs the extra bubble wrapping. Both heights grew with
   EXP-471: the recreation now draws at the app's real 18.5px rem grid, so
   every row is ~1.25× the old hand-tuned 13px scale. */
const COMPACT_W = 440
const COMPACT_H = 760

/* Mara's thread: has widget context and no linked issue, so the rail shows
   Reporter · Context · the Escalate board picker, per the real app. */
const THREAD = SUPPORT_THREADS[0]

/* Phone probe runs post-hydration (no SSR mismatch — the prerender ships
   the desktop variant and the client settles after mount). */
function useCompact(): boolean {
  const [compact, setCompact] = useState(false)
  useEffect(() => {
    if (typeof window.matchMedia !== `function`) return
    const mq = window.matchMedia(`(max-width: 700px)`)
    const apply = () => setCompact(mq.matches)
    apply()
    mq.addEventListener(`change`, apply)
    return () => mq.removeEventListener(`change`, apply)
  }, [])
  return compact
}

export function HelpdeskChatDemo() {
  const compact = useCompact()
  const baseW = compact ? COMPACT_W : BASE_W
  const demoH = compact ? COMPACT_H : DEMO_H
  const { ref, scale } = useDemoScale(baseW)

  return (
    <div
      ref={ref}
      className={`web-scale`}
      style={{ height: Math.round(demoH * scale) }}
    >
      <div
        className={`web-root is-static`}
        style={{
          height: demoH,
          ...(scale < 1
            ? { width: baseW, transform: `scale(${scale})` }
            : undefined),
        }}
      >
        <div className="web-sup co-helpsup">
          <div className="web-sup-chat">
            <SupportChatHead thread={THREAD} />
            <div className="web-sup-msgs">
              {THREAD.messages.map((m, i) => (
                <Bubble key={i} message={m} reporter={THREAD.reporterName} />
              ))}
            </div>
            <SupportComposer
              reporterName={THREAD.reporterName}
              mode={`reply`}
              draft={``}
              interactive={false}
            />
          </div>
          {!compact && (
            <SupportRail thread={THREAD} issue={null} interactive={false} />
          )}
        </div>
      </div>
    </div>
  )
}
