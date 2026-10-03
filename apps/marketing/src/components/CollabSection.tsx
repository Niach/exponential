/* ─── Collaboration — feedback widget → board (EXP-602, SLOP-4) ───
   The visitor side is a scripted, looping widget scene (real-UI widget
   recreation, the ONE form); the team side is the web app recreation on the
   BOARD view, side by side — submitting the report files a new issue row
   into the board. Stages are decorative (aria-hidden + inert); reduced
   motion renders the finished composite statically (widget success + board
   including the filed row). */
import { motion } from "motion/react"
import { useEffect, useState } from "react"
import { EASE_EXPO, sectionReveal } from "../lib/animations"
import { useScenePlayer } from "../lib/use-scene-player"
import { WIDGET_FILED_ISSUE } from "../webui/data"
import { WebDemo } from "../webui/WebDemo"
import { DownloadIconRow } from "./DownloadSection"
import {
  MegaphoneIcon,
  WidgetPanelDemo,
  type WidgetDemoView,
} from "./WidgetPanelDemo"

/* Beat script (~10.5s loop). Beat 0 is the SSR resting state. */
const B = {
  fab: 0,
  form: 1,
  sent: 2,
  handoff: 3,
  filed: 4,
} as const
const BEATS = [1400, 3400, 1300, 800, 3600]

/* The message's FIRST LINE becomes the issue title (the server titles a
   widget report off it), so it IS the injected issue's title (webui/data.ts). */
const REPORT_MESSAGE = `${WIDGET_FILED_ISSUE.title}\nThe upload spinner runs forever when I attach a screenshot. Safari 17 on macOS.`

/* Types the widget message in while `active` (client-only — the scene never
   types during SSR, whose resting beat shows only the FAB). */
function useTypedText(text: string, active: boolean): string {
  const [count, setCount] = useState(0)
  useEffect(() => {
    if (!active) {
      setCount(0)
      return
    }
    const id = window.setInterval(() => {
      setCount((current) => {
        if (current >= text.length) {
          window.clearInterval(id)
          return current
        }
        return current + 3
      })
    }, 40)
    return () => window.clearInterval(id)
  }, [active, text])
  return active ? text.slice(0, count) : ``
}

export function CollabSection() {
  const { ref, beat, reduced } = useScenePlayer(BEATS)
  const at = (from: number) => reduced || beat >= from

  const typing = !reduced && beat === B.form
  const typed = useTypedText(REPORT_MESSAGE, typing)
  const typedDone = typed.length >= REPORT_MESSAGE.length

  const widgetView: WidgetDemoView = at(B.sent) ? `success` : `form`

  /* Entrance props — collapse to nothing under reduced motion. */
  const pop = reduced
    ? {}
    : ({
        initial: { opacity: 0, y: 8 },
        animate: { opacity: 1, y: 0 },
        transition: { duration: 0.4, ease: EASE_EXPO },
      } as const)

  const stageClass = [
    `co-stage`,
    at(B.handoff) ? `is-handoff` : ``,
    reduced ? `is-static` : ``,
  ]
    .filter(Boolean)
    .join(` `)

  return (
    <section id={`collaboration`} className={`home-collab`}>
      <div className={`shell`}>
        <motion.div className={`co-copy`} {...sectionReveal}>
          <h2 className={`section-title`}>
            Embed our widget, get customer feedback on your board
          </h2>
          <p className={`section-sub`}>
            Visitors report bugs and ideas without leaving your site,
            screenshot included. Every report lands as an issue on your board,
            ready to triage with the team, and you answer the reporter right
            from the issue.
          </p>
        </motion.div>

        <div className={stageClass} ref={ref} aria-hidden inert>
          {/* ── The visitor's page: real widget, the one form ── */}
          <div className={`co-widgetcol`}>
            <div className={`co-page`}>
              <span className={`co-page-bar is-w60`} />
              <span className={`co-page-bar is-w80`} />
              <span className={`co-page-bar is-w40`} />
              {!at(B.form) && (
                <span className={`co-fab`}>
                  <MegaphoneIcon size={16} />
                </span>
              )}
              {at(B.form) && (
                <motion.div className={`co-panel`} {...pop}>
                  <WidgetPanelDemo
                    view={widgetView}
                    message={reduced ? REPORT_MESSAGE : typed}
                    emailFilled={reduced || typedDone}
                    caret={typing && !typedDone}
                  />
                </motion.div>
              )}
            </div>
          </div>

          {/* ── Connector: the report travels onto the board ── */}
          <div className={`co-conn`}>
            <span className={`co-conn-label`}>lands on your board</span>
            <span className={`co-conn-track`}>
              <span className={`co-conn-dot`} />
            </span>
          </div>

          {/* ── The team's board — the web app recreation, always mounted
                 so the looping scene never shifts layout ── */}
          <div className={`co-webuicol`}>
            <WebDemo
              view={`board`}
              interactive={false}
              injectedIssue={at(B.filed) ? WIDGET_FILED_ISSUE : null}
            />
          </div>
        </div>

        {/* ── Cross-platform line (real content, outside the stage) ── */}
        <motion.div className={`co-platforms`} {...sectionReveal}>
          <p className={`co-platforms-note`}>
            Collaborate across every platform
          </p>
          <DownloadIconRow />
        </motion.div>
      </div>
    </section>
  )
}
