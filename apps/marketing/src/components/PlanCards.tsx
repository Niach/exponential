import { motion } from "motion/react"
import { Check, Mail, Server } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"

type Plan = {
  name: string
  amount: string
  cadence?: string
  note?: string
  tagline: string
  highlight?: boolean
  features: string[]
  cta: { label: string; href: string }
}

/* Cloud tiers — mirrors apps/web/src/lib/billing.ts PLAN_LIMITS and
   apps/web/src/components/workspace/plan-comparison.tsx. Keep in sync. */
const cloudPlans: Plan[] = [
  {
    name: `Free`,
    amount: `$0`,
    cadence: `forever`,
    tagline: `For you and your side projects.`,
    features: [
      `1 seat`,
      `250 MB storage`,
      `All native apps`,
      `Real-time sync`,
      `Push & steer`,
    ],
    cta: { label: `Sign up free`, href: LINKS.app.register },
  },
  {
    name: `Pro`,
    amount: `$5`,
    cadence: `/seat/mo`,
    note: `· billed yearly`,
    tagline: `For teams that ship together.`,
    highlight: true,
    features: [
      `Everything in Free`,
      `5 GB storage`,
      `Feedback widget`,
      `Helpdesk emails`,
    ],
    cta: { label: `Start with Pro`, href: LINKS.app.register },
  },
  {
    name: `Business`,
    amount: `$10`,
    cadence: `/seat/mo`,
    note: `monthly or yearly`,
    tagline: `For orgs with room to grow.`,
    features: [
      `Everything in Pro`,
      `50 GB storage`,
      `Unlimited feedback widgets`,
      `SSO / OIDC — soon`,
      `Priority support`,
    ],
    cta: { label: `Start with Business`, href: LINKS.app.register },
  },
]

/* Run-it-yourself tiers — self-host is free & unlimited; Enterprise is
   contact-sales. Plain, friendly phrasing. */
const selfHostPlans: (Plan & { selfHost?: boolean; enterprise?: boolean })[] = [
  {
    name: `Self-hosted`,
    amount: `Free`,
    cadence: `your hardware`,
    tagline: `Free for individuals and small businesses — under 10 people.`,
    selfHost: true,
    features: [
      `Every feature unlocked`,
      `Unlimited seats & storage`,
      `One docker compose`,
      `Source-available (ELv2)`,
    ],
    cta: { label: `Read self-host docs`, href: `/docs/self-host/` },
  },
  {
    name: `Enterprise`,
    amount: `Let's talk`,
    cadence: `self-hosted, supported`,
    tagline: `For teams of 10 or more running it in-house.`,
    enterprise: true,
    features: [
      `Everything in self-hosted`,
      `Prioritized support`,
      `Deployment & upgrade help`,
    ],
    /* Dedicated contact page with the sales form (EXP-39). */
    cta: { label: `Contact sales`, href: `/contact/` },
  },
]

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

export function PlanCards() {
  return (
    <motion.div
      className="plan-grid plan-grid-cloud"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      {cloudPlans.map((p) => (
        <motion.div
          key={p.name}
          className={`plan-card${p.highlight ? ` is-highlight` : ``}`}
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
      {selfHostPlans.map((p) => (
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
