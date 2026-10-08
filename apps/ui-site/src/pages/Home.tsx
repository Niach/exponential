/* ui.exponential.at: generative UI in one screen. The live demo (the SDK
   lane's HomeDemo: a prompt, an A2UI replay in the React renderer and the
   same surface's native shots) sits in the hero; the rest is the pitch, the
   four renderers, the entry points and the packages. */
import manifest from "@exponential-at/ui/conformance/manifest.json"
import { DocsCallout } from "@exp/site-shell"
import { CardGrid, DocsTable, IcArrow } from "../components/Content"
import { COMPONENT_COUNT } from "../lib/catalog-facts"
import { PACKAGES, RENDERERS } from "../lib/content"
import { guidePath } from "../lib/guides"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"
import { HomeDemo } from "../sdk/HomeDemo"

const TOTAL_CASES = manifest.suites.reduce((n, s) => n + s.cases, 0)

const STEPS = [
  {
    num: `01`,
    title: `The agent writes A2UI`,
    body: `A model answers with a surface instead of a wall of text: a flat list of components, a data model and the actions it wants back. The catalog prompt teaches it every component in a few thousand tokens.`,
  },
  {
    num: `02`,
    title: `Your host routes it`,
    body: `The host takes messages in over JSONL, SSE, WebSocket or MCP, binds data sources, runs the functions your policy allows and sends the user's actions back.`,
  },
  {
    num: `03`,
    title: `A renderer paints it natively`,
    body: `React on the web, SwiftUI on iOS and macOS, Compose on Android, gpui on the desktop. Each one uses the platform's own views, in your theme, from one shared layout.`,
  },
]

const PITCH = [
  {
    title: `One catalog`,
    body: `${COMPONENT_COUNT} components from layout to charts, with shadcn's names and variants. Surfaces written for the A2UI basic catalog are mapped onto it and render unchanged.`,
    href: `/components/`,
  },
  {
    title: `Native everywhere`,
    body: `No webviews and no downloaded code. One Rust core lays out iOS, Android and the desktop, and the web follows the same frames to the pixel.`,
    href: `/concepts/#renderers`,
  },
  {
    title: `Themable at runtime`,
    body: `A theme is one JSON file of tokens and component recipes. Every renderer loads it at runtime, so a vapp or a server can ship its own look.`,
    href: `/themes/`,
  },
  {
    title: `Extensible`,
    body: `Add components with an extension catalog and a painter per platform, or as a macro over core components with no painter at all.`,
    href: guidePath(`extensions`),
  },
  {
    title: `Conformance-tested`,
    body: `${manifest.suites.length} suites and ${TOTAL_CASES.toLocaleString(`en-US`)} cases every renderer replays in CI. A third-party renderer runs the same suite and checks the same report.`,
    href: `/conformance/`,
  },
  {
    title: `Open source`,
    body: `Apache-2.0, built in the open in the Exponential monorepo. The catalog, the themes, the host API and all four renderers.`,
    href: LINKS.repo,
  },
]

export default function HomePage(_: PageProps) {
  return (
    <>
      <section className="home-hero">
        <div className="shell">
          <div className="home-hero-text">
            <div className="section-eyebrow">Open source · A2UI v0.9 · Apache-2.0</div>
            <h1 className="home-title">
              Generative UI,
              <br />
              <em>native on every platform.</em>
            </h1>
            <p className="home-sub">
              An agent sends a surface as A2UI. Exponential UI paints it natively on the web, iOS, Android and the
              desktop, from one catalog, in your theme.
            </p>
            <div className="home-cta">
              <a className="btn btn-primary" href={guidePath(`react`)}>
                Get started <IcArrow />
              </a>
              <a className="btn btn-ghost" href="/concepts/">
                How it works
              </a>
              <a className="btn btn-ghost" href="/playground/">
                Playground
              </a>
            </div>
          </div>
          <div className="home-demo">
            <HomeDemo />
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">What generative UI is</div>
          <h2 className="section-title">The model decides what to show. The platform decides how it looks.</h2>
          <p className="section-sub">
            <a className="content-link" href={LINKS.a2ui}>
              A2UI
            </a>{` `}
            is the open protocol for it: the agent describes UI as data, and a client renders it with its own
            components. It never sends code, so the user sees native controls and you keep the look and the
            security policy.
          </p>
          <ol className="home-steps">
            {STEPS.map((s) => (
              <li key={s.num} className="home-step glass-card">
                <span className="home-step-num">{s.num}</span>
                <h3>{s.title}</h3>
                <p>{s.body}</p>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Four renderers</div>
          <h2 className="section-title">The same surface, painted by each platform's own toolkit.</h2>
          <div className="home-renderers">
            {RENDERERS.map((r) => (
              <a key={r.pkg} className="home-renderer glass-card" href={guidePath(r.guide)}>
                <span className="home-renderer-platform">{r.platform}</span>
                <span className="home-renderer-framework">{r.framework}</span>
                <code className="home-renderer-pkg">{r.pkg}</code>
                <span className="home-renderer-note">{r.note}</span>
                <span className="home-renderer-link">
                  The {r.framework} guide <IcArrow size={11} />
                </span>
              </a>
            ))}
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Why Exponential UI</div>
          <h2 className="section-title">Built for agents that ship to real apps.</h2>
          <div className="home-pitch">
            {PITCH.map((p) => (
              <a key={p.title} className="home-pitch-item" href={p.href}>
                <h3>{p.title}</h3>
                <p>{p.body}</p>
              </a>
            ))}
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell">
          <div className="section-eyebrow">Start here</div>
          <h2 className="section-title">Pick a way in.</h2>
          <div className="home-entry">
            <CardGrid
              items={[
                { href: `/components/`, title: `Components`, desc: `Every component of the core catalog, rendered live and photographed on all four platforms.` },
                { href: `/guides/`, title: `Guides`, desc: `Render A2UI on your platform, write a theme or an extension, connect an agent. Every example compiles in CI.` },
                { href: `/playground/`, title: `Playground`, desc: `Paste A2UI JSON and watch it render in any theme, then read the prompt the catalog gives a model.` },
                { href: `/themes/`, title: `Themes`, desc: `The built-in themes side by side, and a builder that exports your own as JSON.` },
              ]}
            />
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell home-packages">
          <div>
            <div className="section-eyebrow">Packages</div>
            <h2 className="section-title">One version, every registry.</h2>
            <p className="section-sub">
              One <code>ui-v*</code> tag publishes every package at the same version. We are still claiming the
              registry names, so <strong>nothing is published yet</strong>: until the first release, install from
              the repository as each guide shows.
            </p>
          </div>
          <DocsTable
            className="home-packages-table"
            head={[`Package`, `Registry`, `What`, `Until it is published`]}
            rows={PACKAGES.map((p) => [<code key="n">{p.name}</code>, p.registry, p.what, p.fromRepo])}
          />
          <DocsCallout kind="note" title="Release status">
            The release workflow builds, verifies and stages every artifact today. Each registry step goes live once
            its account is set up: the <code>@exponential-at</code> npm org, the crate name, the{` `}
            <code>Niach/exponential-ui-swift</code> distribution repo and the <code>at.exponential</code> Maven
            namespace. See the{` `}
            <a href={LINKS.source(`packages/exponential-ui/release/README.md`)}>release runbook</a>.
          </DocsCallout>
        </div>
      </section>

      <section className="home-section home-dogfood">
        <div className="shell">
          <div className="home-dogfood-card glass-card">
            <div>
              <div className="section-eyebrow">Built on it</div>
              <h2 className="section-title">Exponential itself runs on Exponential UI.</h2>
              <p>
                <a className="content-link" href={LINKS.marketing}>
                  Exponential
                </a>
                , the issue tracker for teams and their coding agents, is a host like any other. Its issue rows,
                run rows and chips are the first extension catalog, painted by the app's own components on every
                client. Its Devices screen ships as a declarative template package that the web app and the
                desktop IDE both render.
              </p>
            </div>
            <div className="home-dogfood-cta">
              <a className="btn btn-ghost" href={LINKS.marketing}>
                exponential.at <IcArrow />
              </a>
              <a className="btn btn-ghost" href={LINKS.repo}>
                Source on GitHub <IcArrow />
              </a>
            </div>
          </div>
        </div>
      </section>
    </>
  )
}
