import { motion } from "motion/react"
import { eyebrowDraw, sectionReveal } from "../lib/animations"
import { EVERY_PLAN_INCLUDES } from "../lib/plans"
import { LINKS } from "../lib/links"
import { IcArrow } from "./icons"
import { EnterpriseLine, PlanCards } from "./PlanCards"

/* ── Home pricing — Free + Team ──
   The SAME full feature cards as /pricing/ (shared PlanCards over the
   canonical lib/plans.ts); the self-host tiers + comparison table stay
   on /pricing/. */
export function HomePricing() {
  return (
    <section id={`pricing`} className={`home-pricing`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Pricing
          </motion.span>
          <h2 className={`section-title`}>Free for teams of three.</h2>
          <p className={`section-sub`}>
            One plan when you grow. Self-hosting is free under 10 people.
          </p>
        </motion.div>

        <PlanCards />
        <EnterpriseLine />

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
