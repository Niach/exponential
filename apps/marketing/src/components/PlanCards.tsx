import { useState } from "react"
import { motion } from "motion/react"
import { Check, Server } from "lucide-react"
import { cardReveal, staggerContainer, viewportOnce } from "../lib/animations"
import { FOUNDING_CODE } from "../lib/pricing"
import { LINKS } from "../lib/links"
import { IcArrow, IcCopy } from "./icons"

const plans = [
  {
    name: `Free`,
    price: `$0`,
    cadence: `forever`,
    tagline: `For you and your side projects.`,
    features: [
      `1 member per workspace`,
      `3 projects`,
      `50 MB attachments`,
      `1 owned workspace`,
      `Local AI agents included`,
      `All five native apps`,
    ],
    cta: { label: `Sign up free`, href: LINKS.app.register },
  },
  {
    name: `Pro`,
    price: `$18`,
    cadence: `per workspace / year`,
    tagline: `For small teams that ship.`,
    highlight: true,
    features: [
      `5 members per workspace`,
      `10 projects`,
      `1 GB attachments`,
      `3 owned workspaces`,
      `Push notifications`,
      `Local AI agents included`,
    ],
    cta: { label: `Start with Pro`, href: LINKS.app.register },
  },
  {
    name: `Business`,
    price: `$60`,
    cadence: `per workspace / year`,
    tagline: `For teams that outgrew Pro.`,
    features: [
      `25 members per workspace`,
      `Unlimited projects`,
      `10 GB attachments`,
      `10 owned workspaces`,
      `Push notifications`,
      `Local AI agents included`,
    ],
    cta: { label: `Start with Business`, href: LINKS.app.register },
  },
  {
    name: `Self-hosted`,
    price: `Free`,
    cadence: `your infrastructure`,
    tagline: `Everything unlimited, on your hardware.`,
    selfHost: true,
    features: [
      `Unlimited members`,
      `Unlimited projects`,
      `Unlimited storage`,
      `Unlimited workspaces`,
      `One docker compose`,
      `Elastic License 2.0`,
    ],
    cta: { label: `Read self-host docs`, href: `/docs/self-host/` },
  },
]

export function PlanCards() {
  return (
    <motion.div
      className="plan-grid"
      variants={staggerContainer}
      initial="hidden"
      whileInView="visible"
      viewport={viewportOnce}
    >
      {plans.map((p) => (
        <motion.div
          key={p.name}
          className={`plan-card${p.highlight ? ` is-highlight` : ``}${`selfHost` in p && p.selfHost ? ` is-selfhost` : ``}`}
          variants={cardReveal}
        >
          {p.highlight && <span className="plan-flag">Most popular</span>}
          <div className="plan-head">
            <h3>
              {`selfHost` in p && p.selfHost && (
                <Server size={14} strokeWidth={2} style={{ marginRight: 6 }} />
              )}
              {p.name}
            </h3>
            <div className="plan-price">
              <span className="plan-amount">{p.price}</span>
              <span className="plan-cadence">{p.cadence}</span>
            </div>
            <p className="plan-tagline">{p.tagline}</p>
          </div>
          <ul className="plan-features">
            {p.features.map((f) => (
              <li key={f}>
                <Check size={13} strokeWidth={2.4} />
                {f}
              </li>
            ))}
          </ul>
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

export function FoundingCallout() {
  const [copied, setCopied] = useState(false)
  const onCopy = () => {
    navigator.clipboard?.writeText(FOUNDING_CODE)
    setCopied(true)
    setTimeout(() => setCopied(false), 1400)
  }
  return (
    <div className="founding-wrap">
      <div className="founding-callout">
        <code>
          <span className="fc-prompt">$ </span>checkout --code {FOUNDING_CODE}
          {`  `}
          <span className="fc-comment"># 50% off, forever</span>
        </code>
        <button className="copy-btn" onClick={onCopy}>
          <IcCopy size={12} /> {copied ? `Copied` : `Copy`}
        </button>
      </div>
      <p className="founding-note">
        Founding-member discount: use code <strong>{FOUNDING_CODE}</strong> at
        checkout and keep 50% off your plan for as long as you stay
        subscribed.
      </p>
    </div>
  )
}
