import { useRef } from "react"
import { motion, useReducedMotion, useScroll, useTransform } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { HelpdeskSection } from "./components/HelpdeskSection"
import { TeamworkSection } from "./components/TeamworkSection"
import { IcArrow, IcGithub } from "./components/icons"
import {
  EASE_EXPO,
  eyebrowDraw,
  heroChild,
  heroStagger,
  heroTitleStagger,
  heroWord,
  sectionReveal,
} from "./lib/animations"
import { LINKS } from "./lib/links"
import { IdeDemo } from "./ide/Ide"
import { LoopMovie } from "./movie/LoopMovie"
import { MobileDemo } from "./mobile/MobileDemo"

const HERO_TITLE_WORDS = `Issue tracking that ships code.`.split(` `)

export function HomePage() {
  const reduced = useReducedMotion()
  const mobileRef = useRef<HTMLDivElement>(null)
  const { scrollYProgress } = useScroll({
    target: mobileRef,
    offset: [`start end`, `end start`],
  })
  const phoneY = useTransform(scrollYProgress, [0, 1], [12, -12])

  return (
    <>
      <SiteHeader />

      <main>
        {/* ── Hero ─────────────────────────────── */}
        <section className={`hero`} id={`product`}>
          <motion.div
            className={`shell hero-content`}
            variants={heroStagger}
            initial={`hidden`}
            animate={`visible`}
          >
            {/* Words are individually animated spans; the real space text
                nodes between them keep copy/screen-reader output intact. */}
            <motion.h1 className={`hero-title`} variants={heroTitleStagger}>
              {HERO_TITLE_WORDS.map((word, index) => (
                <span key={index}>
                  <motion.span className={`hero-word`} variants={heroWord}>
                    {word}
                  </motion.span>
                  {index < HERO_TITLE_WORDS.length - 1 ? ` ` : null}
                </span>
              ))}
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
            style={{ transformPerspective: 1200 }}
            initial={{ opacity: 0, y: 24, rotateX: reduced ? 0 : 4 }}
            animate={{ opacity: 1, y: 0, rotateX: 0 }}
            transition={{ duration: 0.8, ease: EASE_EXPO, delay: 0.3 }}
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
            <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
              The loop
            </motion.span>
            <h2 className={`section-title`}>
              From complaint to shipped &mdash; without leaving the loop.
            </h2>
            <p className={`section-sub`}>
              Watch a bug travel: reported from a customer&rsquo;s site, on
              your board in seconds, fixed by Claude at your desk, merged and
              answered.
            </p>
            <LoopMovie />
          </motion.div>
        </section>

        {/* ── Helpdesk ─────────────────────────── */}
        <HelpdeskSection />

        {/* ── Teamwork ─────────────────────────── */}
        <TeamworkSection />

        {/* ── Mobile ───────────────────────────── */}
        <section id={`mobile`} className={`home-mobile`}>
          <div className={`shell`}>
            <div className={`home-mobile-grid`} ref={mobileRef}>
              <motion.div style={reduced ? undefined : { y: phoneY }}>
                <MobileDemo autoTour />
              </motion.div>
              <motion.div className={`home-mobile-copy`} {...sectionReveal}>
                <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
                  Mobile
                </motion.span>
                <h2 className={`section-title`}>
                  Steer Claude from your pocket.
                </h2>
                <p className={`section-sub`}>
                  A coding session is running on your desk &mdash; watch the
                  live session and steer it by message, from wherever you are.
                  Native apps for iOS and Android.
                </p>
                <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
                  Get the apps <IcArrow size={12} />
                </a>
              </motion.div>
            </div>
          </div>
        </section>

        <FooterCTA />
      </main>
      <SiteFooter />
    </>
  )
}
