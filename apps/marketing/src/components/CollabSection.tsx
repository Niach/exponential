/* ─── Collaboration — merged Teamwork + Helpdesk section (EXP-176) ───
   The visitor side is a scripted, looping widget scene (real-UI widget
   recreation, Get-help path — support requests are what open Support-inbox
   threads); the team side is the FULL web app recreation (EXP-217: the
   complete WebDemo — sidebar + 3-pane Support inbox — not a partial
   composite), shown statically so the page never shifts as the loop runs.
   Stage is decorative (aria-hidden + inert); reduced motion renders the
   finished widget state statically. */
import { motion } from "motion/react"
import { useEffect, useState } from "react"
import { EASE_EXPO, eyebrowDraw, sectionReveal } from "../lib/animations"
import { useScenePlayer } from "../lib/use-scene-player"
import { SUPPORT_THREADS } from "../webui/data"
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
  home: 1,
  form: 2,
  sent: 3,
  handoff: 4,
  hold: 5,
} as const
const BEATS = [1200, 1100, 2600, 1600, 900, 3200]

/* The typed message is Mara's fixture thread opener (webui/data.ts) — the
   same conversation the full inbox demo below has selected. */
const MARA = SUPPORT_THREADS[0]

/* Types the widget message in while `active` (client-only — the scene
   never types during SSR, whose resting beat shows only the FAB). */
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
  const typed = useTypedText(MARA.messages[0].body, typing)
  const typedDone = typed.length >= MARA.messages[0].body.length

  const widgetView: WidgetDemoView = at(B.sent)
    ? `success`
    : beat === B.form
      ? `support`
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
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Collaboration
          </motion.span>
          <h2 className={`section-title`}>Work as a team in real time.</h2>
          <p className={`section-sub`}>
            A support request from the feedback widget lands in your
            team&rsquo;s shared Support inbox as an email conversation. Reply
            from there, drop internal notes the reporter never sees, and turn
            tickets into board issues your agents can pick up &mdash; everything
            updates live for the whole team.
          </p>
          <span className={`co-pro`}>
            <span className={`co-pro-badge`}>Pro</span> Helpdesk is included in
            the Pro plan.
          </span>
        </motion.div>

        <div className={stageClass} ref={ref} aria-hidden inert>
          {/* ── The visitor's page: real widget, Get-help path ── */}
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
                    message={reduced ? `` : typed}
                    emailFilled={reduced || typedDone}
                    caret={typing && !typedDone}
                  />
                </motion.div>
              )}
            </div>
            <span className={`co-stage-caption`}>
              Your users, in the feedback widget
            </span>
          </div>

          {/* ── Connector: the request travels into the inbox ── */}
          <div className={`co-conn`}>
            <span className={`co-conn-line`} />
            <span className={`co-conn-label`}>lands in your Support inbox</span>
            <span className={`co-conn-line`} />
            <span className={`co-conn-dot`} />
          </div>

          {/* ── The team's Support inbox — the FULL web app recreation,
                 always mounted so the looping scene never shifts layout ── */}
          <div className={`co-webuicol`}>
            <WebDemo view={`support`} interactive={false} />
            <span className={`co-stage-caption`}>
              Your team, in the shared Support inbox
            </span>
          </div>
        </div>

        {/* ── Cross-platform line (real content, outside the stage) ── */}
        <motion.div className={`co-platforms`} {...sectionReveal}>
          <p className={`co-platforms-note`}>
            Collaborate across every platform &mdash; native apps for macOS,
            Windows, Linux, iOS and Android, plus the web.
          </p>
          <DownloadIconRow />
        </motion.div>
      </div>
    </section>
  )
}
