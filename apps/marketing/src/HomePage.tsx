import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { ActionsSection } from "./components/ActionsSection"
import { AgentIconRow } from "./components/agent-icons"
import { AgentsSection } from "./components/AgentsSection"
import { CollabSection } from "./components/CollabSection"
import { HeroDownload } from "./components/HeroDownload"
import { HomePricing } from "./components/HomePricing"
import { SocialProofSection } from "./components/SocialProof"
import {
  EASE_EXPO,
  heroChild,
  heroStagger,
  heroTitleStagger,
  heroWord,
} from "./lib/animations"
import { LoopMovie } from "./movie/LoopMovie"

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
                Two authored nowrap lines (block + nowrap), so the H1 height
                is constant at every viewport (EXP-176: no page jump). */}
            <motion.h1 className={`hero-title`} variants={heroTitleStagger}>
              <span className={`hero-title-line`}>
                <motion.span className={`hero-word`} variants={heroWord}>
                  The
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  next
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  generation
                </motion.span>
              </span>
              <span className={`hero-title-line`}>
                <motion.span className={`hero-word`} variants={heroWord}>
                  dev
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  platform
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  for
                </motion.span>
                {` `}
                <motion.span className={`hero-word`} variants={heroWord}>
                  teams
                </motion.span>
              </span>
            </motion.h1>
            <motion.p className={`hero-sub`} variants={heroChild}>
              Issues, customer support and coding agents in one realtime
              tracker. Agents run locally on your machines, on your
              subscription.
            </motion.p>
            <motion.div className={`hero-cta`} variants={heroChild}>
              <HeroDownload />
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

        {/* ── Actions: reusable AI tasks on your own agents (EXP-337) ── */}
        <ActionsSection />

        {/* ── Collaboration: widget → Support inbox, realtime with the
               team (merged Teamwork + Helpdesk, EXP-176) ── */}
        <CollabSection />

        {/* ── Pricing ──────────────────────────── */}
        <HomePricing />

        {/* Testimonial slot — renders null until TESTIMONIALS has entries. */}
        <SocialProofSection />

        <FooterCTA />
      </main>
      <SiteFooter />
    </>
  )
}
