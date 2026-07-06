import { motion } from "motion/react"
import { DownloadIconRow } from "./components/DownloadSection"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { WidgetEmbed } from "./components/WidgetEmbed"
import { IcArrow, IcGithub } from "./components/icons"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"
import { IdeDemo } from "./ide/Ide"
import { LoopCircle } from "./loop/LoopCircle"
import { MobileDemo } from "./mobile/MobileDemo"

export function HomePage() {
  return (
    <>
      <WidgetEmbed />
      <SiteHeader />

      {/* ── Hero ─────────────────────────────── */}
      <section className={`hero`} id={`product`}>
        <motion.div
          className={`shell hero-content`}
          variants={heroStagger}
          initial={`hidden`}
          animate={`visible`}
        >
          <motion.h1 className={`hero-title`} variants={heroChild}>
            Issue tracking that ships code.
          </motion.h1>
          <motion.p className={`hero-sub`} variants={heroChild}>
            An issue tracker with a built-in coding IDE. Feedback in, pull
            requests out.
          </motion.p>
          <motion.div className={`hero-cta`} variants={heroChild}>
            <a className={`btn btn-primary`} href={LINKS.app.register}>
              Get started free <IcArrow size={12} />
            </a>
            <a className={`btn btn-ghost`} href={LINKS.github.repo}>
              <IcGithub size={14} /> GitHub
            </a>
          </motion.div>
          <motion.div className={`home-hero-dl`} variants={heroChild}>
            <DownloadIconRow />
          </motion.div>
        </motion.div>

        <motion.div
          className={`shell`}
          initial={{ opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, ease: `easeOut`, delay: 0.3 }}
        >
          <div className={`home-ide-wrap`}>
            <IdeDemo />
          </div>
          <p className={`home-ide-caption`}>
            The desktop IDE &mdash; try it, it&apos;s real.
          </p>
        </motion.div>
      </section>

      {/* ── The loop ─────────────────────────── */}
      <section id={`loop`}>
        <motion.div className={`shell home-loop`} {...sectionReveal}>
          <span className={`section-eyebrow`}>The loop</span>
          <LoopCircle />
          <p className={`home-loop-sub`}>
            Feedback becomes an issue, Claude writes the fix, and the pull
            request ships.
          </p>
        </motion.div>
      </section>

      {/* ── Mobile ───────────────────────────── */}
      <section id={`mobile`} className={`home-mobile`}>
        <div className={`shell`}>
          <div className={`home-mobile-grid`}>
            <MobileDemo />
            <motion.div className={`home-mobile-copy`} {...sectionReveal}>
              <span className={`section-eyebrow`}>Mobile</span>
              <h2 className={`section-title`}>Native in your pocket.</h2>
              <p className={`section-sub`}>
                SwiftUI on iOS, Compose on Android &mdash; the same live data,
                down to the terminal of a running coding session.
              </p>
              <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
                Get the apps <IcArrow size={12} />
              </a>
            </motion.div>
          </div>
        </div>
      </section>

      <FooterCTA />
      <SiteFooter />
    </>
  )
}
