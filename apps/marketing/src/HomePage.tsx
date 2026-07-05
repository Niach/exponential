import { motion } from "motion/react"
import { Radio } from "lucide-react"
import { DownloadSection } from "./components/DownloadSection"
import { IdeShowcase } from "./components/IdeShowcase"
import { LoopSection } from "./components/LoopSection"
import { ProductBoard } from "./components/ProductBoard"
import { ProductMobile } from "./components/ProductMobile"
import { AgentTimeline } from "./components/Sections"
import { FooterCTA, SiteFooter, SiteHeader } from "./components/SiteShell"
import { WidgetEmbed } from "./components/WidgetEmbed"
import { IcArrow, IcBot, IcGithub, IcZap } from "./components/icons"
import { heroChild, heroStagger, sectionReveal } from "./lib/animations"
import { LINKS } from "./lib/links"

export function HomePage() {
  return (
    <>
      <WidgetEmbed />
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
            Make your app exponential.
          </motion.h1>
          <motion.p className="hero-sub" variants={heroChild}>
            Feedback becomes an issue, Claude writes the fix, the PR ships, and
            the reporter gets the email &mdash; a real-time tracker that closes
            the loop, native on every device and running on your machine.
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

      {/* ── The loop (centerpiece) ───────────── */}
      <LoopSection />

      {/* ── Desktop IDE showcase ─────────────── */}
      <IdeShowcase />

      {/* ── Start coding ─────────────────────── */}
      <section id="agents">
        <div className="shell">
          <div className="agents-grid">
            <motion.div className="agents-copy" {...sectionReveal}>
              <span className="section-eyebrow">Start coding</span>
              <h2 className="section-title">
                Click Start coding. Get a PR.
              </h2>
              <p className="section-sub">
                Hit <strong>Start coding</strong> on any issue in the desktop
                IDE and Claude opens in an embedded terminal, in a dedicated
                git worktree &mdash; plan-first and fully interactive. It
                commits, pushes an <code>exp/&lt;ID&gt;</code> branch, and opens
                the pull request itself.
              </p>
              <ul className="mobile-bullets">
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Plan-first, never on autopilot.</strong>
                    <p>
                      Claude proposes a plan, then codes with permissions
                      bypassed &mdash; no accept prompts to babysit. Watch the
                      live terminal and type to steer it any time.
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
                      The <code>claude</code> CLI you already pay for runs on
                      your machine, in a real terminal. One issue = one PR = one
                      worktree. Nothing leaves your desk.
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
                    <Radio size={14} strokeWidth={1.8} />
                  </span>
                  <div>
                    <strong>Steer a session from your phone.</strong>
                    <p>
                      A coding session running on your desktop streams to your
                      pocket &mdash; watch the terminal and type a nudge from
                      anywhere.
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

      {/* ── Downloads ────────────────────────── */}
      <section id="download">
        <div className="shell">
          <motion.div {...sectionReveal}>
            <span className="section-eyebrow">Get the apps</span>
            <h2 className="section-title">One instance, every device.</h2>
            <p className="section-sub">
              The desktop IDE for macOS and Linux, native apps for iOS and
              Android &mdash; all syncing the same real-time data.
            </p>
          </motion.div>
          <DownloadSection />
        </div>
      </section>

      <FooterCTA />
      <SiteFooter />
    </>
  )
}
