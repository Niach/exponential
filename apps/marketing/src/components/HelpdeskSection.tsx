import { motion } from "motion/react"
import { Send } from "lucide-react"
import { eyebrowDraw, sectionReveal, staggerContainer, cardReveal } from "../lib/animations"
import { WidgetPreview } from "../loop/WidgetPreview"

/* ── Helpdesk — widget report → email conversation → support inbox ──
   Static site-CSS mock of the real support thread (apps/web support inbox):
   an inbound reporter email, an internal note that never leaves the team,
   and an outbound reply that lands in the reporter's inbox. */
export function HelpdeskSection() {
  return (
    <section id={`helpdesk`} className={`home-helpdesk`}>
      <div className={`shell`}>
        <div className={`hd-grid`}>
          <motion.div className={`hd-copy`} {...sectionReveal}>
            <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
              Helpdesk
            </motion.span>
            <h2 className={`section-title`}>Feedback becomes a conversation.</h2>
            <p className={`section-sub`}>
              A widget report doesn&rsquo;t have to be a dead drop. With the
              helpdesk enabled, every report opens an email conversation pinned
              to the issue &mdash; answered from a support inbox your whole
              team shares.
            </p>
            <ul className={`hd-points`}>
              <li>
                Reporters reply from their inbox or a magic link &mdash; no
                account needed.
              </li>
              <li>Your answers send as email, straight from the issue.</li>
              <li>Internal notes stay internal, next to the thread.</li>
              <li>
                Ship the fix, close the thread &mdash; the transcript link
                keeps working.
              </li>
            </ul>
            <span className={`hd-pro`}>
              <span className={`hd-pro-badge`}>Pro</span> Included in the Pro
              plan
            </span>
          </motion.div>

          <motion.div
            className={`hd-visual`}
            variants={staggerContainer}
            initial={`hidden`}
            whileInView={`visible`}
            viewport={{ once: true, amount: 0.25 }}
            aria-hidden
          >
            <motion.div className={`hd-widget`} variants={cardReveal}>
              <WidgetPreview caption={false} />
            </motion.div>
            <motion.div className={`hd-connector`} variants={cardReveal}>
              <span className={`hd-connector-line`} />
              <span className={`hd-connector-label`}>
                opens a conversation
              </span>
              <span className={`hd-connector-line`} />
            </motion.div>
            <motion.div className={`hd-thread`} variants={cardReveal}>
              <div className={`hd-thread-head`}>
                <span className={`hd-avatar`}>J</span>
                <span className={`hd-thread-meta`}>
                  <span className={`hd-thread-from`}>jamie@acme.shop</span>
                  <span className={`hd-thread-subject`}>
                    Checkout button does nothing
                  </span>
                </span>
                <span className={`hd-pill-open`}>Open</span>
              </div>
              <motion.div className={`hd-msg is-in`} variants={cardReveal}>
                <p>
                  Still broken for me on Safari &mdash; I tried &ldquo;Pay
                  now&rdquo; twice and nothing happens.
                </p>
                <span className={`hd-msg-meta`}>jamie@acme.shop &middot; 2h</span>
              </motion.div>
              <motion.div className={`hd-note`} variants={cardReveal}>
                <span className={`hd-note-label`}>
                  Internal note &mdash; never sent to the reporter
                </span>
                <p>Repro&rsquo;d on 17.5. Fix is EXP-151 &mdash; merging today.</p>
              </motion.div>
              <motion.div className={`hd-msg is-out`} variants={cardReveal}>
                <p>Fixed and live &mdash; thanks for the report!</p>
                <span className={`hd-msg-meta`}>
                  Danny &middot; emailed to jamie@acme.shop
                </span>
              </motion.div>
              <div className={`hd-composer`}>
                <span className={`hd-composer-input`}>
                  Reply to jamie@acme.shop&hellip;
                </span>
                <span className={`hd-composer-send`}>
                  <Send size={12} />
                </span>
              </div>
            </motion.div>
          </motion.div>
        </div>
      </div>
    </section>
  )
}
