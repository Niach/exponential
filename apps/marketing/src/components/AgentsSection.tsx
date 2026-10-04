/* ─── Agents — your devices run your agents, you steer from anywhere ───
   SLOP-6: the product in its four nouns (People · Devices · Apps · Actions),
   one sentence each, over REAL captures from the committed store (shots/):
   the same run on the desktop IDE and on an iPhone. No bespoke mockups and
   no scripted stage; every claim maps onto a shipped surface. */
import { motion } from "motion/react"
import {
  cardReveal,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import { LINKS } from "../lib/links"
import type { Platform } from "@exp/view-catalog"
import { shotSrc } from "./DocShot"
import { IcArrow, IcBot, IcInbox, IcMonitor, IcUserPlus } from "./icons"

const NOUNS = [
  {
    icon: IcUserPlus,
    title: `People`,
    text: `Invite your team and everyone works on the same boards, synced in realtime on web, desktop, iOS and Android.`,
  },
  {
    icon: IcMonitor,
    title: `Devices`,
    text: `Your desktop or a headless server runs Claude Code or Codex on your own subscription, and you steer every run from any of your devices.`,
  },
  {
    icon: IcInbox,
    title: `Apps`,
    text: `Boards, issues, reviews and your inbox: start a run on an issue, then review and merge the pull request it opens.`,
  },
  {
    icon: IcBot,
    title: `Actions`,
    text: `Save a task as an action and run it on demand, or give it a trigger so a device runs it on a schedule or when something changes on a board.`,
  },
]

function Shot({
  view,
  platform,
  className,
}: {
  view: string
  platform: Platform
  className: string
}) {
  const { src, frame, title } = shotSrc(view, platform)
  return (
    <img
      className={className}
      src={src}
      width={frame.w}
      height={frame.h}
      loading={`lazy`}
      decoding={`async`}
      alt={title}
    />
  )
}

export function AgentsSection() {
  return (
    <section id={`agents`} className={`home-agents`}>
      <div className={`shell`}>
        <div className={`aw-grid`}>
          <motion.div className={`aw-copy`} {...sectionReveal}>
            <h2 className={`section-title`}>Your devices run your agents</h2>
            <p className={`section-sub`}>
              Start a run on an issue and your own machine works on it in a
              fresh worktree with your own agent subscription, then opens a
              pull request. Answer its questions and steer it live from your
              desktop, the web or your phone.
            </p>
            <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
              Get the apps <IcArrow size={12} />
            </a>
          </motion.div>

          <motion.figure className={`aw-shots`} {...sectionReveal}>
            <Shot
              view={`steering`}
              platform={`desktop`}
              className={`aw-shot aw-shot-desktop`}
            />
            <Shot
              view={`steering`}
              platform={`ios`}
              className={`aw-shot aw-shot-phone`}
            />
            <figcaption className={`aw-shots-caption`}>
              The same run on the desktop app and on iPhone
            </figcaption>
          </motion.figure>
        </div>

        <motion.div
          className={`aw-nouns`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={viewportOnce}
        >
          {NOUNS.map(({ icon: Icon, title, text }) => (
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
