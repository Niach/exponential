/* ─── Social proof slot (EXP-337) ───
   Testimonial/tweet cards for later — TESTIMONIALS ships EMPTY and the
   section renders null until the first entry lands (SSR-safe: null on both
   sides). Add entries here to light it up; styles live in actions.css
   (.sp-*). */
import { motion } from "motion/react"
import {
  cardReveal,
  eyebrowDraw,
  sectionReveal,
  staggerContainer,
  viewportOnce,
} from "../lib/animations"

export type Testimonial = {
  quote: string
  author: string
  handle?: string // e.g. "@name" — rendered muted after the author
  href?: string // source link (tweet, post)
}

export const TESTIMONIALS: Testimonial[] = []

export function SocialProofSection() {
  if (TESTIMONIALS.length === 0) return null
  return (
    <section id={`loved`} className={`home-social`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Loved by teams
          </motion.span>
        </motion.div>
        <motion.div
          className={`sp-grid`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={viewportOnce}
        >
          {TESTIMONIALS.map((t) => (
            <motion.blockquote
              key={t.quote}
              className={`glass-card sp-card`}
              variants={cardReveal}
            >
              <p className={`sp-quote`}>{t.quote}</p>
              <footer className={`sp-author`}>
                {t.href ? <a href={t.href}>{t.author}</a> : t.author}
                {t.handle ? (
                  <span className={`sp-handle`}>{t.handle}</span>
                ) : null}
              </footer>
            </motion.blockquote>
          ))}
        </motion.div>
      </div>
    </section>
  )
}
