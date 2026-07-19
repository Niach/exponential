/* ─── Agents — merged "Bring your own agents" + Mobile section (EXP-176) ───
   One scripted, looping scene: the REAL Start-coding dialog (the faithful
   ide/ recreation, verified against the desktop's start_coding_dialog.rs)
   hands off to an infographic that is deliberately NOT product UI — Claude
   running on a MacBook with a phone connected for live watch + steer.
   Stage is decorative (aria-hidden + inert); reduced motion renders the
   dialog and the finished infographic statically, side by side. */
import { AnimatePresence, motion } from "motion/react"
import { EASE_EXPO, eyebrowDraw, sectionReveal } from "../lib/animations"
import { useScenePlayer } from "../lib/use-scene-player"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"
import { StartCodingDialog } from "../ide/StartCodingDialog"
import { IdeContext, type IdeApi } from "../ide/state"

/* Beat script (~13.5s loop). Beat 0 is the SSR resting state. */
const B = {
  dialog: 0,
  armed: 1,
  handoff: 2,
  running: 3,
  steer: 4,
  hold: 5,
} as const
const BEATS = [2400, 1000, 700, 3800, 3200, 2400]

/* Static IdeApi stub — just enough context for the real StartCodingDialog
   recreation to render its resting state (EXP-8 pre-checked, plan mode on).
   Pure module-scope data + no-ops: SSR-safe by construction. */
const noop = () => {}
const EMPTY = new Set<string>()
const STATIC_IDE: IdeApi = {
  interactive: false,
  tool: `issues`,
  setTool: noop,
  tabs: [],
  active: null,
  selectTab: noop,
  closeTab: noop,
  openIssue: noop,
  openFile: noop,
  openSourceControl: noop,
  filter: `all`,
  setFilter: noop,
  collapsedGroups: EMPTY,
  toggleGroup: noop,
  expandedDirs: EMPTY,
  toggleDir: noop,
  selectedFile: null,
  selectFile: noop,
  viewedBranch: `master`,
  viewBranch: noop,
  changes: [],
  staged: EMPTY,
  toggleStaged: noop,
  commits: [],
  commitAll: noop,
  ahead: 0,
  push: noop,
  inboxRead: EMPTY,
  markInboxRead: noop,
  markAllInboxRead: noop,
  mergedReviews: EMPTY,
  goneReviews: EMPTY,
  mergeReview: noop,
  coding: `idle`,
  codingTarget: null,
  codingScript: [],
  codedIssues: EMPTY,
  pendingCoding: { kind: `issue`, id: `EXP-8` },
  requestCoding: noop,
  cancelStartCoding: noop,
  confirmStartCoding: noop,
  stopCoding: noop,
  scriptPos: { done: 0, chars: 0 },
  dockOpen: false,
  setDockOpen: noop,
  dockTab: `shell`,
  setDockTab: noop,
}

/* The Claude→phone infographic — stylized devices (plain CSS shapes, not
   product UI); every text label is fixed-px HTML so it stays readable at
   any viewport width. */
function DeviceLink() {
  return (
    <div className={`aw-info`}>
      <div className={`aw-device aw-laptopcol`}>
        <span className={`aw-pr-chip`}>PR #214 opened</span>
        <div className={`aw-laptop`}>
          <div className={`aw-laptop-screen`}>
            <span className={`aw-term-tag`}>claude · exp/EXP-8</span>
            <span className={`aw-term-line is-w80`} />
            <span className={`aw-term-line is-w60`} />
            <span className={`aw-term-line is-w72`} />
            <span className={`aw-term-line is-w45`} />
          </div>
          <div className={`aw-laptop-base`} />
        </div>
        <span className={`aw-device-caption`}>
          Claude runs in your desktop IDE
        </span>
      </div>

      <div className={`aw-link`}>
        <svg
          className={`aw-link-svg`}
          viewBox={`0 0 110 70`}
          preserveAspectRatio={`none`}
          aria-hidden
        >
          <path className={`aw-link-path`} d={`M4 54 C 32 16, 78 16, 106 54`} />
        </svg>
        <span className={`aw-link-dot`} />
        <span className={`aw-steer-chip`}>Cap the backoff at 15s</span>
      </div>

      <div className={`aw-device aw-phonecol`}>
        <div className={`aw-phone`}>
          <span className={`aw-live-chip`}>
            <span className={`aw-live-dot`} />
            Live
          </span>
          <span className={`aw-msg is-w85`} />
          <span className={`aw-msg is-w65`} />
          <span className={`aw-msg is-reply`} />
        </div>
        <span className={`aw-device-caption`}>Steer from your phone</span>
      </div>
    </div>
  )
}

export function AgentsSection() {
  const { ref, beat, reduced } = useScenePlayer(BEATS)

  const showDialog = reduced || beat <= B.armed
  const showInfo = reduced || beat >= B.handoff

  const stageClass = [
    `aw-stage`,
    !reduced && beat === B.armed ? `is-armed` : ``,
    reduced || beat >= B.running ? `is-running` : ``,
    reduced || beat >= B.steer ? `is-steer` : ``,
    reduced ? `is-static` : ``,
  ]
    .filter(Boolean)
    .join(` `)

  return (
    <section id={`agents`} className={`home-agents`}>
      <div className={`shell`}>
        <div className={`aw-grid`}>
          <motion.div className={`aw-copy`} {...sectionReveal}>
            <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
              Agents
            </motion.span>
            <h2 className={`section-title`}>Work on issues from anywhere.</h2>
            <p className={`section-sub`}>
              Pick an issue &mdash; or a batch &mdash; and hit Start coding.
              Claude works on a real branch in the desktop IDE and opens the PR
              when it&rsquo;s done. Watch the session live and steer it by
              message from your phone.
            </p>
            <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
              Get the apps <IcArrow size={12} />
            </a>
          </motion.div>

          <div className={stageClass} ref={ref} aria-hidden inert>
            <AnimatePresence initial={false}>
              {showDialog && (
                <motion.div
                  key={`dialog`}
                  className={`aw-dialog`}
                  initial={reduced ? false : { opacity: 0, y: 16 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={
                    reduced ? undefined : { opacity: 0, scale: 0.94, x: -32 }
                  }
                  transition={{ duration: 0.5, ease: EASE_EXPO }}
                >
                  <IdeContext.Provider value={STATIC_IDE}>
                    <StartCodingDialog />
                  </IdeContext.Provider>
                </motion.div>
              )}
              {showInfo && (
                <motion.div
                  key={`info`}
                  className={`aw-infowrap`}
                  initial={reduced ? false : { opacity: 0, y: 16 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.5, ease: EASE_EXPO, delay: 0.15 }}
                >
                  <DeviceLink />
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>
    </section>
  )
}
