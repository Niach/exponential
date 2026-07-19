/* ─── Collaboration — merged Teamwork + Helpdesk section (EXP-176) ───
   One scripted, looping scene built ONLY from real-UI recreations: a user
   asks for help through the real feedback widget (Get-help path — support
   requests are what open Support-inbox threads), the thread lands in the
   team's shared Support inbox (composed from the webui/SupportInbox
   recreation + shared fixtures), a member replies, then drops an internal
   note. Stage is decorative (aria-hidden + inert); reduced motion renders
   the finished composite statically. */
import { motion } from "motion/react"
import { useEffect, useState } from "react"
import { EASE_EXPO, eyebrowDraw, sectionReveal } from "../lib/animations"
import { useScenePlayer } from "../lib/use-scene-player"
import { getIssue } from "../ide/data"
import { IcCheck, IcSend } from "../ide/icons"
import { IcMail, IcStickyNote } from "../webui/icons"
import { Bubble, SupportThreadRow } from "../webui/SupportInbox"
import { SUPPORT_THREADS, type SupportThread } from "../webui/data"
import { DownloadIconRow } from "./DownloadSection"
import {
  MegaphoneIcon,
  WidgetPanelDemo,
  type WidgetDemoView,
} from "./WidgetPanelDemo"

/* Beat script (~18.5s loop). Beat 0 is the SSR resting state. */
const B = {
  fab: 0,
  home: 1,
  form: 2,
  sent: 3,
  handoff: 4,
  arrive: 5,
  open: 6,
  inbound: 7,
  reply: 8,
  note: 9,
  hold: 10,
} as const
const BEATS = [900, 1100, 2600, 1600, 700, 1800, 900, 1500, 2400, 2200, 2800]

/* The whole scene is Mara's fixture thread (webui/data.ts) — the same
   conversation the docs' full support-inbox demo shows. */
const MARA = SUPPORT_THREADS[0]
const JONAS = SUPPORT_THREADS[1]
const MARA_ISSUE = getIssue(MARA.issueId)
/* The arriving row previews only the opening message, stamped `now`. */
const MARA_ARRIVING: SupportThread = {
  ...MARA,
  time: `now`,
  messages: [MARA.messages[0]],
}
const LIVE_MESSAGES = MARA.messages
  .slice(0, 3)
  .map((message) => ({ ...message, time: `just now` }))

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
  const noteActive = !reduced && beat >= B.note
  const unread = !reduced && beat >= B.arrive && beat < B.reply

  /* Entrance props — collapse to nothing under reduced motion. */
  const pop = reduced
    ? {}
    : ({
        initial: { opacity: 0, y: 8 },
        animate: { opacity: 1, y: 0 },
        transition: { duration: 0.4, ease: EASE_EXPO },
      } as const)
  const grow = reduced
    ? {}
    : ({
        initial: { opacity: 0, height: 0 },
        animate: { opacity: 1, height: `auto` },
        transition: { duration: 0.45, ease: EASE_EXPO },
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
          <h2 className={`section-title`}>
            Your users, your team, one conversation.
          </h2>
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
              Your users, in the real feedback widget
            </span>
          </div>

          {/* ── Connector: the request travels into the inbox ── */}
          <div className={`co-conn`}>
            <span className={`co-conn-line`} />
            <span className={`co-conn-label`}>lands in your Support inbox</span>
            <span className={`co-conn-line`} />
            <span className={`co-conn-dot`} />
          </div>

          {/* ── The team's Support inbox (real UI composition) ── */}
          <div className={`co-inboxcol`}>
            <div className={`co-card`}>
              <div className={`web-sup-listhead`}>
                <span className={`web-sup-h1`}>Support</span>
                <button
                  type={`button`}
                  className={`web-tab is-small is-active`}
                >
                  Open
                </button>
                <button type={`button`} className={`web-tab is-small`}>
                  Resolved
                </button>
              </div>
              <div className={`co-threads`}>
                {at(B.arrive) && (
                  <motion.div className={`co-rowwrap`} {...grow}>
                    <SupportThreadRow
                      thread={MARA_ARRIVING}
                      unread={unread}
                      selected={at(B.open)}
                      interactive={false}
                    />
                  </motion.div>
                )}
                <SupportThreadRow
                  thread={JONAS}
                  unread={false}
                  selected={false}
                  interactive={false}
                />
              </div>
              {at(B.open) && (
                <motion.div className={`co-convo`} {...grow}>
                  <div className={`web-sup-chathead`}>
                    <div className={`web-sup-chatwho`}>
                      <span className={`web-sup-name`}>
                        {MARA.reporterName}
                      </span>
                      <span className={`web-sup-issuetitle`}>
                        {MARA_ISSUE.title}
                      </span>
                    </div>
                    <button className={`web-btn-outline`} type={`button`}>
                      <IcCheck size={12} />
                      Close
                    </button>
                  </div>
                  <div className={`web-sup-msgs co-msgs`}>
                    {at(B.inbound) && (
                      <motion.div className={`co-bubblewrap is-in`} {...pop}>
                        <Bubble
                          message={LIVE_MESSAGES[0]}
                          reporter={MARA.reporterName}
                        />
                      </motion.div>
                    )}
                    {at(B.reply) && (
                      <motion.div className={`co-bubblewrap is-out`} {...pop}>
                        <Bubble
                          message={LIVE_MESSAGES[1]}
                          reporter={MARA.reporterName}
                        />
                      </motion.div>
                    )}
                    {at(B.note) && (
                      <motion.div className={`co-bubblewrap is-out`} {...pop}>
                        <Bubble
                          message={LIVE_MESSAGES[2]}
                          reporter={MARA.reporterName}
                        />
                      </motion.div>
                    )}
                  </div>
                  <div className={`web-sup-composer`}>
                    <div className={`web-sup-modes`}>
                      <span
                        className={`web-modepill${noteActive ? `` : ` is-active`}`}
                      >
                        <IcMail size={12} />
                        Reply
                      </span>
                      <span
                        className={`web-modepill is-note${noteActive ? ` is-active` : ``}`}
                      >
                        <IcStickyNote size={12} />
                        Internal note
                      </span>
                    </div>
                    <div className={`web-sup-inputrow`}>
                      <span
                        className={`web-composer-input co-composer${noteActive ? ` is-note` : ``}`}
                      >
                        {noteActive
                          ? `Add an internal note… (never sent to the reporter)`
                          : `Reply to ${MARA.reporterName}… (emailed to them)`}
                      </span>
                      <span className={`web-send`}>
                        <IcSend size={14} />
                      </span>
                    </div>
                  </div>
                </motion.div>
              )}
            </div>
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
