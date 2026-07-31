import { useId, useState } from "react"
import { motion } from "motion/react"
import { Check, Server, TriangleAlert } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import {
  ENTERPRISE_LINE,
  PLANS,
  type Plan,
} from "../lib/plans"
import { IcArrow } from "./icons"

function FeatureList({
  features,
  caveat,
}: {
  features: string[]
  caveat?: string
}) {
  return (
    <ul className="plan-features">
      {features.map((f) => (
        <li key={f}>
          <Check size={13} strokeWidth={2.4} />
          {f}
        </li>
      ))}
      {caveat && (
        <li className="plan-caveat">
          <TriangleAlert size={13} strokeWidth={2.4} />
          {caveat}
        </li>
      )}
    </ul>
  )
}

function PriceLockup({ plan, amount }: { plan: Plan; amount: string }) {
  return (
    <div className="plan-price">
      <span className="plan-amount">{amount}</span>
      {plan.cadence && <span className="plan-cadence">{plan.cadence}</span>}
      {plan.note && <span className="plan-note">{plan.note}</span>}
    </div>
  )
}

/* Monthly/yearly cadence switch — only the Team card has one (EXP-341).
   Yearly is the default state, so the card still opens on the headline €12
   price and the prerendered HTML matches what hydration produces. */
function CadenceToggle({
  yearly,
  onChange,
}: {
  yearly: boolean
  onChange: (next: boolean) => void
}) {
  const labelId = useId()
  return (
    <div className="plan-toggle">
      <button
        type="button"
        role="switch"
        aria-checked={yearly}
        aria-labelledby={labelId}
        className={`plan-switch${yearly ? ` is-on` : ``}`}
        onClick={() => onChange(!yearly)}
      >
        <span className="plan-switch-knob" />
      </button>
      <span className="plan-toggle-label" id={labelId}>
        Billed yearly
      </span>
    </div>
  )
}

function PlanCard({ plan }: { plan: Plan }) {
  const [yearly, setYearly] = useState(true)
  const amount =
    plan.monthlyAmount && !yearly ? plan.monthlyAmount : plan.amount

  return (
    <motion.div
      className={`plan-card${plan.highlight ? ` is-highlight` : ``}${plan.selfHost ? ` is-selfhost` : ``}`}
      variants={cardReveal}
    >
      {plan.highlight && <span className="plan-flag">Most popular</span>}
      <div className="plan-head">
        <h3>
          {plan.selfHost && (
            <Server size={14} strokeWidth={2} style={{ marginRight: 6 }} />
          )}
          {plan.name}
        </h3>
        <PriceLockup plan={plan} amount={amount} />
        <p className="plan-tagline">{plan.tagline}</p>
      </div>
      {plan.monthlyAmount && (
        <CadenceToggle yearly={yearly} onChange={setYearly} />
      )}
      <FeatureList features={plan.features} caveat={plan.caveat} />
      <a
        className={`btn ${plan.highlight ? `btn-primary` : `btn-ghost`}`}
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
   plan grid instead of a fourth card. Since EXP-352 it sells the optional
   Enterprise Support add-on. */
export function EnterpriseLine() {
  return (
    <p className="plan-enterprise-line">
      {ENTERPRISE_LINE}{` `}
      <a href="/contact/">Talk to us about Enterprise Support</a>.
    </p>
  )
}

