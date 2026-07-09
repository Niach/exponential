import { motion } from "motion/react"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { PlanCards, SelfHostCards } from "./components/PlanCards"
import { ContactForm } from "./components/ContactForm"
import { ComparisonTable } from "./components/ComparisonTable"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"

export function PricingPage() {
  return (
    <>
      <SiteHeader />

      <section className="hero pricing-hero">
        <motion.div
          className="shell hero-content"
          variants={heroStagger}
          initial="hidden"
          animate="visible"
        >
          <motion.h1 className="hero-title" variants={heroChild}>
            Free for individuals.
            <br />
            <em>Affordable for teams.</em>
          </motion.h1>
          <motion.p className="hero-sub" variants={heroChild}>
            Start solo for free. Bring your team when you&apos;re ready — without
            breaking the bank.
          </motion.p>
        </motion.div>
      </section>

      <section style={{ paddingTop: 0 }}>
        <div className="shell">
          <PlanCards />
          <p className="plan-footnote">
            Unlimited projects, repos and coding sessions on every tier. Agents
            are free everywhere — you only ever pay for people.
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
          <h2 className="section-title">Self-host it, free.</h2>
          <p className="section-sub">
            Every feature, no caps, on your own hardware under the Elastic
            License 2.0.
          </p>
          <SelfHostCards />
          <div id="enterprise-contact" className="contact-block">
            <h3>Talk to us about Enterprise</h3>
            <p className="contact-block-sub">
              Tell us about your team and how you plan to run Exponential —
              we&apos;ll get back to you within a business day.
            </p>
            <ContactForm />
          </div>
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
        subtitle="A workspace for you and your coding agents, free forever. No card required."
      />
      <SiteFooter />
    </>
  )
}
