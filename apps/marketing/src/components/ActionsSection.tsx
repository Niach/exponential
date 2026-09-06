/* ─── Actions — reusable AI tasks on your own agents (EXP-337) ───
   Copy-first glass-card grid; the heavy scripted stages belong to the
   Agents and Collaboration sections. */
import { motion } from "motion/react"
import {
  cardReveal,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import {
  IcCal,
  IcGitMerge,
  IcGitPr,
  IcRocket,
  IcSparkles,
  IcZap,
} from "./icons"

const ACTIONS = [
  {
    icon: IcGitPr,
    title: `Code reviews`,
    text: `Run a review action on any pull request. Your agent reads the diff and reports what matters.`,
  },
  {
    icon: IcGitMerge,
    title: `Fix merge conflicts`,
    text: `One click rebases the PR branch, resolves the conflicts and merges.`,
  },
  {
    icon: IcRocket,
    title: `Deploy your server`,
    text: `Ship a release, run a migration, restart a service.`,
  },
  {
    icon: IcSparkles,
    title: `Custom AI tasks`,
    text: `Describe any task once, save it as an action, and run it on demand from web, desktop or your phone.`,
  },
]

export function ActionsSection() {
  return (
    <section id={`actions`} className={`home-actions`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <h2 className={`section-title`}>Use actions for AI tasks</h2>
          <p className={`section-sub`}>
            Deploy your server, do code reviews, fix merge conflicts. Every
            action runs on your own agents, on your own machines.
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

        {/* ── Automations (EXP-583): actions bound to a device, fired by a
               schedule or an event, always running locally. ── */}
        <motion.div className={`glass-card ac-autos`} {...sectionReveal}>
          <div className={`ac-autos-head`}>
            <span className={`ac-autos-badge`}>New</span>
            <h3 className={`ac-autos-title`}>Automations</h3>
          </div>
          <p className={`ac-autos-text`}>
            Bind any action to one of your devices and run it on a schedule or
            when something happens. Sort incoming issues, autofix them, review
            every PR each morning. Everything runs on your own machines, on
            your own subscription.
          </p>
          <div className={`ac-autos-chips`}>
            <span className={`ac-autos-chip`}>
              <IcCal size={13} />
              On a schedule
            </span>
            <span className={`ac-autos-chip`}>
              <IcZap size={13} />
              When something happens
            </span>
          </div>
        </motion.div>
      </div>
    </section>
  )
}
