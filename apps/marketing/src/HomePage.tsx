import { ProductBoard } from "./components/ProductBoard"
import { ProductMobile } from "./components/ProductMobile"
import {
  AgentTimeline,
  CopyBlock,
  FeatureGrid,
  HostTerminal,
  RepoCard,
  SectionTag,
} from "./components/Sections"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import {
  IcArrow,
  IcBot,
  IcDocker,
  IcGithub,
  IcServer,
  IcShield,
  IcZap,
} from "./components/icons"

export function HomePage() {
  return (
    <>
      <SiteHeader />

      <section className="hero">
        <div className="hero-grid" />
        <div className="shell hero-content fade-in">
          <h1 className="hero-title">
            Issue tracking that runs
            <br />
            <em>on your own machines.</em>
          </h1>
          <p className="hero-sub">
            Exponential is a real-time issue tracker with native iOS and
            Android apps and coding agents that ship to your repo. Self-host
            the stack, or skip setup and use the free cloud instance. Open
            source, MIT-licensed, yours end-to-end.
          </p>
          <div className="hero-cta">
            <a className="btn btn-primary" href="#install">
              <IcDocker size={14} /> docker compose up
            </a>
            <a
              className="btn btn-ghost"
              href="https://app.exponential.at/auth/login"
            >
              Use the free cloud <IcArrow size={12} />
            </a>
            <a
              className="btn btn-ghost"
              href="https://github.com/Niach/exponential"
            >
              <IcGithub size={14} /> View source
            </a>
          </div>
        </div>

        <div className="shell">
          <div className="preview-wrap">
            <div className="window">
              <div className="window-bar">
                <div className="window-dots">
                  <span />
                  <span />
                  <span />
                </div>
                <div className="window-url">
                  <span className="url-host">exp.your-team.dev</span>
                </div>
              </div>
              <ProductBoard animate />
            </div>
          </div>
        </div>
      </section>

      <section id="features">
        <div className="shell">
          <SectionTag num="01" label="Built for self-hosting" />
          <h2 className="section-title">A small surface, sharpened.</h2>
          <p className="section-sub">
            Just the primitives a software team actually uses, wired together
            with care.
          </p>
          <FeatureGrid />
        </div>
      </section>

      <section
        id="mobile"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 50%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <div className="shell">
          <SectionTag num="02" label="Native mobile" />
          <div className="mobile-grid">
            <div className="mobile-copy">
              <h2 className="section-title">Your tracker, in your pocket.</h2>
              <p className="section-sub">
                SwiftUI on iOS and Compose on Android, both shipping today.
                Same instance, same data, same auth — live across every device,
                with push delivered through your own relay.
              </p>
              <ul className="mobile-bullets">
                <li>
                  <span className="mobile-bullet-icon">
                    <IcZap size={14} />
                  </span>
                  <div>
                    <strong>Real-time, everywhere.</strong>
                    <p>
                      Edits made on the web ripple through every phone in the
                      room. No pull-to-refresh.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcServer size={14} />
                  </span>
                  <div>
                    <strong>Multi-server. One tap to switch.</strong>
                    <p>
                      Sign into your cloud workspace and three self-hosted
                      instances. Add, switch, and remove servers from Settings
                      — sessions stay logged in, swap is instant.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcShield size={14} />
                  </span>
                  <div>
                    <strong>Offline-first.</strong>
                    <p>
                      iOS uses GRDB, Android uses Room. Your most recent
                      workspace data lives on the device — open the app on the
                      subway, edit issues, sync when you're back.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/docs/#mobile">
                  Read the docs <IcArrow size={12} />
                </a>
                <a
                  className="btn btn-ghost"
                  href="https://github.com/Niach/exponential/tree/master/apps/ios"
                >
                  <IcGithub size={14} /> iOS source
                </a>
                <a
                  className="btn btn-ghost"
                  href="https://github.com/Niach/exponential/tree/master/apps/android"
                >
                  <IcGithub size={14} /> Android source
                </a>
              </div>
            </div>
            <div className="mobile-stage">
              <div className="mobile-stage-glow" aria-hidden />
              <ProductMobile animate />
            </div>
          </div>
        </div>
      </section>

      <section id="agents">
        <div className="shell">
          <SectionTag num="03" label="Agents" />
          <div className="agents-grid">
            <div className="agents-copy">
              <h2 className="section-title">
                Coding agents that run where your repo does.
              </h2>
              <p className="section-sub">
                Assign an issue to your agent. The companion daemon — one
                binary, Linux or macOS, your machine — runs Claude or Codex in
                a local git worktree, drafts a plan in the comments, waits for
                approval, then opens a real GitHub PR. A push notification
                lands on your phone the moment the plan is ready.
              </p>
              <ul className="mobile-bullets">
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>Plan, then code.</strong>
                    <p>
                      Agents post their plan as a special comment. Approve,
                      request changes, or retry — the daemon picks up the
                      latest signal and resumes from there.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcGithub size={14} />
                  </span>
                  <div>
                    <strong>No GitHub App needed.</strong>
                    <p>
                      The daemon authenticates with your local <code>gh</code>{` `}
                      token. Repos link to projects in the UI; PRs show up
                      under the agent's GitHub identity.
                    </p>
                  </div>
                </li>
                <li>
                  <span className="mobile-bullet-icon">
                    <IcBot size={14} />
                  </span>
                  <div>
                    <strong>MCP for the rest.</strong>
                    <p>
                      Point Claude Code or Cursor at <code>/api/mcp</code> to
                      drive Exponential as a tool — list, create, comment,
                      label.
                    </p>
                  </div>
                </li>
              </ul>
              <div className="mobile-cta">
                <a className="btn btn-primary" href="/docs/#agents">
                  Read the docs <IcArrow size={12} />
                </a>
                <a
                  className="btn btn-ghost"
                  href="https://github.com/Niach/exponential/tree/master/apps/companion"
                >
                  <IcGithub size={14} /> Companion source
                </a>
              </div>
            </div>
            <div className="agents-stage">
              <AgentTimeline />
            </div>
          </div>
        </div>
      </section>

      <section
        id="install"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 60%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <div className="shell">
          <SectionTag num="04" label="Self-host" />
          <div className="host-grid">
            <div>
              <h2 className="section-title">Up and running in minutes.</h2>
              <p className="section-sub">
                The whole stack ships as a docker-compose file. Configure your{` `}
                <code
                  style={{
                    fontFamily: `var(--font-mono)`,
                    fontSize: `0.85em`,
                    padding: `2px 8px`,
                    background: `var(--bg-soft)`,
                    borderRadius: 6,
                  }}
                >
                  .env
                </code>
                , bring up four containers, run migrations. Email/password auth
                works out of the box; add OIDC by setting four variables and
                you're done.
              </p>
              <div style={{ marginTop: 24 }}>
                <CopyBlock />
              </div>
              <div
                style={{
                  marginTop: 28,
                  display: `flex`,
                  gap: 10,
                  flexWrap: `wrap`,
                }}
              >
                <a
                  className="btn btn-primary"
                  href="https://github.com/Niach/exponential"
                >
                  <IcGithub size={14} /> Get the source
                </a>
              </div>
            </div>
            <HostTerminal />
          </div>
        </div>
      </section>

      <section id="open-source">
        <div className="shell">
          <SectionTag num="05" label="Open source" />
          <div className="oss-solo">
            <RepoCard />
          </div>
        </div>
      </section>

      <SiteFooter />
    </>
  )
}
