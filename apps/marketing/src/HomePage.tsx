import { useState } from "react"
import { motion } from "motion/react"
import { ProductBoard } from "./components/ProductBoard"
import { ProductMobile } from "./components/ProductMobile"
import {
  AgentTimeline,
  CopyBlock,
  RepoCard,
  ValueProps,
} from "./components/Sections"
import { TerminalDemo, type AgentPhase } from "./components/TerminalDemo"
import { HeroSidecar } from "./components/HeroSidecar"
import { DownloadSection } from "./components/DownloadSection"
import { PricingTeaser } from "./components/PricingTeaser"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow, IcBot, IcGithub, IcZap } from "./components/icons"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"

const CLIENTS = [`web`, `ios`, `android`, `macos`, `linux`]

export function HomePage() {
  const [phase, setPhase] = useState<AgentPhase>(`idle`)

  return (
    <>
      <SiteHeader />

      {/* ── Hero: the live agent terminal ────── */}
      <section className="hero">
        <div className="hero-atmos" aria-hidden />
        <motion.div
          className="shell hero-content"
          variants={heroStagger}
          initial="hidden"
          animate="visible"
        >
          <motion.span className="hero-kicker" variants={heroChild}>
            exponential — issue tracking for teams that ship
          </motion.span>
          <motion.h1 className="hero-title" variants={heroChild}>
            Assign the issue. Approve the plan.
            <br />
            The agent ships — on{` `}
            <span className="mono-accent">your machine.</span>
            <span className="caret" aria-hidden />
          </motion.h1>
          <motion.p className="hero-sub" variants={heroChild}>
            A real-time issue tracker, native on web, iOS, Android, macOS and
            Linux. The desktop apps run Claude or Codex locally in an embedded
            terminal — with your subscription, your repo, your rules — and open
            real GitHub PRs.
          </motion.p>
          <motion.div className="hero-cta" variants={heroChild}>
            <a className="btn btn-primary" href={LINKS.app.register}>
              Sign up free <IcArrow size={12} />
            </a>
            <a className="btn btn-ghost" href="/#download">
              Download apps
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
          transition={{ duration: 0.6, ease: `easeOut`, delay: 0.4 }}
        >
          <div className="command-deck">
            <TerminalDemo onPhase={setPhase} />
            <HeroSidecar phase={phase} />
          </div>
          <div className="client-strip" aria-label="Available on web, iOS, Android, macOS and Linux">
            {CLIENTS.map((c, i) => (
              <span key={c} style={{ display: `contents` }}>
                {i > 0 && <span className="sep">·</span>}
                <span className="client">{c.toUpperCase()}</span>
              </span>
            ))}
            <span className="client-note">
              all native · all real-time · one tracker
            </span>
          </div>
        </motion.div>
      </section>

      {/* ── The tracker ──────────────────────── */}
      <section id="product">
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">the-tracker</span>
          <h2 className="section-title">
            First, a tracker that&apos;s actually fast.
          </h2>
          <p className="section-sub">
            Keyboard-first, five-state workflow, fractional ordering, live
            presence. Every edit lands on every screen instantly — powered by
            ElectricSQL, not polling.
          </p>
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
      <section
        id="agents"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 50%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <div className="shell">
          <div className="agents-grid">
            <motion.div className="agents-copy" {...sectionReveal}>
              <span className="section-eyebrow">agents --local</span>
              <h2 className="section-title">
                Not a cloud agent. <em>Your</em> agent.
              </h2>
              <p className="section-sub">
                Install the desktop app and your machine becomes an agent in
                the workspace. Assign it an issue: the agent plans in the
                comments, waits for your approval, then codes in a real
                terminal you can watch and steer — and opens a GitHub PR.
              </p>
              <ul className="mobile-bullets">
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Your subscription, your hardware.</strong>
                    <p>
                      The agent runs the claude or codex CLI you already pay
                      for, against the repo on your disk. Nothing is delegated
                      to someone else&apos;s cloud.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Visible, steerable, embedded in ghostty.</strong>
                    <p>
                      The CLI session runs in an embedded ghostty terminal
                      inside the app. Watch every keystroke, take over any
                      time.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Approve from any device.</strong>
                    <p>
                      Plan-ready pushes to your phone. Approve, request
                      changes, or cancel from the web, iOS, Android — anywhere.
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
      <section id="mobile">
        <div className="shell">
          <div className="mobile-grid">
            <motion.div className="mobile-copy" {...sectionReveal}>
              <span className="section-eyebrow">mobile --native</span>
              <h2 className="section-title">Your tracker, in your pocket.</h2>
              <p className="section-sub">
                SwiftUI on iOS and Jetpack Compose on Android — real native
                apps, not webviews. Same instance, same data, live across
                every device.
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
                      Review and approve from the train.
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
                      Local databases on every device. Edit on the subway,
                      sync when you&apos;re back.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/#download">
                  Get the apps <IcArrow size={12} />
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

      {/* ── Everything else ──────────────────── */}
      <section
        id="features"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 50%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">everything-else</span>
          <h2 className="section-title">
            Everything you need, nothing you don&apos;t.
          </h2>
          <p className="section-sub">
            Built for speed, real-time from the ground up, and designed to get
            out of your way.
          </p>
          <ValueProps />
        </motion.div>
      </section>

      {/* ── Download ─────────────────────────── */}
      <section id="download">
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">download</span>
          <h2 className="section-title">Get it on every screen.</h2>
          <p className="section-sub">
            Web needs no install — <a href={LINKS.app.register} style={{ textDecoration: `underline`, textUnderlineOffset: 3 }}>sign up</a> and
            you&apos;re in. The native apps bring push notifications, offline
            support, and on desktop: the local agent runtime.
          </p>
          <DownloadSection />
        </motion.div>
      </section>

      {/* ── Self-host ────────────────────────── */}
      <section
        id="self-host"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 60%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">self-host</span>
          <h2 className="section-title">Self-host it, or don&apos;t.</h2>
          <p className="section-sub">
            The whole stack — Postgres 17, ElectricSQL, Garage and Caddy —
            ships as one Docker Compose. Set{` `}
            <code style={{ fontFamily: `var(--font-mono)`, fontSize: `0.9em`, color: `var(--phosphor)` }}>SELF_HOSTED=true</code>
            {` `}and every limit disappears. Or skip setup entirely and use
            the cloud.
          </p>
          <div className="selfhost-grid">
            <div className="selfhost-copy">
              <CopyBlock />
              <div style={{ marginTop: 20, display: `flex`, gap: 10, flexWrap: `wrap` }}>
                <a className="btn btn-primary" href="/docs/self-host/">
                  Read the docs <IcArrow size={12} />
                </a>
                <a className="btn btn-ghost" href={LINKS.github.repo}>
                  <IcGithub size={14} /> View source
                </a>
              </div>
            </div>
            <RepoCard />
          </div>
        </motion.div>
      </section>

      {/* ── Pricing teaser ───────────────────── */}
      <section id="pricing">
        <motion.div className="shell" {...sectionReveal}>
          <span className="section-eyebrow">pricing</span>
          <h2 className="section-title">Priced like a tool, not a tax.</h2>
          <p className="section-sub">
            Per-seat pricing punishes you for growing. Exponential charges per
            workspace, per year.
          </p>
          <PricingTeaser />
        </motion.div>
      </section>

      <FooterCTA />
      <SiteFooter />
    </>
  )
}
