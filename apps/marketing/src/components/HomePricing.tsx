import { motion } from "motion/react"
import {
  cardReveal,
  eyebrowDraw,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"

/* ── Home pricing — the three cloud tiers at a glance ──
   Compact by design: name + price + one line. The full comparison (features,
   Enterprise, self-host tiers) lives on /pricing/; keep amounts in sync with
   PlanCards.tsx and apps/web/src/lib/billing.ts. */
const HOME_PLANS = [
  {
    name: `Free`,
    amount: `$0`,
    cadence: `forever`,
    tagline: `For you and your side projects.`,
  },
  {
    name: `Pro`,
    amount: `$5`,
    cadence: `/seat/mo`,
    tagline: `Adds the helpdesk and more widgets.`,
    highlight: true,
  },
  {
    name: `Business`,
    amount: `$10`,
    cadence: `/seat/mo`,
    tagline: `More storage, unlimited widgets.`,
  },
]

export function HomePricing() {
  return (
    <section id={`pricing`} className={`home-pricing`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Pricing
          </motion.span>
          <h2 className={`section-title`}>Free for individuals.</h2>
          <p className={`section-sub`}>
            Upgrade when your team does. Self-hosting is free.
          </p>
        </motion.div>

        <motion.div
          className={`plan-grid home-plan-grid`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={viewportOnce}
        >
          {HOME_PLANS.map((p) => (
            <motion.div
              key={p.name}
              className={`plan-card${p.highlight ? ` is-highlight` : ``}`}
              variants={cardReveal}
            >
              {p.highlight && <span className={`plan-flag`}>Most popular</span>}
              <div className={`plan-head`}>
                <h3>{p.name}</h3>
                <div className={`plan-price`}>
                  <span className={`plan-amount`}>{p.amount}</span>
                  <span className={`plan-cadence`}>{p.cadence}</span>
                </div>
                <p className={`plan-tagline`}>{p.tagline}</p>
              </div>
            </motion.div>
          ))}
        </motion.div>

        <div className={`home-pricing-links`}>
          <a className={`btn btn-primary`} href={LINKS.app.register}>
            Get started free <IcArrow size={12} />
          </a>
          <a className={`btn btn-ghost`} href={`/pricing/`}>
            Compare all plans
          </a>
        </div>
      </div>
    </section>
  )
}
