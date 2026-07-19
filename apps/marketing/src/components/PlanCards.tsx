import { motion } from "motion/react"
import { Check, Mail, Server } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import { CLOUD_PLANS, SELF_HOST_PLANS, type CloudPlan } from "../lib/plans"
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

function PriceLockup({ plan }: { plan: CloudPlan }) {
  return (
    <div className="plan-price">
      <span className="plan-amount">{plan.amount}</span>
      {plan.cadence && <span className="plan-cadence">{plan.cadence}</span>}
      {plan.note && <span className="plan-note">{plan.note}</span>}
    </div>
  )
}

export function PlanCards() {
  return (
    <motion.div
      className="plan-grid plan-grid-cloud"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      {CLOUD_PLANS.map((p) => (
        <motion.div
          key={p.name}
          className={`plan-card${p.highlight ? ` is-highlight` : ``}${p.enterprise ? ` is-enterprise` : ``}`}
          variants={cardReveal}
        >
          {p.highlight && <span className="plan-flag">Most popular</span>}
          <div className="plan-head">
            <h3>{p.name}</h3>
            <PriceLockup plan={p} />
            <p className="plan-tagline">{p.tagline}</p>
          </div>
          <FeatureList features={p.features} />
          <a
            className={`btn ${p.highlight ? `btn-primary` : `btn-ghost`}`}
            href={p.cta.href}
            style={{ justifyContent: `center` }}
          >
            {p.cta.label} <IcArrow size={12} />
          </a>
        </motion.div>
      ))}
    </motion.div>
  )
}

export function SelfHostCards() {
  return (
    <motion.div
      className="plan-grid plan-grid-selfhost"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      {SELF_HOST_PLANS.map((p) => (
        <motion.div
          key={p.name}
          className={`plan-card${p.selfHost ? ` is-selfhost` : ``}${p.enterprise ? ` is-enterprise` : ``}`}
          variants={cardReveal}
        >
          <div className="plan-head">
            <h3>
              {p.selfHost ? (
                <Server size={14} strokeWidth={2} style={{ marginRight: 6 }} />
              ) : (
                <Mail size={14} strokeWidth={2} style={{ marginRight: 6 }} />
              )}
              {p.name}
            </h3>
            <PriceLockup plan={p} />
            <p className="plan-tagline">{p.tagline}</p>
          </div>
          <FeatureList features={p.features} />
          <a
            className={`btn ${p.enterprise ? `btn-primary` : `btn-ghost`}`}
            href={p.cta.href}
            style={{ justifyContent: `center` }}
          >
            {p.cta.label} <IcArrow size={12} />
          </a>
        </motion.div>
      ))}
    </motion.div>
  )
}
