import { motion } from "motion/react"
import { Check, Mail, Server } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"

type Plan = {
  name: string
  price: string
  cadence: string
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
    price: `$0`,
    cadence: `forever`,
    tagline: `For you and your side projects.`,
    features: [
      `1 seat (just you)`,
      `Unlimited projects, repos & coding sessions`,
      `250 MB storage`,
      `Push & steer — free`,
      `All native apps`,
    ],
    cta: { label: `Sign up free`, href: LINKS.app.register },
  },
  {
    name: `Pro`,
    price: `$5`,
    cadence: `per seat / month`,
    note: `billed yearly`,
    tagline: `For teams that ship together.`,
    highlight: true,
    features: [
      `Buy exactly the seats you need`,
      `Unlimited projects, repos & coding sessions`,
      `5 GB storage`,
      `Feedback widget + helpdesk emails`,
      `Push & steer — free`,
    ],
    cta: { label: `Start with Pro`, href: LINKS.app.register },
  },
  {
    name: `Business`,
    price: `$10`,
    cadence: `per seat / month`,
    note: `monthly or yearly`,
    tagline: `For orgs that need room to grow.`,
    features: [
      `Everything in Pro`,
      `50 GB storage`,
      `Unlimited feedback widgets`,
      `SSO / OIDC — coming soon`,
      `Priority support`,
    ],
    cta: { label: `Start with Business`, href: LINKS.app.register },
  },
]

/* Run-it-yourself tiers — self-host is free & unlimited; Enterprise is
   contact-sales with honor-system ">10 employees" language (no enforcement). */
const selfHostPlans: (Plan & { selfHost?: boolean; enterprise?: boolean })[] = [
  {
    name: `Self-hosted`,
    price: `Free`,
    cadence: `your infrastructure`,
    tagline: `Everything unlimited, on your hardware.`,
    selfHost: true,
    features: [
      `Unlimited seats, projects & storage`,
      `Every feature unlocked`,
      `One docker compose`,
      `Elastic License 2.0`,
    ],
    cta: { label: `Read self-host docs`, href: `/docs/self-host/` },
  },
  {
    name: `Enterprise`,
    price: `Let's talk`,
    cadence: `self-hosted, supported`,
    tagline: `For companies of more than 10 running Exponential in-house.`,
    enterprise: true,
    features: [
      `Everything in self-hosted`,
      `Extended, prioritized support`,
      `Deployment & upgrade guidance`,
      `Honor-system — pay if you're >10 people`,
    ],
    cta: {
      label: `Contact sales`,
      href: `mailto:sales@exponential.at?subject=Exponential%20Enterprise`,
    },
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
            <div className="plan-price">
              <span className="plan-amount">{p.price}</span>
              <span className="plan-cadence">{p.cadence}</span>
            </div>
            {p.note && <span className="plan-note">{p.note}</span>}
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
            <div className="plan-price">
              <span className="plan-amount">{p.price}</span>
              <span className="plan-cadence">{p.cadence}</span>
            </div>
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
