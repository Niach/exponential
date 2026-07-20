import { useEffect, useRef, useState } from "react"
import { AnimatePresence, motion, useReducedMotion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { AgentIconRow } from "./components/agent-icons"
import { AgentsSection } from "./components/AgentsSection"
import { CollabSection } from "./components/CollabSection"
import { HomePricing } from "./components/HomePricing"
import { IcArrow } from "./components/icons"
import {
  EASE_EXPO,
  heroChild,
  heroStagger,
  heroTitleStagger,
  heroWord,
} from "./lib/animations"
import { LINKS } from "./lib/links"
import { LoopMovie } from "./movie/LoopMovie"

/* The hero slot machine spins through these once and lands on the last
   entry — the segment includes `your` so the final headline reads
   "Make yourself go exponential." (EXP-176). */
const SLOT_SEGMENTS = [
  `your app`,
  `your product`,
  `your team`,
  `your website`,
  `your business`,
  `yourself`,
]
/* Delay in ms BEFORE leaving segment i: hold the opener while the hero
   entrance settles, fly through the middle, decelerate into the landing. */
const SLOT_DELAYS = [1400, 280, 240, 300, 430]
const SLOT_LAST = SLOT_SEGMENTS.length - 1

/* The switching segment in the hero H1 — a FINITE slot-machine reel, not a
   loop: one decelerating pass that settles on `yourself` and never runs
   again. SSR renders segment 0 as plain in-flow text (no width style);
   after mount the slot gets an explicit measured width that TRANSITIONS
   between segments, so the line glides instead of snapping. Segments are
   re-measured once webfonts land. Reduced motion jumps straight to the
   final segment. */
function RotatingWord() {
  const reduced = useReducedMotion()
  const [index, setIndex] = useState(0)
  const [width, setWidth] = useState<number | null>(null)
  const [fontsReady, setFontsReady] = useState(false)
  const sizerRefs = useRef<(HTMLSpanElement | null)[]>([])
  const doneRef = useRef(false)

  useEffect(() => {
    if (doneRef.current) return
    if (reduced) {
      doneRef.current = true
      setIndex(SLOT_LAST)
      return
    }
    let step = 0
    let id: number
    const advance = () => {
      step += 1
      setIndex(step)
      if (step < SLOT_LAST) {
        id = window.setTimeout(advance, SLOT_DELAYS[step])
      } else {
        doneRef.current = true
      }
    }
    id = window.setTimeout(advance, SLOT_DELAYS[0])
    return () => window.clearTimeout(id)
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

  const landing = index === SLOT_LAST
  return (
    <span
      className={`hero-rotator`}
      style={width === null ? undefined : { width }}
    >
      <span className={`hero-rotator-sizers`} aria-hidden>
        {SLOT_SEGMENTS.map((segment, i) => (
          <span
            key={segment}
            ref={(el) => {
              sizerRefs.current[i] = el
            }}
            className={`hero-rotator-sizer`}
          >
            {segment}
          </span>
        ))}
      </span>
      {/* Flight props collapse under reduced motion so the single
          0→final index hop is an instant text swap (framer does not
          honor the OS preference on its own). */}
      <AnimatePresence mode={`popLayout`} initial={false}>
        <motion.span
          key={index}
          className={`hero-rotator-word`}
          initial={reduced ? false : { opacity: 0, y: `0.6em` }}
          animate={
            !reduced && landing
              ? { opacity: 1, y: [`0.6em`, `-0.05em`, `0em`] }
              : { opacity: 1, y: 0 }
          }
          exit={reduced ? { opacity: 0 } : { opacity: 0, y: `-0.6em` }}
          transition={
            reduced
              ? { duration: 0 }
              : landing
                ? { duration: 0.55, ease: EASE_EXPO }
                : { duration: 0.24, ease: `easeOut` }
          }
        >
          {SLOT_SEGMENTS[index]}
        </motion.span>
      </AnimatePresence>
    </span>
  )
}

export function HomePage() {
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
                nodes between them keep copy/screen-reader output intact.
                The two authored lines are fixed (block + nowrap) so the
                slot machine can never move the wrap point — the H1 height
                is constant at every viewport (EXP-176: no page jump). */}
            <motion.h1 className={`hero-title`} variants={heroTitleStagger}>
              <span className={`hero-title-line`}>
                <motion.span className={`hero-word`} variants={heroWord}>
                  Make
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  <RotatingWord />
                </motion.span>
              </span>
              {` `}
              <span className={`hero-title-line`}>
                <motion.span className={`hero-word`} variants={heroWord}>
                  go
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  exponential.
                </motion.span>
              </span>
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
            <motion.div variants={heroChild}>
              <AgentIconRow />
            </motion.div>
          </motion.div>

          <motion.div
            className={`shell hero-movie-shell`}
            initial={{ opacity: 0, y: 24 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, ease: EASE_EXPO, delay: 0.3 }}
          >
            <LoopMovie />
          </motion.div>
        </section>

        {/* ── Agents: Start coding from your phone → your agent on the
               desktop, steered live (merged Agents + Mobile, EXP-176) ── */}
        <AgentsSection />

        {/* ── Collaboration: widget → Support inbox, realtime with the
               team (merged Teamwork + Helpdesk, EXP-176) ── */}
        <CollabSection />

        {/* ── Pricing ──────────────────────────── */}
        <HomePricing />

        <FooterCTA />
      </main>
      <SiteFooter />
    </>
  )
}
