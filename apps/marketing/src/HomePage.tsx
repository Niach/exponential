import { motion } from "motion/react"
import { ProductBoard } from "./components/ProductBoard"
import { ProductMobile } from "./components/ProductMobile"
import { AgentTimeline } from "./components/Sections"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcBot, IcGithub, IcZap } from "./components/icons"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"

export function HomePage() {
  return (
    <>
      <SiteHeader />

      {/* ── Hero ─────────────────────────────── */}
      <section className="hero" id="product">
        <div className="hero-atmos" aria-hidden />
        <motion.div
          className="shell hero-content"
          variants={heroStagger}
          initial="hidden"
          animate="visible"
        >
          <motion.h1 className="hero-title" variants={heroChild}>
            Go Exponential.
          </motion.h1>
          <motion.p className="hero-sub" variants={heroChild}>
            The issue tracker for teams that ship. Real-time, native on every
            device, AI agents that open PRs &mdash; running on your machine.
          </motion.p>
          <motion.div className="hero-cta" variants={heroChild}>
            <a className="btn btn-primary" href={LINKS.app.register}>
              Sign up free <IcArrow size={12} />
            </a>
            <a className="btn btn-ghost" href={LINKS.github.repo}>
              <IcGithub size={14} /> View source
            </a>
          </motion.div>
        </motion.div>

        <motion.div
          className="shell"
          initial={{ opacity: 0, y: 24 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, ease: `easeOut`, delay: 0.35 }}
        >
          <div className="preview-wrap">
            <div className="window">
              <div className="window-bar">
                <div className="window-dots">
                  <span />
                  <span />
                  <span />
                </div>
                <div className="window-url">
                  <span className="url-host">app.exponential.at</span>
                </div>
              </div>
              <ProductBoard animate />
            </div>
          </div>
        </motion.div>
      </section>

      {/* ── Agents ───────────────────────────── */}
      <section id="agents">
        <div className="shell">
          <div className="agents-grid">
            <motion.div className="agents-copy" {...sectionReveal}>
              <span className="section-eyebrow">AI Agents</span>
              <h2 className="section-title">
                Agents that join your workspace.
              </h2>
              <p className="section-sub">
                Assign an issue to Claude or Codex. The agent plans in the
                comments, waits for your approval, then opens a real GitHub
                PR &mdash; running locally on your machine, in a terminal you
                can watch.
              </p>
              <ul className="mobile-bullets">
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Plan, then code.</strong>
                    <p>
                      Agents post their plan as a comment. Approve, request
                      changes, or cancel &mdash; from any device.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Your hardware, your subscription.</strong>
                    <p>
                      The desktop app runs the claude or codex CLI you already
                      pay for, in an embedded terminal. Nothing leaves your
                      machine.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/docs/#agents">
                  Read the docs <IcArrow size={12} />
                </a>
              </div>
            </motion.div>
            <motion.div className="agents-stage" {...sectionReveal}>
              <AgentTimeline />
            </motion.div>
          </div>
        </div>
      </section>

      {/* ── Mobile ───────────────────────────── */}
      <section
        id="mobile"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 50%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <div className="shell">
          <div className="mobile-grid">
            <motion.div className="mobile-copy" {...sectionReveal}>
              <span className="section-eyebrow">Native mobile</span>
              <h2 className="section-title">Your tracker, in your pocket.</h2>
              <p className="section-sub">
                SwiftUI on iOS and Compose on Android. Same instance, same
                data, live across every device.
              </p>
              <ul className="mobile-bullets">
                <li>
                  <span className="mobile-bullet-icon">
                    <IcZap size={14} />
                  </span>
                  <div>
                    <strong>Real-time, everywhere.</strong>
                    <p>
                      Edits made on the web ripple through every phone. No
                      pull-to-refresh.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcZap size={14} />
                  </span>
                  <div>
                    <strong>Approve agent plans on the go.</strong>
                    <p>
                      A push lands the moment your agent&apos;s plan is ready.
                      Review it from anywhere.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/docs/#mobile">
                  Read the docs <IcArrow size={12} />
                </a>
              </div>
            </motion.div>
            <div className="mobile-stage">
              <div className="mobile-stage-glow" aria-hidden />
              <ProductMobile animate />
            </div>
          </div>
        </div>
      </section>

      <FooterCTA />
      <SiteFooter />
    </>
  )
}
