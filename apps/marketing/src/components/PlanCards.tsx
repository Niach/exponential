import { motion } from "motion/react"
import { Check, Mail, Server } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import {
  COMMERCIAL_LICENSE,
  ENTERPRISE_LINE,
  PLANS,
  type Plan,
} from "../lib/plans"
import { IcArrow } from "./icons"

function FeatureList({ features }: { features: string[] }) {
  return (
    <ul className="plan-features">
      {features.map((f) => (
        <li key={f}>
          <Check size={13} strokeWidth={2.4} />
          {f}
        </li>
      ))}
    </ul>
  )
}

function PriceLockup({ plan }: { plan: Plan }) {
  return (
    <div className="plan-price">
      <span className="plan-amount">{plan.amount}</span>
      {plan.cadence && <span className="plan-cadence">{plan.cadence}</span>}
      {plan.note && <span className="plan-note">{plan.note}</span>}
    </div>
  )
}

function PlanCard({ plan }: { plan: Plan }) {
  return (
    <motion.div
      className={`plan-card${plan.highlight ? ` is-highlight` : ``}${plan.selfHost ? ` is-selfhost` : ``}${plan.enterprise ? ` is-enterprise` : ``}`}
      variants={cardReveal}
    >
      {plan.highlight && <span className="plan-flag">Most popular</span>}
      <div className="plan-head">
        <h3>
          {plan.selfHost && (
            <Server size={14} strokeWidth={2} style={{ marginRight: 6 }} />
          )}
          {plan.enterprise && (
            <Mail size={14} strokeWidth={2} style={{ marginRight: 6 }} />
          )}
          {plan.name}
        </h3>
        <PriceLockup plan={plan} />
        <p className="plan-tagline">{plan.tagline}</p>
      </div>
      <FeatureList features={plan.features} />
      <a
        className={`btn ${plan.highlight || plan.enterprise ? `btn-primary` : `btn-ghost`}`}
        href={plan.cta.href}
        style={{ justifyContent: `center` }}
      >
        {plan.cta.label} <IcArrow size={12} />
      </a>
    </motion.div>
  )
}

/* The main grid — Free · Team · Self-hosted (EXP-338) — rendered on BOTH the
   home pricing section and /pricing/. */
export function PlanCards() {
  return (
    <motion.div
      className="plan-grid plan-grid-cloud"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      {PLANS.map((p) => (
        <PlanCard key={p.name} plan={p} />
      ))}
    </motion.div>
  )
}

/* Enterprise is a sales motion, not a tier (EXP-286) — one line under the
   plan grid instead of a fourth card. */
export function EnterpriseLine() {
  return (
    <p className="plan-enterprise-line">
      {ENTERPRISE_LINE} <a href="/contact/">Talk to us</a>.
    </p>
  )
}

/* Self-host licensing (EXP-338: the free self-host card lives in the main
   grid; what remains here is the 10+-people commercial license). */
export function LicenseCard() {
  return (
    <motion.div
      className="plan-grid plan-grid-selfhost"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      <PlanCard plan={COMMERCIAL_LICENSE} />
    </motion.div>
  )
}
