import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcGithub } from "./components/icons"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"
import { IdeDemo } from "./ide/Ide"
import { LoopCircle } from "./loop/LoopCircle"
import { WidgetPreview } from "./loop/WidgetPreview"
import { MobileDemo } from "./mobile/MobileDemo"

export function HomePage() {
  return (
    <>
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
          <motion.div variants={heroChild}>
            <a className={`hero-dl-link`} href={LINKS.downloadPage}>
              Download the desktop app <IcArrow size={12} />
            </a>
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
            This is the desktop IDE. Go ahead &mdash; click around.
          </p>
        </motion.div>
      </section>

      {/* ── The loop ─────────────────────────── */}
      <section id={`loop`}>
        <motion.div className={`shell home-loop`} {...sectionReveal}>
          <span className={`section-eyebrow`}>The loop</span>
          <div className={`home-loop-grid`}>
            <LoopCircle />
            <WidgetPreview />
          </div>
        </motion.div>
      </section>

      {/* ── Mobile ───────────────────────────── */}
      <section id={`mobile`} className={`home-mobile`}>
        <div className={`shell`}>
          <div className={`home-mobile-grid`}>
            <MobileDemo autoTour />
            <motion.div className={`home-mobile-copy`} {...sectionReveal}>
              <span className={`section-eyebrow`}>Mobile</span>
              <h2 className={`section-title`}>
                Steer Claude from your pocket.
              </h2>
              <p className={`section-sub`}>
                A coding session is running on your desk &mdash; watch the live
                session and steer it by message, from wherever you are. Native
                apps for iOS and Android.
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
