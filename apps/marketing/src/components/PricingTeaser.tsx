import { motion } from "motion/react"
import { sectionReveal } from "../lib/animations"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"

export function PricingTeaser() {
  return (
    <motion.div className="pricing-teaser" {...sectionReveal}>
      <div className="pricing-teaser-tiers">
        <span className="tier-chip">
          <strong>Free</strong> <span className="tier-price">$0</span>
        </span>
        <span className="tier-chip is-highlight">
          <strong>Pro</strong> <span className="tier-price">$18/yr</span>
        </span>
        <span className="tier-chip">
          <strong>Business</strong> <span className="tier-price">$60/yr</span>
        </span>
        <span className="tier-chip">
          <strong>Self-hosted</strong> <span className="tier-price">free · unlimited</span>
        </span>
      </div>
      <p className="pricing-teaser-line">
        Flat <strong>per workspace, per year</strong> — not per seat. Agents on
        every tier.
      </p>
      <div style={{ display: `flex`, gap: 10, flexWrap: `wrap`, justifyContent: `center` }}>
        <a className="btn btn-primary" href="/pricing/">
          See pricing <IcArrow size={12} />
        </a>
        <a className="btn btn-ghost" href={LINKS.app.register}>
          Sign up free
        </a>
      </div>
    </motion.div>
  )
}
