/* ─── Home FAQ (MKT-9, MKT-4) ───
   Native <details> rows: the answers sit in the prerendered HTML whether a
   row is open or not, and no JS is needed to toggle them. The SAME array
   (lib/faq.ts) feeds the FAQPage JSON-LD in seo.ts. */
import { motion } from "motion/react"
import { sectionReveal } from "../lib/animations"
import { HOME_FAQ } from "../lib/faq"

export function HomeFaq() {
  return (
    <section className={`faq-section`} id={`faq`}>
      <div className={`shell faq-inner`}>
        <motion.div className={`faq-head`} {...sectionReveal}>
          <div className={`section-eyebrow`}>FAQ</div>
          <h2 className={`section-title`}>Questions, answered.</h2>
        </motion.div>
        <motion.div className={`faq-list`} {...sectionReveal}>
          {HOME_FAQ.map((item, i) => (
            <details key={item.q} className={`faq-item`} open={i === 0}>
              <summary>
                <span>{item.q}</span>
                <span className={`faq-toggle`} aria-hidden />
              </summary>
              <p>{item.a}</p>
            </details>
          ))}
        </motion.div>
      </div>
    </section>
  )
}
