/* exponential.at/ui: generative UI in one screen. The live demo (HomeDemo:
   the app's phone chat answering with a surface generated node by node,
   beside the same surface's native shots) sits in the hero; the rest is the
   pitch, the six renderers, the entry points and the packages. The header's
   "eUI" link opens this page (NEW-15). */
import manifest from "@exponential_app/ui/conformance/manifest.json"
import promptBudget from "@exponential_app/ui/fixtures/prompt-budget.json"
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
    body: `A flat list of components, a data model and the actions it wants back. The catalog prompt teaches all ${COMPONENT_COUNT} components in ~${Math.round(promptBudget.full.tokens / 100) / 10}k tokens.`,
  },
  {
    num: `02`,
    title: `Your host routes it`,
    body: `Messages in over JSONL, SSE, WebSocket or MCP; data sources bound; only the functions your policy allows run; actions go back.`,
  },
  {
    num: `03`,
    title: `A renderer paints it natively`,
    body: `React, Solid, web components, SwiftUI, Compose or gpui, with the platform's own views, in your theme, from one shared layout.`,
  },
]

const PITCH = [
  {
    title: `One catalog`,
    body: `${COMPONENT_COUNT} components, layout to charts, with shadcn's names and variants. A2UI basic surfaces map onto it unchanged.`,
    href: `/ui/components/`,
  },
  {
    title: `Native everywhere`,
    body: `No webviews, no downloaded code. One Rust core lays out iOS, Android and desktop; the web matches its frames.`,
    href: `/ui/concepts/#renderers`,
  },
  {
    title: `Themable at runtime`,
    body: `One JSON file of tokens and recipes, loaded at runtime by every renderer.`,
    href: `/ui/themes/`,
  },
  {
    title: `Extensible`,
    body: `New components as an extension catalog: a painter per platform, or a macro with none.`,
    href: guidePath(`extensions`),
  },
  {
    title: `Conformance-tested`,
    body: `${manifest.suites.length} suites, ${TOTAL_CASES.toLocaleString(`en-US`)} cases, replayed by every renderer in CI, third-party ones too.`,
    href: `/ui/conformance/`,
  },
  {
    title: `Open source`,
    body: `Apache-2.0: catalog, themes, host API and all six renderers.`,
    href: LINKS.repo,
  },
]

export default function HomePage(_: PageProps) {
  return (
    <>
      <section className="home-hero">
        <div className="shell">
          <div className="home-hero-text">
            <h1 className="home-title">
              Generative UI,
              <br />
              <em>native on every platform.</em>
            </h1>
            <p className="home-sub">An agent sends A2UI JSON. eUI (Exponential UI) paints it natively on web, iOS, Android and desktop, in your theme.</p>
            <div className="home-cta">
              <a className="btn btn-primary" href={guidePath(`react`)}>
                Get started <IcArrow />
              </a>
              <a className="btn btn-ghost" href="/ui/concepts/">
                How it works
              </a>
              <a className="btn btn-ghost" href="/ui/playground/">
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
          <div className="section-eyebrow">Six renderers</div>
          <h2 className="section-title">One surface, six native toolkits.</h2>
          <div className="home-renderers">
            {RENDERERS.map((r) => (
              <a key={r.pkg} className="home-renderer glass-card" href={guidePath(r.guide)}>
                <span className="home-renderer-platform">{r.platform}</span>
                <span className="home-renderer-framework">{r.framework}</span>
                <code className="home-renderer-pkg">{r.pkg}</code>
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
          <h2 className="section-title">Why eUI</h2>
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
          <h2 className="section-title">Start here</h2>
          <div className="home-entry">
            <CardGrid
              items={[
                { href: `/ui/components/`, title: `Components`, desc: `Every component, live, with shots from all four platforms.` },
                { href: `/ui/guides/`, title: `Guides`, desc: `Per platform, themes, extensions, agents. Every example compiles in CI.` },
                { href: `/ui/playground/`, title: `Playground`, desc: `Paste A2UI, render it in any theme, read the catalog prompt.` },
                { href: `/ui/themes/`, title: `Themes`, desc: `The built-in themes and a builder that exports JSON.` },
              ]}
            />
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="shell home-packages">
          <div>
            <h2 className="section-title">Packages</h2>
          </div>
          <DocsTable
            className="home-packages-table"
            head={[`Package`, `Registry`, `What`, `Install`]}
            rows={PACKAGES.map((p) => [<code key="n">{p.name}</code>, p.registry, p.what, <code key="i">{p.install}</code>])}
          />
        </div>
      </section>

      <section className="home-section home-dogfood">
        <div className="shell">
          <div className="home-dogfood-card glass-card">
            <div>
              <h2 className="section-title">Exponential runs on it</h2>
              <p>
                <a className="content-link" href={LINKS.marketing}>
                  Exponential
                </a>
                {` `}is a host like any other: its issue rows, session rows and chips are the first extension catalog. Its team-made apps are template packages that the web app and the desktop IDE both render.
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
