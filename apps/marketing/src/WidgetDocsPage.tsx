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
  { id: `form-fields`, num: `03`, label: `Form fields` },
  { id: `headless`, num: `04`, label: `Headless mode` },
  { id: `screenshots`, num: `05`, label: `Screenshots & annotation` },
  { id: `what-lands`, num: `06`, label: `What lands in Exponential` },
  { id: `try-it`, num: `07`, label: `Try it` },
]

const WIDGET_SNIPPET =
  `<script>
  // Exponential feedback widget. Full docs — init options, identify,
  // setCustomData, setTheme (dark/light/auto), setLauncherHidden, labels,
  // headless submit:
  // https://exponential.at/docs/widget/
  (function (w, d, u) {
    if (w.ExponentialWidget) return;
    var q = [], api = { q: q };
    ["init","identify","setCustomData","setTheme","setLauncherHidden","open","close","submit"].forEach(function (m) {
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
              A feedback button for any website. Visitors report bugs with an
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
              (team owners only; every plan includes at least one). Each
              config gets a public <code>expw_</code> key, a{` `}
              <strong>domain allowlist</strong> (submissions are only accepted
              from pages on domains you list), a{` `}
              <strong>mode</strong> (feedback, support, or both), and for
              feedback a <strong>target board</strong> where reports land.
            </p>
            <p>
              Then paste the snippet before <code>&lt;/head&gt;</code> on your
              site. It&apos;s the GA-style async pattern: a tiny queue stub
              loads the real script lazily, so it never blocks your page.
            </p>
            <DocsCode language="html">{WIDGET_SNIPPET}</DocsCode>
            <p>
              That&apos;s the whole install. A floating feedback button
              appears, and calls made before the script loads are queued and
              replayed.
            </p>
            <DocsCallout kind="note" title="The key is public by design">
              <code>expw_</code> keys ship in page source, like an analytics
              ID. The domain allowlist plus server-side rate limits are what
              gate submissions, so no secret ever has to live in the page.
            </DocsCallout>
          </DocsSection>

          {/* ── 02 JS API ── */}
          <DocsSection id="js-api" num="02" label="JS API">
            <h2>JS API</h2>
            <p>
              The snippet exposes <code>window.ExponentialWidget</code> with
              seven calls:
            </p>
            <DocsCode language="js">{`
// Call this once to boot the widget with your public key.
// Optional init overrides: theme ("dark" | "light" | "auto"),
// position, color, label, showButton, zIndex.
ExponentialWidget.init({ key: "expw_YOUR_KEY" });

// Attach your signed-in user, so reports arrive with a
// real reporter (and helpdesk replies reach their inbox).
ExponentialWidget.identify({
  email: "ada@example.com",
  name: "Ada Lovelace",
  userId: "usr_123",
});

// Arbitrary context stamped onto every submission:
// plan, build, feature flags, tenant…
ExponentialWidget.setCustomData({
  plan: "business",
  version: "1.42.0",
});

// Hook the widget to your site's dark/light toggle — launcher
// and panel restyle live. "auto" follows the visitor's system.
ExponentialWidget.setTheme("light");

// Hide the launcher while your own UI covers its corner (a bottom
// sheet, a mobile action bar). Only the button goes away: an open
// panel keeps rendering and open()/close()/submit() keep working.
ExponentialWidget.setLauncherHidden(true);

// Open / close the panel programmatically. Wire your own
// "Report a bug" menu item to open().
ExponentialWidget.open();
ExponentialWidget.close();

// Submit without the panel. See Headless mode below.
ExponentialWidget.submit({ title: "Broken button" });
`}</DocsCode>
            <p>
              All calls are safe to make before the script has loaded. The
              loader queues and replays them in order. (A queued{` `}
              <code>submit</code> runs fire-and-forget; call it after load,
              e.g. from a click handler, to get its Promise.)
            </p>
          </DocsSection>

          {/* ── 03 Form fields ── */}
          <DocsSection id="form-fields" num="03" label="Form fields">
            <h2>Form fields</h2>
            <p>
              The feedback form always asks for a title and details. Everything
              else is configured per widget in{` `}
              <strong>Team settings → Widget</strong>:
            </p>
            <ul>
              <li>
                <strong>Email</strong>: shown by default and optional; make it
                required, or hide it entirely for internal tools where nobody
                wants resolution emails.
              </li>
              <li>
                <strong>Name</strong>: off by default. Turn it on (optionally
                required) when a plain name is all you need to walk over and
                ask &ldquo;what did you mean?&rdquo; without collecting an
                email.
              </li>
              <li>
                <strong>Custom fields</strong>: up to 8 extra text inputs
                (e.g. &ldquo;Which page?&rdquo;, &ldquo;Order number&rdquo;).
                Responses land in the submission&apos;s custom-data block,
                alongside your <code>setCustomData</code> payload. A typed
                response wins over a host-set key of the same name.
              </li>
              <li>
                <strong>Labels</strong>: expose up to 10 of your team&apos;s
                labels (&ldquo;Bug&rdquo;, &ldquo;Idea&rdquo;, …) as toggle
                chips, and the reporter&apos;s picks arrive on the created
                issue — triage done at the source.
              </li>
              <li>
                <strong>Appearance</strong>: dark (default), light, or
                match-the-visitor&apos;s-system theme, plus accent, background
                and text color overrides — all with a live preview in
                settings.
              </li>
            </ul>
            <p>
              Visitors attached via <code>identify()</code> skip the email and
              name fields. Their identity rides along invisibly. Support mode
              always asks for an email: it&apos;s the reply channel.
            </p>
          </DocsSection>

          {/* ── 04 Headless mode ── */}
          <DocsSection id="headless" num="04" label="Headless mode">
            <h2>Headless mode</h2>
            <p>
              Want your own feedback UI? Boot the widget without its button and
              submit programmatically. You keep the key + domain gating, rate
              limits, and issue creation, and skip the panel entirely:
            </p>
            <DocsCode language="js">{`
ExponentialWidget.init({ key: "expw_YOUR_KEY", showButton: false });
ExponentialWidget.identify({ email: "ada@example.com", name: "Ada" });

// Later, from your own form's submit handler:
const result = await ExponentialWidget.submit({
  title: "Broken button",           // required by the server
  description: "Steps to reproduce…",
  name: "dani",                     // overrides identify()
  customData: { page: "checkout" }, // merged over setCustomData()
  screenshot: myBlob,               // optional: you capture it
});

if (result.ok) {
  console.log("Filed as", result.identifier); // e.g. "EXP-42"
} else {
  console.error(result.error, result.code);
}

// Support mode works too (requires the helpdesk):
await ExponentialWidget.submit({
  mode: "support",
  message: "I can't log in",
  email: "ada@example.com", // required: it's the reply channel
});
`}</DocsCode>
            <p>
              <code>submit()</code> resolves with{` `}
              <code>{`{ ok, identifier, url }`}</code> on success and{` `}
              <code>{`{ ok: false, error, code }`}</code> on failure. It never
              throws. Screenshots are yours to capture in headless mode; pass a{` `}
              <code>Blob</code> (PNG, JPEG, or WebP) and it&apos;s attached
              like a panel screenshot. Server-side validation (required fields,
              modes, rate limits) applies exactly as it does to the panel.
            </p>
          </DocsSection>

          {/* ── 05 Screenshots & annotation ── */}
          <DocsSection
            id="screenshots"
            num="05"
            label="Screenshots & annotation"
          >
            <h2>Screenshots &amp; annotation</h2>
            <p>
              Screenshots are captured <strong>client-side, in the
              browser</strong>. The visitor&apos;s viewport is rendered
              locally and nothing is fetched by a server-side browser, so
              what&apos;s on their screen (including logged-in state) is what
              you see.
            </p>
            <p>
              On desktop browsers <strong>Take screenshot</strong> uses the
              browser&apos;s native screen sharing to grab a single frame —
              it captures content a page snapshot can&apos;t render
              (canvas/WebGL, video, cross-origin iframes). The visitor picks
              the surface in the browser&apos;s own dialog, one frame is
              taken, and sharing stops immediately. On mobile (and if the
              visitor dismisses that dialog) the widget falls back to a local
              page snapshot automatically.
            </p>
            <p>
              Menus, dropdowns and popups that close on click can be captured
              too: the small segment next to <strong>Take screenshot</strong>{" "}
              holds the shot for <strong>3 or 5 seconds</strong> with a
              countdown in the launcher&apos;s corner, so the visitor can open
              whatever should be in the picture first. On desktop the
              countdown starts once the share dialog is confirmed.
            </p>
            <p>
              Before submitting, the visitor can <strong>annotate</strong> the
              screenshot in a full-screen editor: rectangles, arrows, and
              freehand lines, with undo. Annotations are flattened into the
              image on submit.
            </p>
            <DocsCallout kind="tip" title="Capture never blocks a report">
              If capture fails on an exotic page, the report still submits,
              just without the image.
            </DocsCallout>
          </DocsSection>

          {/* ── 06 What lands in Exponential ── */}
          <DocsSection
            id="what-lands"
            num="06"
            label="What lands in Exponential"
          >
            <h2>What lands in Exponential</h2>
            <p>Each feedback submission becomes, atomically:</p>
            <ul>
              <li>
                An <strong>issue</strong> on the configured board, with the
                visitor&apos;s message as the description.
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
              The reporter is <strong>auto-subscribed</strong> to the issue.
              Resolve it and they&apos;re notified.
            </p>
            <p>
              <strong>Support requests are different</strong>: with the{` `}
              <a href="/docs/feedback/#helpdesk">helpdesk</a> enabled (Team
              plan), they skip the board entirely and open a{` `}
              <strong>ticket in your team&apos;s Support inbox</strong>. That
              ticket is an email conversation with the reporter that any member
              can answer, and escalate into an issue on any board when it turns
              out to be a bug.
            </p>
          </DocsSection>

          {/* ── 07 Try it ── */}
          <DocsSection id="try-it" num="07" label="Try it">
            <h2>Try it</h2>
            <p>
              This site runs the real widget. The feedback button in the
              corner of this page is a live install of exactly the snippet
              above. Click it, annotate a screenshot, submit, and your report
              lands on the Exponential team&apos;s own feedback board.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
