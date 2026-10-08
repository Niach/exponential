/* Conformance: what it means, the suites (rendered from the generated
   manifest), how a third-party renderer runs them and checks its report,
   our four runners and CI, and round 1's real-font harness (VAPP-98) as the
   second half that arrives with round 1. */
import manifest from "@exponential-at/ui/conformance/manifest.json"
import { DocsCallout, DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import { LEARN_NAV } from "../lib/content"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `meaning`, num: `01`, label: `What it means` },
  { id: `suites`, num: `02`, label: `The suites` },
  { id: `run`, num: `03`, label: `Run it on your renderer` },
  { id: `report`, num: `04`, label: `The report` },
  { id: `runners`, num: `05`, label: `Our runners and CI` },
  { id: `real-font`, num: `06`, label: `The real-font harness` },
  { id: `listing`, num: `07`, label: `A2UI listing` },
]

/** The manifest's prose marks code with backticks; render those as code. */
const Ticks = ({ text }: { text: string }) => (
  <>{text.split(`\``).map((part, i) => (i % 2 ? <code key={i}>{part}</code> : part))}</>
)

const TOTAL = manifest.suites.reduce((n, s) => n + s.cases, 0)
const fixtureLink = (file: string) => LINKS.source(`packages/exponential-ui/${file}`)

const REPORT = `{
  "renderer": "my-renderer",
  "platform": "web",
  "version": "0.3.0",
  "conformanceVersion": ${manifest.version},
  "suites": {
    "catalog": { "cases": ${manifest.suites[0].cases}, "passed": ${manifest.suites[0].cases}, "failed": [] },
    "macros":  { "cases": ${manifest.suites[1].cases}, "passed": ${manifest.suites[1].cases}, "failed": [] },
    …
  }
}`

const RUNNERS = [
  {
    renderer: `exponential-ui (Rust core)`,
    runner: `apps/desktop/crates/exponential-ui/tests/conformance.rs`,
    command: `cargo test -p exponential-ui --test conformance`,
  },
  {
    renderer: `@exponential-at/ui-react`,
    runner: `packages/exponential-ui-react/browser/suites.test.ts`,
    command: `bun run --filter @exponential-at/ui-react test:conformance`,
  },
  {
    renderer: `exponential-ui-gpui`,
    runner: `apps/desktop/crates/exponential-ui-gpui/tests/suites.rs`,
    command: `cargo test -p exponential-ui-gpui --test suites`,
  },
  {
    renderer: `ExponentialUI (SwiftUI)`,
    runner: `packages/exponential-ui-swift/Tests/ExponentialUITests/ConformanceTests.swift`,
    command: `swift test`,
  },
  {
    renderer: `at.exponential:ui-compose`,
    runner: `packages/exponential-ui-compose/ui-compose/src/test`,
    label: `ConformanceTest.kt`,
    command: `./gradlew :ui-compose:testDebugUnitTest`,
  },
]

export default function ConformancePage({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>Conformance</h1>
          <p>
            {manifest.suites.length} suites, {TOTAL.toLocaleString(`en-US`)} cases. Every renderer replays them, ours
            in CI on every change, yours with the same files and the same verdict.
          </p>
        </div>
      </section>

      <DocsLayout nav={LEARN_NAV} title="Exponential UI" sections={SECTIONS} currentPath={path}>
        <DocsSection id="meaning" num="01" label="What it means">
          <h2>What it means</h2>
          <p>
            A renderer is <strong>Exponential UI conformant</strong> when its runner replays every suite of the
            manifest and the report passes: every suite present, every case run, nothing failed or skipped. Then a
            surface means the same thing on it as on ours. The same tree comes out of the reducer, every macro expands
            the same way, a theme resolves to the same colours, a control measures to the same box, and the host
            makes the same policy decisions.
          </p>
          <p>
            The fixtures in <a href={LINKS.source(`packages/exponential-ui/fixtures`)}>fixtures/</a> are the contract.
            The TypeScript reference generated their expected values, and the Rust core replays them byte for byte.
            A renderer built on the Rust core gets the pure suites through its bindings, one built on the TypeScript
            package gets them from <code>@exponential-at/ui</code>, and anything else implements the reducer and{` `}
            <a href={fixtureLink(`catalog/host.json`)}>host.json</a> itself.
          </p>
        </DocsSection>

        <DocsSection id="suites" num="02" label="The suites">
          <h2>The suites</h2>
          <p>
            Generated from the fixtures into{` `}
            <a href={fixtureLink(`conformance/manifest.json`)}>conformance/manifest.json</a> (version {manifest.version}
            ), which drift-gates the case counts. A suite's <em>unit</em> says what one case is.
          </p>
          <DocsTable
            className="conf-suites"
            head={[`Suite`, `Unit`, `Cases`, `A runner checks, per case`]}
            rows={manifest.suites.map((s) => [
              <a key="id" href={fixtureLink(s.files[0])}>
                <code>{s.id}</code>
              </a>,
              s.unit,
              <span key="n" className="conf-num">
                {s.cases}
              </span>,
              <Ticks key="c" text={s.check} />,
            ])}
          />
          <p className="conf-total">
            Total: <strong>{TOTAL.toLocaleString(`en-US`)}</strong> cases in {manifest.suites.length} suites.
          </p>
        </DocsSection>

        <DocsSection id="run" num="03" label="Run it on your renderer">
          <h2>Run it on your renderer</h2>
          <p>A third-party renderer (Flutter, Qt, a terminal UI) becomes conformant in four steps:</p>
          <ol>
            <li>
              <strong>Read the manifest.</strong> It lists each suite's fixture files, the unit and the case count.
              It ships in the npm package as <code>@exponential-at/ui/conformance/manifest.json</code>, next to the
              fixtures.
            </li>
            <li>
              <strong>Replay every case</strong> the way the suite's <em>check</em> says: reduce, expand, resolve,
              measure, lay out, place, paint, decode or decide, and compare with the fixture's expected value.
              Geometry suites take your painter's own measurements (controls within 0.5, a CSS layout within 1 px).
            </li>
            <li>
              <strong>Write the report</strong> (below), one entry per suite, naming every failed case.
            </li>
            <li>
              <strong>Check it.</strong> The checker compares the report with the manifest and prints the verdict:
            </li>
          </ol>
          <DocsCode>{`bun run --filter @exponential-at/ui conformance:check path/to/report.json
# CONFORMANT  my-renderer (flutter 0.3.0): ${TOTAL} cases passed in ${manifest.suites.length} suites`}</DocsCode>
          <p>
            Outside the monorepo, <code>checkReport()</code> from <code>@exponential-at/ui</code> gives the same
            verdict in code. A report with a missing suite, a short case count or a skipped case is not conformant.
          </p>
        </DocsSection>

        <DocsSection id="report" num="04" label="The report">
          <h2>The report</h2>
          <p>
            One JSON object per run, validated by{` `}
            <a href={fixtureLink(`conformance/report.schema.json`)}>report.schema.json</a>:
          </p>
          <DocsCode language="json">{REPORT}</DocsCode>
          <ul>
            <li>
              <code>renderer</code>, <code>platform</code>, <code>version</code>: who ran it, where.
            </li>
            <li>
              <code>conformanceVersion</code>: the manifest's version the runner was written against.
            </li>
            <li>
              <code>suites.&lt;id&gt;</code>: <code>cases</code> run, <code>passed</code>, <code>failed</code> (the
              failed cases' names with a short reason) and, optionally, <code>skipped</code>.
            </li>
          </ul>
          <p>
            Our runners write it to <code>$EXPONENTIAL_UI_CONFORMANCE_REPORT</code> (default{` `}
            <code>.conformance/&lt;name&gt;.json</code> at the repo root).
          </p>
        </DocsSection>

        <DocsSection id="runners" num="05" label="Our runners and CI">
          <h2>Our runners and CI</h2>
          <p>The core and all four first-party renderers run the full suite:</p>
          <DocsTable
            className="conf-runners"
            head={[`Renderer`, `Runner`, `Command`]}
            rows={RUNNERS.map((r) => [
              r.renderer,
              <a key="r" href={LINKS.source(r.runner)}>
                <code>{(r as { label?: string }).label ?? r.runner.split(`/`).pop()}</code>
              </a>,
              <code key="c">{r.command}</code>,
            ])}
          />
          <p>
            CI (<a href={LINKS.source(`.github/workflows/exponential-ui.yml`)}>exponential-ui.yml</a>) runs all of
            them on every SDK pull request and fails unless every report is conformant. A release tag is only cut
            from a commit where that workflow is green.
          </p>
        </DocsSection>

        <DocsSection id="real-font" num="06" label="The real-font harness">
          <h2>The real-font harness</h2>
          <DocsCallout kind="note" title="Arrives with round 1">
            The suites above are on master today. The real-font harness is the second half, and it lands with
            renderer hardening round 1.
          </DocsCallout>
          <p>
            The fixture suites lock the <strong>contract</strong> case by case, with no fonts involved: layout
            replays with a fixed fake measure, so it tests the rules, not the text. The real-font harness locks what
            real text does to them. It renders the kitchen sink and the responsive cases in the React renderer
            (headless Chromium) and in the gpui painter (headless, gpui's own text system), both with the same
            committed font files. Then it compares the two frame by frame against a committed web baseline.
          </p>
          <ul>
            <li>
              <strong>Frame dumps</strong>: every placed node of every case (fixture × theme × mode × width ×
              direction) with its frame, its text and its line count.
            </li>
            <li>
              <strong>The comparison</strong>: frames within a pixel tolerance; a text node may wrap one line more or
              less. Every divergence is reported, and the ones nothing below them explains are marked as origins,
              the list to fix.
            </li>
            <li>
              <strong>A pixel step</strong>: the web surface next to a real gpui window, with a pixel-difference
              ratio and a structural similarity score. It is reported, never gating.
            </li>
          </ul>
          <p>
            Once both halves are on master, the harness's comparison becomes one more suite in the manifest,{` `}
            <code>real-font</code>, so <code>conformance:check</code> stays the one verdict.
          </p>
        </DocsSection>

        <DocsSection id="listing" num="07" label="A2UI listing">
          <h2>A2UI listing</h2>
          <p>
            Once two platforms pass, Exponential UI gets an entry on the A2UI ecosystem's renderers page, as a pull
            request to google/A2UI:
          </p>
          <blockquote className="content-quote">
            <strong>Exponential UI</strong> (React, SwiftUI, Jetpack Compose, gpui): native renderers of the A2UI
            v0.9 basic catalog plus a core catalog, runtime themes, extension catalogs and a host API (transports,
            functions, bindings, policy). Apache-2.0. https://ui.exponential.at
          </blockquote>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
