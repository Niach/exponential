import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { PlanCards, FoundingCallout } from "./components/PlanCards"
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
            Per workspace. Per year.
            <br />
            <em>Not per seat.</em>
          </motion.h1>
          <motion.p className="hero-sub" variants={heroChild}>
            Your tracker shouldn&apos;t bill you for hiring. One flat price per
            workspace, billed annually — and local AI agents are included on
            every tier, even Free.
          </motion.p>
        </motion.div>
      </section>

      <section style={{ paddingTop: 0 }}>
        <div className="shell">
          <PlanCards />
          <p className="plan-footnote">
            Annual billing only. Agents on every tier. Upgrade or downgrade
            any time from workspace settings.
          </p>
          <FoundingCallout />
        </div>
      </section>

      <section
        id="compare"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 50%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">Comparison</span>
          <h2 className="section-title">How it compares to Linear.</h2>
          <p className="section-sub">
            Linear is a great tracker. But it bills per seat, runs only in
            their cloud, and its agents run on their machines — not yours.
          </p>
          <ComparisonTable />
        </motion.div>
      </section>

      <FooterCTA
        title="Start free. Stay cheap."
        subtitle="A workspace for you and your agents, free forever. No credit card required."
      />
      <SiteFooter />
    </>
  )
}
