import { motion } from "motion/react"
import { AtSign, Check, Copy, GitPullRequest, Inbox, Link2, UserPlus } from "lucide-react"
import { eyebrowDraw, sectionReveal, staggerContainer, cardReveal } from "../lib/animations"
import { INBOX_ITEMS } from "../ide/data"

/* ── Teamwork — inbox, mentions & refs, reviews, invites ──
   A bento of static site-CSS mocks; the inbox rows reuse the demo fixture
   universe so the sentences match the IDE/mobile/docs demos. */
export function TeamworkSection() {
  return (
    <section id={`teamwork`} className={`home-teamwork`}>
      <div className={`shell`}>
        <motion.div {...sectionReveal}>
          <motion.span className={`section-eyebrow`} {...eyebrowDraw}>
            Teamwork
          </motion.span>
          <h2 className={`section-title`}>Your whole team in the loop.</h2>
          <p className={`section-sub`}>
            Shared boards, an inbox that only holds what concerns you, and
            mentions that pull the right person in at the right line.
          </p>
        </motion.div>

        <motion.div
          className={`tw-grid`}
          variants={staggerContainer}
          initial={`hidden`}
          whileInView={`visible`}
          viewport={{ once: true, amount: 0.2 }}
        >
          <motion.div className={`tw-card tw-inbox`} variants={cardReveal}>
            <div className={`tw-card-head`}>
              <Inbox size={14} />
              <span className={`tw-card-title`}>Inbox</span>
            </div>
            <div className={`tw-inbox-rows`} aria-hidden>
              {INBOX_ITEMS.slice(0, 3).map((item) => (
                <div key={item.id} className={`tw-inbox-row`}>
                  <span
                    className={`tw-inbox-dot${item.unread ? ` is-unread` : ``}`}
                  />
                  <span className={`tw-inbox-sentence`}>{item.sentence}</span>
                  <span className={`tw-inbox-time`}>{item.time}</span>
                </div>
              ))}
            </div>
            <p className={`tw-card-caption`}>
              Assignments, replies, mentions, merges &mdash; one list, marked
              read everywhere at once.
            </p>
          </motion.div>

          <motion.div className={`tw-card tw-mentions`} variants={cardReveal}>
            <div className={`tw-card-head`}>
              <AtSign size={14} />
              <span className={`tw-card-title`}>Mentions &amp; refs</span>
            </div>
            <div className={`tw-composer`} aria-hidden>
              <p>
                <span className={`tw-pill tw-pill-person`}>@robin</span> this
                regressed in <span className={`tw-pill tw-pill-issue`}>#EXP-142</span>{` `}
                &mdash; can you take the Safari path?
              </p>
            </div>
            <p className={`tw-card-caption`}>
              @ pulls someone in and subscribes them &mdash; # links the work,
              on every client.
            </p>
          </motion.div>

          <motion.div className={`tw-card tw-reviews`} variants={cardReveal}>
            <div className={`tw-card-head`}>
              <GitPullRequest size={14} />
              <span className={`tw-card-title`}>Reviews</span>
            </div>
            <div className={`tw-review-row`} aria-hidden>
              <GitPullRequest size={13} className={`tw-review-icon`} />
              <span className={`tw-review-ref`}>
                #214 &middot; exp/EXP-8
              </span>
              <span className={`tw-review-stats`}>
                <em>+24</em> <s>&minus;6</s>
              </span>
              <span className={`tw-review-merge`}>
                <Check size={11} /> Merge
              </span>
            </div>
            <p className={`tw-card-caption`}>
              Every open pull request in one list &mdash; review the diff and
              merge without leaving the tracker.
            </p>
          </motion.div>

          <motion.div className={`tw-card tw-invite`} variants={cardReveal}>
            <div className={`tw-card-head`}>
              <UserPlus size={14} />
              <span className={`tw-card-title`}>Invites</span>
            </div>
            <div className={`tw-invite-row`} aria-hidden>
              <Link2 size={12} />
              <span className={`tw-invite-link`}>
                app.exponential.at/invite/x7Kd&hellip;
              </span>
              <Copy size={12} className={`tw-invite-copy`} />
            </div>
            <div className={`tw-avatars`} aria-hidden>
              <span className={`tw-avatar`}>DS</span>
              <span className={`tw-avatar`}>RC</span>
              <span className={`tw-avatar`}>JL</span>
              <span className={`tw-avatar is-more`}>+2</span>
            </div>
            <p className={`tw-card-caption`}>
              One link brings the team in &mdash; everyone can triage, comment,
              and close.
            </p>
          </motion.div>
        </motion.div>
      </div>
    </section>
  )
}
