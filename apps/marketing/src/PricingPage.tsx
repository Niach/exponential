import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { PlanCards, SelfHostCards } from "./components/PlanCards"
import { ComparisonTable } from "./components/ComparisonTable"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"

export function PricingPage() {
  return (
    <>
      <SiteHeader />

      <section className="hero pricing-hero">
        <div className="hero-atmos" aria-hidden />
        <motion.div
          className="shell hero-content"
          variants={heroStagger}
          initial="hidden"
          animate="visible"
        >
          <motion.h1 className="hero-title" variants={heroChild}>
            Pay for people.
            <br />
            <em>Never for agents.</em>
          </motion.h1>
          <motion.p className="hero-sub" variants={heroChild}>
            One price per seat — that&apos;s it. Projects, repositories and
            Claude coding sessions are unlimited on every tier, and push and
            steer are free everywhere. Start solo for free.
          </motion.p>
        </motion.div>
      </section>

      <section style={{ paddingTop: 0 }}>
        <div className="shell">
          <PlanCards />
          <p className="plan-footnote">
            Seats count people, never agents. Pro is billed yearly; Business is
            monthly or yearly. Change your seat count any time from workspace
            settings.
          </p>
        </div>
      </section>

      <section
        id="self-host"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 50%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">Run it yourself</span>
          <h2 className="section-title">Self-host it, free and unlimited.</h2>
          <p className="section-sub">
            Every feature, no seat count, no storage cap — on your own hardware
            under the Elastic License 2.0. Running it across a bigger company?
            Enterprise adds prioritized support.
          </p>
          <SelfHostCards />
        </motion.div>
      </section>

      <section id="compare">
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">Comparison</span>
          <h2 className="section-title">How it compares to Linear.</h2>
          <p className="section-sub">
            Linear is a great tracker. But it bills per seat for its agents
            too, runs only in their cloud, and its agents run on their
            machines — not yours.
          </p>
          <ComparisonTable />
        </motion.div>
      </section>

      <FooterCTA
        title="Start free. Grow one seat at a time."
        subtitle="A workspace for you and your coding agents, free forever. No credit card required."
      />
      <SiteFooter />
    </>
  )
}
