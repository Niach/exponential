import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { EnterpriseLine, LicenseCard, PlanCards } from "./components/PlanCards"
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
            <span className="section-eyebrow">Self-host licensing</span>
            <h2 className="section-title">10 or more people? One invoice.</h2>
            <p className="section-sub">
              Self-hosting is free under the Exponential Small Team License
              while you&apos;re under 10 people — the card above. From 10
              people it takes a commercial license, at published annual
              pricing: no negotiation, no enforcement in the software, just a
              contract.
            </p>
            <LicenseCard />
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
