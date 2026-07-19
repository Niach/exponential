import { useEffect, useRef, useState } from "react"
import {
  AnimatePresence,
  motion,
  useReducedMotion,
  useScroll,
  useTransform,
} from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { HelpdeskSection } from "./components/HelpdeskSection"
import { HomePricing } from "./components/HomePricing"
import { TeamworkSection } from "./components/TeamworkSection"
import { IcArrow } from "./components/icons"
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
import { LoopClip } from "./movie/LoopClip"
import { LoopMovie } from "./movie/LoopMovie"
import { MobileDemo } from "./mobile/MobileDemo"

const ROTATOR_WORDS = [`app`, `product`, `team`, `roadmap`]
const ROTATOR_INTERVAL_MS = 2400

/* The switching word in the hero H1. SSR renders the first word as plain
   in-flow text (no width style); after mount the slot gets an explicit
   measured width that TRANSITIONS between words, so the centered line
   glides instead of snapping. Words are re-measured once webfonts land. */
function RotatingWord() {
  const reduced = useReducedMotion()
  const [index, setIndex] = useState(0)
  const [width, setWidth] = useState<number | null>(null)
  const [fontsReady, setFontsReady] = useState(false)
  const sizerRefs = useRef<(HTMLSpanElement | null)[]>([])

  useEffect(() => {
    if (reduced) return
    const id = window.setInterval(
      () => setIndex((i) => (i + 1) % ROTATOR_WORDS.length),
      ROTATOR_INTERVAL_MS,
    )
    return () => window.clearInterval(id)
  }, [reduced])

  useEffect(() => {
    let cancelled = false
    document.fonts?.ready.then(() => {
      if (!cancelled) setFontsReady(true)
    })
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    const el = sizerRefs.current[index]
    if (el) setWidth(el.offsetWidth)
  }, [index, fontsReady])

  return (
    <span
      className={`hero-rotator`}
      style={width === null ? undefined : { width }}
    >
      <span className={`hero-rotator-sizers`} aria-hidden>
        {ROTATOR_WORDS.map((word, i) => (
          <span
            key={word}
            ref={(el) => {
              sizerRefs.current[i] = el
            }}
            className={`hero-rotator-sizer`}
          >
            {word}
          </span>
        ))}
      </span>
      <AnimatePresence mode={`popLayout`} initial={false}>
        <motion.span
          key={ROTATOR_WORDS[index]}
          className={`hero-rotator-word`}
          initial={{ opacity: 0, y: `0.55em` }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: `-0.55em` }}
          transition={{ duration: 0.45, ease: EASE_EXPO }}
        >
          {ROTATOR_WORDS[index]}
        </motion.span>
      </AnimatePresence>
    </span>
  )
}

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
              <motion.span className={`hero-word`} variants={heroWord}>
                Make
              </motion.span>{` `}
              <motion.span className={`hero-word`} variants={heroWord}>
                your
              </motion.span>{` `}
              <motion.span className={`hero-word`} variants={heroWord}>
                <RotatingWord />
              </motion.span>{` `}
              <motion.span className={`hero-word`} variants={heroWord}>
                go
              </motion.span>{` `}
              <motion.span className={`hero-word`} variants={heroWord}>
                exponential.
              </motion.span>
            </motion.h1>
            <motion.p className={`hero-sub`} variants={heroChild}>
              The development platform for teams and agents. Feedback in, pull
              requests out. Ship faster.
            </motion.p>
            <motion.div className={`hero-cta`} variants={heroChild}>
              <a className={`btn btn-primary`} href={LINKS.app.login}>
                Get started free <IcArrow size={12} />
              </a>
              <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
                Download the app
              </a>
            </motion.div>
          </motion.div>

          <motion.div
            className={`shell hero-movie-shell`}
            initial={{ opacity: 0, y: 24 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, ease: EASE_EXPO, delay: 0.3 }}
          >
            <p className={`hero-movie-caption`}>
              From bug report to merged PR. Watch it happen.
            </p>
            <LoopMovie />
          </motion.div>
        </section>

        {/* ── Agents ───────────────────────────── */}
        <section id={`agents`}>
          <div className={`shell`}>
            <div className={`home-agents-grid`}>
              <motion.div className={`home-agents-copy`} {...sectionReveal}>
                <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
                  Agents
                </motion.span>
                <h2 className={`section-title`}>Bring your own agents.</h2>
                <p className={`section-sub`}>
                  Start a coding session on any issue. The agent works on a
                  real branch and opens the PR when it&rsquo;s done.
                </p>
              </motion.div>
              <motion.div {...sectionReveal}>
                <LoopClip chapter={`code`} />
              </motion.div>
            </div>
          </div>
        </section>

        {/* ── Helpdesk ─────────────────────────── */}
        <HelpdeskSection />

        {/* ── Teamwork ─────────────────────────── */}
        <TeamworkSection />

        {/* ── Mobile ───────────────────────────── */}
        <section id={`mobile`} className={`home-mobile`}>
          <div className={`shell`}>
            <div className={`home-mobile-grid`} ref={mobileRef}>
              <motion.div className={`home-mobile-copy`} {...sectionReveal}>
                <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
                  Mobile
                </motion.span>
                <h2 className={`section-title`}>
                  Steer your agents from anywhere.
                </h2>
                <p className={`section-sub`}>
                  Watch live coding sessions and steer them by message. Native
                  apps for iOS and Android.
                </p>
                <a className={`btn btn-ghost`} href={LINKS.downloadPage}>
                  Get the apps <IcArrow size={12} />
                </a>
              </motion.div>
              <motion.div style={reduced ? undefined : { y: phoneY }}>
                <MobileDemo autoTour />
              </motion.div>
            </div>
          </div>
        </section>

        {/* ── Pricing ──────────────────────────── */}
        <HomePricing />

        <FooterCTA />
      </main>
      <SiteFooter />
    </>
  )
}
