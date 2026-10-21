/* ─── Prompts: saved team prompts, run from the composer's `/` menu ───
   Copy-first glass-card grid (EXP-337); NEW-2: prompts replaced actions, so
   the feature block shows the REAL composer `/` menu from the committed
   store (shots/composer-slash-menu). */
import { motion } from "motion/react"
import {
  cardReveal,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import { shotSrc } from "./DocShot"
import {
  IcCompose,
  IcMessage,
  IcPlay,
  IcSettings,
  IcShield,
  IcTerminalSquare,
} from "./icons"

const PROMPTS = [
  {
    icon: IcSettings,
    title: `Save a prompt`,
    text: `Give it a name, an icon and a body in Settings → Prompts.`,
  },
  {
    icon: IcPlay,
    title: `Run it from /`,
    text: `Type / in the Home composer and pick it. The session starts with the prompt plus what you typed.`,
  },
  {
    icon: IcTerminalSquare,
    title: `Reuse it in a session`,
    text: `In a live session, / puts the prompt's body in your draft.`,
  },
  {
    icon: IcShield,
    title: `Owners write, everyone runs`,
    text: `Owners create and edit prompts. Every member can run them.`,
  },
]

const SLASH_MENU = shotSrc(`composer-slash-menu`, `web`)

export function ActionsSection() {
  return (
    <section id={`prompts`} className={`home-actions`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <h2 className={`section-title`}>Save prompts for the team</h2>
          <p className={`section-sub`}>
            Write a prompt once and run it from any composer.
          </p>
        </motion.div>
        <motion.div
          className={`ac-grid`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={viewportOnce}
        >
          {PROMPTS.map(({ icon: Icon, title, text }) => (
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

        {/* ── The `/` menu: the Home composer and a live session. ── */}
        <motion.div className={`glass-card ac-triggers`} {...sectionReveal}>
          <div className={`ac-triggers-copy`}>
            <div className={`ac-triggers-head`}>
              <span className={`ac-triggers-badge`}>New</span>
              <h3 className={`ac-triggers-title`}>The / menu</h3>
            </div>
            <p className={`ac-triggers-text`}>
              Type / to list the team&apos;s prompts. A live session adds
              /compact, /model and /effort.
            </p>
            <div className={`ac-triggers-chips`}>
              <span className={`ac-triggers-chip`}>
                <IcCompose size={13} />
                Home composer
              </span>
              <span className={`ac-triggers-chip`}>
                <IcMessage size={13} />
                Live session
              </span>
            </div>
          </div>
          <img
            className={`ac-triggers-shot`}
            src={SLASH_MENU.src}
            width={SLASH_MENU.frame.w}
            height={SLASH_MENU.frame.h}
            loading={`lazy`}
            decoding={`async`}
            alt={SLASH_MENU.title}
          />
        </motion.div>
      </div>
    </section>
  )
}
