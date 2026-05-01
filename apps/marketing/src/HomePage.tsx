import { ProductBoard } from "./components/ProductBoard"
import {
  CopyBlock,
  FeatureGrid,
  HostTerminal,
  OssCopy,
  RepoCard,
  SectionTag,
} from "./components/Sections"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcDocker, IcGithub } from "./components/icons"

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
            Exponential is a real-time, self-hosted issue tracker. Open source,
            MIT-licensed, and yours end-to-end.
          </p>
          <div className="hero-cta">
            <a className="btn btn-primary" href="#install">
              <IcDocker size={14} /> docker compose up
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
        id="install"
        style={{
          background: `color-mix(in oklch, var(--bg-elev) 60%, var(--bg))`,
          borderTop: `1px solid var(--border)`,
          borderBottom: `1px solid var(--border)`,
        }}
      >
        <div className="shell">
          <SectionTag num="02" label="Self-host" />
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
          <div className="oss-grid">
            <RepoCard />
            <OssCopy />
          </div>
        </div>
      </section>

      <SiteFooter />
    </>
  )
}
