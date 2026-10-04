/* ─── Actions — saved tasks your devices run, on demand or by trigger ───
   Copy-first glass-card grid (EXP-337); SLOP-6: the cards name the shipped
   builtins plus your own, and the triggers block shows the REAL action page
   from the committed store (shots/action-page). */
import { motion } from "motion/react"
import {
  cardReveal,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import { shotSrc } from "./DocShot"
import {
  IcCal,
  IcGitMerge,
  IcRocket,
  IcSparkles,
  IcWrench,
  IcZap,
} from "./icons"

/* The first three are the builtins every team gets (apps/web
   lib/builtin-actions.ts); the last is the team's own. */
const ACTIONS = [
  {
    icon: IcGitMerge,
    title: `Fix merge conflicts`,
    text: `Pick a conflicted pull request and your agent rebases it, resolves the conflicts and merges it.`,
  },
  {
    icon: IcWrench,
    title: `Tidy up`,
    text: `Your agent dedupes, labels and links a board's issues. Nothing is deleted.`,
  },
  {
    icon: IcSparkles,
    title: `Create action`,
    text: `Describe a new action in plain words and your agent writes it for the team.`,
  },
  {
    icon: IcRocket,
    title: `Your own`,
    text: `Nightly test triage, a deploy, a release: write the prompt once, pick a repository and run it from any app.`,
  },
]

const ACTION_PAGE = shotSrc(`action-page`, `web`)

export function ActionsSection() {
  return (
    <section id={`actions`} className={`home-actions`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <h2 className={`section-title`}>Use actions for AI tasks</h2>
          <p className={`section-sub`}>
            Save a task once and run it again whenever you need it. Every
            action runs on one of your devices, with your own agent.
          </p>
        </motion.div>
        <motion.div
          className={`ac-grid`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={viewportOnce}
        >
          {ACTIONS.map(({ icon: Icon, title, text }) => (
            <motion.div
              key={title}
              className={`glass-card ac-card`}
              variants={cardReveal}
            >
              <span className={`ac-card-icon`}>
                <Icon size={17} stroke={1.7} />
              </span>
              <span className={`ac-card-title`}>{title}</span>
              <span className={`ac-card-text`}>{text}</span>
            </motion.div>
          ))}
        </motion.div>

        {/* ── Triggers: an action bound to a device, fired by a schedule or
               an event, always running locally. ── */}
        <motion.div className={`glass-card ac-triggers`} {...sectionReveal}>
          <div className={`ac-triggers-copy`}>
            <div className={`ac-triggers-head`}>
              <span className={`ac-triggers-badge`}>New</span>
              <h3 className={`ac-triggers-title`}>Triggers</h3>
            </div>
            <p className={`ac-triggers-text`}>
              Give an action a trigger and the device you pick runs it daily,
              weekly or monthly, or when an issue is created, changes status
              or gets its pull request opened or merged.
            </p>
            <div className={`ac-triggers-chips`}>
              <span className={`ac-triggers-chip`}>
                <IcCal size={13} />
                On a schedule
              </span>
              <span className={`ac-triggers-chip`}>
                <IcZap size={13} />
                When something happens
              </span>
            </div>
          </div>
          <img
            className={`ac-triggers-shot`}
            src={ACTION_PAGE.src}
            width={ACTION_PAGE.frame.w}
            height={ACTION_PAGE.frame.h}
            loading={`lazy`}
            decoding={`async`}
            alt={ACTION_PAGE.title}
          />
        </motion.div>
      </div>
    </section>
  )
}
