/* ─── Actions — reusable AI tasks on your own agents (EXP-337) ───
   Copy-first glass-card grid; the heavy scripted stages belong to the
   Agents and Collaboration sections. */
import { motion } from "motion/react"
import {
  cardReveal,
  eyebrowDraw,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import { IcGitMerge, IcGitPr, IcRocket, IcSparkles } from "./icons"

const ACTIONS = [
  {
    icon: IcGitPr,
    title: `Code reviews`,
    text: `Run a review action on any pull request — your agent reads the diff and reports what matters.`,
  },
  {
    icon: IcGitMerge,
    title: `Fix merge conflicts`,
    text: `One click rebases the PR branch, resolves the conflicts and merges — in the branch's own worktree.`,
  },
  {
    icon: IcRocket,
    title: `Deploy your server`,
    text: `Ship a release, run a migration, restart a service — your runbooks become one-click actions.`,
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
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Actions
          </motion.span>
          <h2 className={`section-title`}>Use actions for AI tasks.</h2>
          <p className={`section-sub`}>
            Deploy your server, do code reviews, fix merge conflicts &mdash;
            every action runs on your own agents, on your own machines.
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
      </div>
    </section>
  )
}
