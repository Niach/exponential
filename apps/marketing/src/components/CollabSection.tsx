/* ─── Collaboration — feedback widget → board, plus helpdesk (EXP-602) ───
   The visitor side is a scripted, looping widget scene (real-UI widget
   recreation, GIVE-FEEDBACK path); the team side is the web app recreation
   on the BOARD view, side by side — submitting the report files a new issue
   row into the board. Below, a small helpdesk subsection shows the Support
   conversation view (chat + details rail only). Stages are decorative
   (aria-hidden + inert); reduced motion renders the finished composite
   statically (widget success + board including the filed row). */
import { motion } from "motion/react"
import { useEffect, useState } from "react"
import { EASE_EXPO, sectionReveal } from "../lib/animations"
import { useScenePlayer } from "../lib/use-scene-player"
import { WIDGET_FILED_ISSUE } from "../webui/data"
import { WebDemo } from "../webui/WebDemo"
import { HelpdeskChatDemo } from "../webui/HelpdeskChatDemo"
import { DownloadIconRow } from "./DownloadSection"
import {
  MegaphoneIcon,
  WidgetPanelDemo,
  type WidgetDemoView,
} from "./WidgetPanelDemo"

/* Beat script (~11.6s loop). Beat 0 is the SSR resting state. */
const B = {
  fab: 0,
  home: 1,
  form: 2,
  sent: 3,
  handoff: 4,
  filed: 5,
} as const
const BEATS = [1400, 1100, 3400, 1300, 800, 3600]

/* The typed report title IS the injected issue's title (webui/data.ts) —
   the same bug Mara's helpdesk fixture thread below is about. */
const REPORT_TITLE = WIDGET_FILED_ISSUE.title
const REPORT_DETAILS = `The upload spinner runs forever when I attach a screenshot. Safari 17 on macOS.`

/* Types the widget title in while `active` (client-only — the scene never
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
  const typed = useTypedText(REPORT_TITLE, typing)
  const typedDone = typed.length >= REPORT_TITLE.length

  const widgetView: WidgetDemoView = at(B.sent)
    ? `success`
    : beat === B.form
      ? `feedback`
      : `home`

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
            ready to triage with the team.
          </p>
        </motion.div>

        <div className={stageClass} ref={ref} aria-hidden inert>
          {/* ── The visitor's page: real widget, Give-feedback path ── */}
          <div className={`co-widgetcol`}>
            <div className={`co-page`}>
              <span className={`co-page-bar is-w60`} />
              <span className={`co-page-bar is-w80`} />
              <span className={`co-page-bar is-w40`} />
              {!at(B.home) && (
                <span className={`co-fab`}>
                  <MegaphoneIcon size={16} />
                </span>
              )}
              {at(B.home) && (
                <motion.div className={`co-panel`} {...pop}>
                  <WidgetPanelDemo
                    view={widgetView}
                    title={reduced ? `` : typed}
                    details={reduced || typedDone ? REPORT_DETAILS : ``}
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

        {/* ── Helpdesk subsection: the Support conversation view ── */}
        <div className={`co-help`}>
          <motion.div className={`co-help-copy`} {...sectionReveal}>
            <h3 className={`co-help-title`}>
              Stay in touch with your customers with our helpdesk
            </h3>
            <p className={`co-help-sub`}>
              Support requests from the widget open email conversations in a
              shared inbox, and any ticket escalates to an issue in one click.
            </p>
            <span className={`co-pro`}>
              <span className={`co-pro-badge`}>Team</span> Helpdesk is included
              in the Team plan.
            </span>
          </motion.div>
          <div className={`co-help-demo`} aria-hidden inert>
            <HelpdeskChatDemo />
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
