import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import {
  EnterpriseLine,
  PlanCards,
} from "./components/PlanCards"
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
              Free for teams of three
              <br />
              <em>One plan after that</em>
            </motion.h1>
            <motion.p className="hero-sub" variants={heroChild}>
              Start with your whole team for free. One paid plan with
              everything in it.
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
            <span className="section-eyebrow">Self-hosting</span>
            <h2 className="section-title">
              Open source. Support if you want it
            </h2>
            <p className="section-sub">
              Exponential is fully open source under Apache-2.0 — self-hosting
              is free for everyone, at any size, forever. If you want an SLA,
              priority support, deployment help, or custom development on top,
              Enterprise Support is available as an optional add-on — never a
              requirement. <a href="/contact/">Talk to us</a>.
            </p>
            <p className="plan-footnote">
              📱 Mobile push is cloud-only — the store apps are built against
              the first-party Firebase project. Web and desktop notifications
              work fully on self-hosted.
            </p>
          </motion.div>
        </section>

        <section id="compare">
          <motion.div className="shell" {...sectionReveal}>
            <span className="section-eyebrow">Comparison</span>
            <h2 className="section-title">Exponential vs. Linear</h2>
            <p className="section-sub">
              A great tracker — but it bills for AI agents, runs only in their
              cloud, and can&apos;t be self-hosted.
            </p>
            <ComparisonTable />
          </motion.div>
        </section>

        <FooterCTA
          title="Start free today"
          subtitle="Three seats and your coding agents, free forever. No card required."
        />
      </main>
      <SiteFooter />
    </>
  )
}
