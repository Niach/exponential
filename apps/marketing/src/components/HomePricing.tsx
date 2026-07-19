import { motion } from "motion/react"
import {
  cardReveal,
  eyebrowDraw,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"
import { CLOUD_PLANS, EVERY_PLAN_INCLUDES } from "../lib/plans"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"

/* ── Home pricing — the four cloud tiers at a glance ──
   Compact by design: name + price + one line, mapped from the canonical
   lib/plans.ts (the full feature cards + self-host tiers live on
   /pricing/). */
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
          className={`plan-grid plan-grid-cloud`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={viewportOnce}
        >
          {CLOUD_PLANS.map((p) => (
            <motion.div
              key={p.id}
              className={`plan-card${p.highlight ? ` is-highlight` : ``}${p.enterprise ? ` is-enterprise` : ``}`}
              variants={cardReveal}
            >
              {p.highlight && <span className={`plan-flag`}>Most popular</span>}
              <div className={`plan-head`}>
                <h3>{p.name}</h3>
                <div className={`plan-price`}>
                  <span className={`plan-amount`}>{p.amount}</span>
                  {p.cadence && (
                    <span className={`plan-cadence`}>{p.cadence}</span>
                  )}
                </div>
                <p className={`plan-tagline`}>{p.homeTagline}</p>
              </div>
            </motion.div>
          ))}
        </motion.div>

        <p className={`plan-footnote`}>{EVERY_PLAN_INCLUDES}</p>

        <div className={`home-pricing-links`}>
          <a className={`btn btn-primary`} href={LINKS.app.login}>
            Get started free <IcArrow size={12} />
          </a>
          <a className={`btn btn-ghost`} href={`/pricing/`}>
            Compare all plans
          </a>
          <a className={`btn btn-ghost`} href={`/docs/self-host/`}>
            Self-host
          </a>
        </div>
      </div>
    </section>
  )
}
