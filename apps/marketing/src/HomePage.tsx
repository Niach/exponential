import { motion } from "motion/react"
import type { Platform } from "@exp/view-catalog"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { AgentIconRow } from "./components/agent-icons"
import { StatusChip, type Status } from "./components/blog/StatusChip"
import { shotSrc } from "./components/DocShot"
import { HeroDownload } from "./components/HeroDownload"
import { HeroPhone } from "./components/HeroPhone"
import { IcArrow } from "./components/icons"
import { UiDemo } from "./components/UiDemo"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"

/* NEW-15: the home page is the product in four parts (Talk, agents, apps,
   Exponential UI), each one or two sentences over REAL parts: committed
   captures (shots/) and live @exp/ui islands. The hero is an agent's phone
   chat (HeroPhone, src/ui-demos/AgentChat.tsx); Talk states only what has
   shipped, the rest reads "coming". */

function Shot({ view, platform, className }: { view: string; platform: Platform; className: string }) {
  const { src, frame, title } = shotSrc(view, platform)
  return (
    <img className={className} src={src} width={frame.w} height={frame.h} loading={`lazy`} decoding={`async`} alt={title} />
  )
}

const TALK: { text: string; status: Status; label: string }[] = [
  { text: `Push to talk on your desktop`, status: `in-progress`, label: `beta` },
  { text: `Spoken replies in English and German`, status: `in-progress`, label: `beta` },
  { text: `Call your agent from your phone`, status: `planned`, label: `coming` },
  { text: `Wake word`, status: `planned`, label: `coming` },
]

function Feature({
  id,
  title,
  text,
  link,
  children,
}: {
  id: string
  title: string
  text: string
  link?: { href: string; label: string }
  children: React.ReactNode
}) {
  return (
    <section id={id} className={`home-feature`}>
      <div className={`shell aw-grid`}>
        <motion.div className={`aw-copy`} {...sectionReveal}>
          <h2 className={`section-title`}>{title}</h2>
          <p className={`section-sub`}>{text}</p>
          {link && (
            <a className={`btn btn-ghost`} href={link.href}>
              {link.label} <IcArrow size={12} />
            </a>
          )}
        </motion.div>
        <motion.figure className={`aw-shots`} {...sectionReveal}>
          {children}
        </motion.figure>
      </div>
    </section>
  )
}

export function HomePage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className={`home-hero`} id={`product`}>
          <motion.div className={`shell home-hero-grid`} variants={heroStagger} initial={`hidden`} animate={`visible`}>
            <div className={`home-hero-copy`}>
              <motion.h1 className={`hero-title`} variants={heroChild}>
                <span className={`hero-title-line`}>The next generation</span>
                <span className={`hero-title-line`}>dev platform for</span>
                <span className={`hero-title-line`}>teams and agents</span>
              </motion.h1>
              <motion.p className={`hero-sub`} variants={heroChild}>
                Agents plan the work, write the code, build your apps and answer when you talk to them. On web, desktop,
                iOS and Android.
              </motion.p>
              <motion.div className={`hero-cta`} variants={heroChild}>
                <HeroDownload />
              </motion.div>
              <motion.div variants={heroChild}>
                <AgentIconRow />
              </motion.div>
            </div>
            <motion.div variants={heroChild}>
              <HeroPhone />
            </motion.div>
          </motion.div>
        </section>

        <section id={`talk`} className={`home-feature home-talk`}>
          <div className={`shell aw-grid`}>
            <motion.div className={`aw-copy`} {...sectionReveal}>
              <h2 className={`section-title`}>Talk to it</h2>
              <p className={`section-sub`}>
                Hold a key and speak. Your agent gets it as a message and reads its reply aloud.
              </p>
              <ul className={`home-talk-list`}>
                {TALK.map((v) => (
                  <li key={v.text}>
                    <span>{v.text}</span>
                    <StatusChip status={v.status}>{v.label}</StatusChip>
                  </li>
                ))}
              </ul>
              <a className={`btn btn-ghost`} href={`/talk/`}>
                How Talk works <IcArrow size={12} />
              </a>
            </motion.div>
            <motion.figure className={`home-talk-stage`} {...sectionReveal}>
              <UiDemo id={`voice-overlay`} />
            </motion.figure>
          </div>
        </section>

        <Feature
          id={`agents`}
          title={`Agents work your boards`}
          text={`They run Claude Code or Codex on your own machine and subscription. Give one a job, message it like a teammate, review its pull request. Reports from the feedback widget on your site land on its board.`}
          link={{ href: LINKS.downloadPage, label: `Get the apps` }}
        >
          <Shot view={`agent-channel`} platform={`desktop`} className={`aw-shot aw-shot-desktop`} />
          <Shot view={`agent-channel`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
        </Feature>

        <Feature
          id={`apps`}
          title={`Ask for an app`}
          text={`Describe it and watch it build in a live preview. Your team opens it on every platform.`}
          link={{ href: `/docs/apps/`, label: `How apps work` }}
        >
          <Shot view={`app`} platform={`desktop`} className={`aw-shot aw-shot-desktop`} />
          <Shot view={`app`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
        </Feature>

        <Feature
          id={`ui`}
          title={`One UI on every platform`}
          text={`Agents and apps draw with Exponential UI, our open framework. React, SwiftUI, Compose and the desktop paint it the same.`}
          link={{ href: `/ui/`, label: `Exponential UI` }}
        >
          <Shot view={`exponential-ui-demo`} platform={`desktop`} className={`aw-shot aw-shot-desktop`} />
          <Shot view={`exponential-ui-demo`} platform={`ios`} className={`aw-shot aw-shot-phone`} />
        </Feature>

        <section id={`pricing`} className={`home-price-line`}>
          <motion.div className={`shell`} {...sectionReveal}>
            <h2 className={`section-title`}>Free for teams of three</h2>
            <p className={`section-sub`}>€12 per seat after that. Free to self-host.</p>
            <div className={`home-pricing-links`}>
              <a className={`btn btn-primary`} href={LINKS.app.login}>
                Get started free <IcArrow size={12} />
              </a>
              <a className={`btn btn-ghost`} href={`/pricing/`}>
                Pricing
              </a>
            </div>
          </motion.div>
        </section>
      </main>
      <SiteFooter />
    </>
  )
}
