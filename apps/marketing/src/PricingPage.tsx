import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { EnterpriseLine, PlanCards, SelfHostCards } from "./components/PlanCards"
import { ComparisonTable } from "./components/ComparisonTable"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { EVERY_PLAN_INCLUDES } from "./lib/plans"

export function PricingPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="hero pricing-hero">
          <motion.div
            className="shell hero-content"
            variants={heroStagger}
            initial="hidden"
            animate="visible"
          >
            <motion.h1 className="hero-title" variants={heroChild}>
              Free for teams of three.
              <br />
              <em>One plan after that.</em>
            </motion.h1>
            <motion.p className="hero-sub" variants={heroChild}>
              Start with your whole team for free. One paid plan with
              everything in it — no feature matrix to decode.
            </motion.p>
          </motion.div>
        </section>

        <section style={{ paddingTop: 0 }}>
          <div className="shell">
            <PlanCards />
            <EnterpriseLine />
            <p className="plan-footnote">
              {EVERY_PLAN_INCLUDES} Agents are free everywhere — you only ever
              pay for people.
            </p>
          </div>
        </section>

        <section
          id="self-host"
          style={{
            background: `var(--bg-elev)`,
            borderTop: `1px solid var(--border)`,
            borderBottom: `1px solid var(--border)`,
          }}
        >
          <motion.div className="shell" {...sectionReveal}>
            <span className="section-eyebrow">Run it yourself</span>
            <h2 className="section-title">Self-host it. Your data.</h2>
            <p className="section-sub">
              Every feature unlocked, on your own hardware. Free under the
              Exponential Small Team License while you&apos;re under 10 people
              — published annual pricing above that, no negotiation required.
            </p>
            <SelfHostCards />
          </motion.div>
        </section>

        <section id="compare">
          <motion.div className="shell" {...sectionReveal}>
            <span className="section-eyebrow">Comparison</span>
            <h2 className="section-title">Exponential vs. Linear.</h2>
            <p className="section-sub">
              A great tracker — but it bills for AI agents, runs only in their
              cloud, and can&apos;t be self-hosted.
            </p>
            <ComparisonTable />
          </motion.div>
        </section>

        <FooterCTA
          title="Start free today."
          subtitle="Three seats and your coding agents, free forever. No card required."
        />
      </main>
      <SiteFooter />
    </>
  )
}
