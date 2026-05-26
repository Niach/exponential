import { motion } from "motion/react"
import { ProductBoard } from "./components/ProductBoard"
import { ProductMobile } from "./components/ProductMobile"
import {
  AgentTimeline,
  CopyBlock,
  RepoCard,
  ValueProps,
} from "./components/Sections"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcBot, IcGithub, IcZap } from "./components/icons"
import {
  heroChild,
  heroStagger,
  sectionReveal,
} from "./lib/animations"

export function HomePage() {
  return (
    <>
      <SiteHeader />

      {/* ── Hero ─────────────────────────────── */}
      <section className="hero">
        <div className="hero-grid" />
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
            The issue tracker for teams that ship. Real-time sync, native on
            every device, AI agents that open PRs. Free to use &mdash; or
            self-host on your own infrastructure.
          </motion.p>
          <motion.div className="hero-cta" variants={heroChild}>
            <a
              className="btn btn-primary"
              href="https://app.exponential.at/auth/register"
            >
              Sign up free <IcArrow size={12} />
            </a>
            <a className="btn btn-ghost" href="/docs/self-host/">
              Self-host
            </a>
            <a
              className="btn btn-ghost"
              href="https://github.com/Niach/exponential"
            >
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

      {/* ── Value Props ──────────────────────── */}
      <section id="features">
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">Why Exponential</span>
          <h2 className="section-title">Everything you need, nothing you don&apos;t.</h2>
          <p className="section-sub">
            Built for speed, real-time from the ground up, and designed to get
            out of your way.
          </p>
          <ValueProps />
        </motion.div>
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
                SwiftUI on iOS and Compose on Android, both shipping today.
                Same instance, same data, same auth &mdash; live across every
                device.
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
                    <strong>Offline-first.</strong>
                    <p>
                      Local databases on every device. Edit on the subway, sync
                      when you&apos;re back.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/docs/#mobile-apps">
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
                PR. A push notification lands on your phone the moment
                the plan is ready.
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
                      changes, or cancel &mdash; the agent picks up your signal
                      and resumes.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Works with Claude Code, Codex, or any MCP client.</strong>
                    <p>
                      Point any MCP-aware tool at <code>/api/mcp</code> and let
                      it drive issues, labels, and comments.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/docs/#ai-agents">
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

      {/* ── Self-host + OSS ──────────────────── */}
      <section
        id="self-host"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 60%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">Open source</span>
          <h2 className="section-title">Self-host it, or don&apos;t.</h2>
          <p className="section-sub">
            The whole stack ships as Docker Compose &mdash; everything you need
            in one command. Or skip setup and use the free cloud.
          </p>
          <div className="selfhost-grid">
            <div className="selfhost-copy">
              <CopyBlock />
              <div style={{ marginTop: 20, display: `flex`, gap: 10, flexWrap: `wrap` }}>
                <a className="btn btn-primary" href="/docs/self-host/">
                  Read the docs <IcArrow size={12} />
                </a>
                <a
                  className="btn btn-ghost"
                  href="https://github.com/Niach/exponential"
                >
                  <IcGithub size={14} /> View source
                </a>
              </div>
            </div>
            <RepoCard />
          </div>
        </motion.div>
      </section>

      <FooterCTA />
      <SiteFooter />
    </>
  )
}
