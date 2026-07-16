import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"

const SECTIONS: DocsSectionType[] = [
  { id: `install`, num: `01`, label: `Install` },
  { id: `js-api`, num: `02`, label: `JS API` },
  { id: `screenshots`, num: `03`, label: `Screenshots & annotation` },
  { id: `what-lands`, num: `04`, label: `What lands in Exponential` },
  { id: `try-it`, num: `05`, label: `Try it` },
]

const WIDGET_SNIPPET =
  `<script>
  (function (w, d, u) {
    if (w.ExponentialWidget) return;
    var q = [], api = { q: q };
    ["init","identify","setCustomData","open","close"].forEach(function (m) {
      api[m] = function () { q.push([m, [].slice.call(arguments)]); };
    });
    w.ExponentialWidget = api;
    var s = d.createElement("script");
    s.async = true; s.src = u;
    d.head.appendChild(s);
  })(window, document, "https://app.exponential.at/widget/v1/loader.js");
  ExponentialWidget.init({ key: "expw_YOUR_KEY" });
</` + `script>`

export function WidgetDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Feedback widget</h1>
            <p>
              A feedback button for any website — visitors report bugs with an
              annotated screenshot, and each report lands as an issue on your
              board.
            </p>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/widget/">
          {/* ── 01 Install ── */}
          <DocsSection id="install" num="01" label="Install">
            <h2>Install</h2>
            <p>
              Create a widget in <strong>Team settings → Widget</strong>{` `}
              (team owners only; Pro plan). Each config gets a public{` `}
              <code>expw_</code> key, a <strong>domain allowlist</strong> —
              submissions are only accepted from pages on domains you list —
              and a target feedback project where reports land.
            </p>
            <p>
              Then paste the snippet before <code>&lt;/head&gt;</code> on your
              site. It&apos;s the GA-style async pattern: a tiny queue stub
              loads the real script lazily, so it never blocks your page.
            </p>
            <DocsCode language="html">{WIDGET_SNIPPET}</DocsCode>
            <p>
              That&apos;s the whole install — a floating feedback button
              appears, and calls made before the script loads are queued and
              replayed.
            </p>
            <DocsCallout kind="note" title="The key is public by design">
              <code>expw_</code> keys ship in page source, like an analytics
              ID. The domain allowlist plus server-side rate limits are what
              gate submissions — never any secret in the page.
            </DocsCallout>
          </DocsSection>

          {/* ── 02 JS API ── */}
          <DocsSection id="js-api" num="02" label="JS API">
            <h2>JS API</h2>
            <p>
              The snippet exposes <code>window.ExponentialWidget</code> with
              five calls:
            </p>
            <DocsCode language="js">{`
// Required once — boots the widget with your public key.
ExponentialWidget.init({ key: "expw_YOUR_KEY" });

// Attach your signed-in user, so reports arrive with a
// real reporter (and helpdesk replies reach their inbox).
ExponentialWidget.identify({
  email: "ada@example.com",
  name: "Ada Lovelace",
  userId: "usr_123",
});

// Arbitrary context stamped onto every submission —
// plan, build, feature flags, tenant…
ExponentialWidget.setCustomData({
  plan: "business",
  version: "1.42.0",
});

// Open / close the panel programmatically — e.g. wire
// "Report a bug" in your own menu to open().
ExponentialWidget.open();
ExponentialWidget.close();
`}</DocsCode>
            <p>
              All calls are safe to make before the script has loaded — the
              loader queues and replays them in order.
            </p>
          </DocsSection>

          {/* ── 03 Screenshots & annotation ── */}
          <DocsSection
            id="screenshots"
            num="03"
            label="Screenshots & annotation"
          >
            <h2>Screenshots &amp; annotation</h2>
            <p>
              Screenshots are captured <strong>client-side, in the
              browser</strong> — the visitor&apos;s viewport is rendered
              locally and nothing is fetched by a server-side browser, so
              what&apos;s on their screen (including logged-in state) is what
              you see.
            </p>
            <p>
              Before submitting, the visitor can <strong>annotate</strong> the
              screenshot in a full-screen editor: rectangles, arrows, and
              freehand lines, with undo. Annotations stay editable until
              submit, then are flattened into the final image — your issue
              gets one plain screenshot with the markings baked in.
            </p>
            <DocsCallout kind="tip" title="Capture never blocks a report">
              If capture fails on an exotic page, the report still submits —
              just without the image. A lost screenshot is never a lost bug
              report.
            </DocsCallout>
          </DocsSection>

          {/* ── 04 What lands in Exponential ── */}
          <DocsSection
            id="what-lands"
            num="04"
            label="What lands in Exponential"
          >
            <h2>What lands in Exponential</h2>
            <p>Each submission becomes, atomically:</p>
            <ul>
              <li>
                An <strong>issue</strong> in the configured feedback project,
                with the visitor&apos;s message as the description.
              </li>
              <li>
                The <strong>screenshot as an attachment</strong>, embedded in
                the issue.
              </li>
              <li>
                A metadata block: <strong>reporter email</strong> (from{` `}
                <code>identify</code> or the form), the <strong>page
                URL</strong>, browser and viewport details, and your{` `}
                <code>setCustomData</code> payload.
              </li>
            </ul>
            <p>
              The reporter is <strong>auto-subscribed</strong> to the issue —
              resolve it and they&apos;re notified. With the{` `}
              <a href="/docs/feedback/#helpdesk">helpdesk</a> enabled on the
              project, the report also opens an email conversation you answer
              from the Support inbox.
            </p>
          </DocsSection>

          {/* ── 05 Try it ── */}
          <DocsSection id="try-it" num="05" label="Try it">
            <h2>Try it</h2>
            <p>
              This site runs the real widget — the feedback button in the
              corner of this page is a live install of exactly the snippet
              above. Click it, annotate a screenshot, submit, and your report
              lands on the{` `}
              <a href="https://app.exponential.at">
                public Exponential feedback board
              </a>
              .
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
