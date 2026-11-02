/* /docs/apps/: the apps. First your team's apps (eApps, EXP-1606: the
   concepts, the package, templates and bindings, records, functions,
   exposes and upstreams, triggers, building one with an agent), then
   Exponential's own native apps. Facts: docs/spec/apps.md (what it marks
   🆕/❌ reads "coming" here, never as shipped), the fixtures apps.json,
   app-records.json, app-actions.json and app-export.json, and
   packages/exponential-ui/docs/package-format.md. The visual pitch is
   exponential.at/apps/. */
import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcArrow } from "./components/icons"
import { DocShot } from "./components/DocShot"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `team-apps`, num: `01`, label: `Team apps` },
  { id: `build`, num: `02`, label: `Build one with an agent` },
  { id: `package`, num: `03`, label: `The package` },
  { id: `templates`, num: `04`, label: `Templates and bindings` },
  { id: `records`, num: `05`, label: `Records` },
  { id: `functions`, num: `06`, label: `Functions` },
  { id: `exposes`, num: `07`, label: `Exposes and upstreams` },
  { id: `triggers`, num: `08`, label: `Triggers` },
  { id: `share`, num: `09`, label: `Share as a file` },
  { id: `desktop`, num: `10`, label: `Desktop` },
  { id: `mobile`, num: `11`, label: `Mobile` },
  { id: `push`, num: `12`, label: `Push notifications` },
  { id: `steer`, num: `13`, label: `Steer from anywhere` },
]

const PACKAGE = `{
  "formatVersion": 2,
  "id": "acme.stunden",
  "name": "Stunden",
  "version": "1.0.0",
  "catalogId": "https://ui.exponential.at/catalogs/core/v1",
  "icon": { "glyph": "clock", "color": "blue" },
  "sources": ["exp:records", "exp:issues"],
  "functions": ["harness.records.*", "exponential.comments.create"],
  "records": {
    "entry": {
      "title": "Entry",
      "schema": {
        "type": "object",
        "properties": {
          "date":  { "type": "string", "format": "date" },
          "hours": { "type": "number", "minimum": 0, "maximum": 24 },
          "note":  { "type": "string", "maxLength": 500 }
        },
        "required": ["date", "hours"]
      }
    }
  },
  "templates": {
    "month": {
      "components": [ { "id": "root", "component": "Column", "children": ["chart", "table"] }, … ],
      "data": { "title": "Stunden" },
      "bindings": [
        { "path": "/month", "source": "exp:records?kind=entry&mine=1&by=date&sum=hours" }
      ]
    }
  },
  "exposes": [ … ],
  "upstreams": []
}`

const BINDING = `{ "id": "days", "component": "Chart", "type": "bar",
  "labels": { "path": "/month/month/labels" },
  "series": [{ "name": "Hours", "values": { "path": "/month/month/sums" } }] }`

const ACTION = `{ "id": "log", "component": "Button", "label": "Log hours",
  "on": { "press": { "functionCall": { "call": "harness.records.upsert",
    "args": { "kind": "entry", "data": { "date": { "path": "/form/date" }, "hours": { "path": "/form/hours" } } } } } } }`

const EXPOSES = `"upstreams": ["stripe"],
"exposes": [
  { "name": "log_hours", "title": "Log hours", "description": "Log hours for a day.",
    "input": { "type": "object", "properties": { "date": { "type": "string" }, "hours": { "type": "number" } },
               "required": ["date", "hours"] },
    "access": "write",
    "run": { "records": { "op": "upsert", "kind": "entry",
                          "data": { "date": "\${input.date}", "hours": "\${input.hours}" } } } },
  { "name": "failed_payments", "title": "Failed payments", "description": "Customers whose payment failed.",
    "input": { "type": "object", "properties": {} },
    "access": "read",
    "run": { "upstream": "stripe", "tool": "list_failed_invoices", "args": {} } }
]`

const TRIGGERS = `"automations": [
  { "name": "standup_digest", "on": { "schedule": "0 17 * * MON-FRI" },
    "run": { "agent": { "agentId": "standup-bot", "prompt": "Summarise today's standup records" } } }
]`

export function AppsDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Apps</h1>
            <p>
              The apps your team builds inside Exponential by asking an agent,
              and Exponential&apos;s own apps for desktop, iOS and Android.
            </p>
            <div className="docs-hero-cta">
              <a className="btn btn-primary" href="/apps/">
                eApps at a glance <IcArrow size={12} />
              </a>
              <a className="btn btn-ghost" href={LINKS.downloadPage}>
                Download
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/apps/">
          {/* ── 01 Team apps ── */}
          <DocsSection id="team-apps" num="01" label="Team apps">
            <h2>Team apps</h2>
            <p>
              A team app is a small tool your team uses every day: a
              working-hours tracker, a release checklist, an admin console
              over an API, a dashboard over a board. You describe it, an agent
              builds it, and everyone on the team opens it on the web, the
              desktop, iOS and Android.
            </p>
            <p>
              An app is <strong>data, never code</strong>: one declarative
              package (a manifest, eUI templates, bindings, record kinds, a
              function allowlist, <code>exposes</code> and{` `}
              <code>upstreams</code>), stored per team and versioned. The same
              eUI renderers paint it on every platform. Three things run an
              app, and none of them runs app code:
            </p>
            <ul>
              <li>
                <strong>The client host</strong>, while someone is looking at
                the app: bindings, buttons and forms.
              </li>
              <li>
                <strong>The server broker</strong>, for the app&apos;s MCP
                tools, so no device has to be online.
              </li>
              <li>
                <strong>An agent</strong>, when the work needs judgement: it
                builds the app and fills its data through MCP today; handing
                work from a running app to an agent is coming.
              </li>
            </ul>
            <p>
              Code that must always run lives behind an{` `}
              <strong>upstream MCP server</strong> your team already runs
              (your own API, Stripe, GitHub); the app calls it by name. There
              are no hosted apps, no sandbox and no per-app process.
            </p>
            <p>
              Apps live in the sidebar&apos;s <strong>Apps</strong> group,
              between Agents and Boards; on the phones, Apps is its own tab.
              The group lists only your team&apos;s apps: Exponential ships
              none of its own.
            </p>
          </DocsSection>

          {/* ── 02 Build one with an agent ── */}
          <DocsSection id="build" num="02" label="Build one with an agent">
            <h2>Build one with an agent</h2>
            <p>
              The Apps group&apos;s <strong>+</strong> (or{` `}
              <strong>New app</strong> on the Apps list) opens the editor
              straight away on an <strong>Untitled app</strong> draft: no form.
              Pick a starter (<strong>Log working hours</strong>,{` `}
              <strong>Release checklist</strong>,{` `}
              <strong>Admin console for an API</strong>,{` `}
              <strong>Team dashboard</strong>) or type into{` `}
              <strong>Describe your app…</strong>. The agent names the app and
              picks its icon and colour with its first version. A draft you
              leave without sending anything is discarded.
            </p>
            <DocShot
              view="app"
              caption="The app editor: the chat with its editor session beside the live preview"
            />
            <p>
              The chat is your <strong>editor session</strong> with the app.
              It runs on your device (<strong>Start a device to edit this
              app</strong> while none is online): the agent reads the eUI
              catalog, writes the package and seeds sample records, and every
              change lands as a new version the preview repaints. Type into{` `}
              <strong>Ask for a change…</strong> to keep going.
            </p>
            <p>
              On the web and the desktop, the chat sits beside four tabs:{` `}
              <strong>Preview</strong>, <strong>Data</strong> (the app&apos;s
              records), <strong>Versions</strong> and{` `}
              <strong>Settings</strong>. On a phone, the editor is three tabs
              (<strong>Chat</strong>, <strong>Preview</strong>,{` `}
              <strong>Data</strong>) with Versions, Settings, Export and Delete
              in the <strong>…</strong> menu.
            </p>
            <p>
              <strong>Versions</strong> lists every version (<code>v1</code>,
              {` `}<code>v2</code>, …) with its note, when and who; the newest
              is <strong>Current</strong>. <strong>Preview</strong> an older
              one, then <strong>Restore</strong> to publish it again as a new
              version. Any member can create an app; the creator or an owner
              can rename and delete it.
            </p>
            <p>
              Any coding agent can do the same over MCP: the{` `}
              <a href="/docs/mcp/#tools"><code>exponential_apps</code></a>{` `}
              tool reads the catalog (<code>catalog</code>), writes a version
              (<code>upsert</code>) and fills records (<code>records_*</code>).
            </p>
            <DocsCallout kind="note" title="Coming">
              Draft and published versions (members see the published one
              while the editor works on the head), package checks against
              sample records, and a preview that runs as another member.
            </DocsCallout>
          </DocsSection>

          {/* ── 03 The package ── */}
          <DocsSection id="package" num="03" label="The package">
            <h2>The package</h2>
            <p>
              An app is one JSON package. The agent writes it; you rarely
              need to read it, but this is all there is:
            </p>
            <DocsCode language="json">{PACKAGE}</DocsCode>
            <ul>
              <li>
                <strong>Manifest</strong>: <code>id</code>, <code>name</code>,{` `}
                <code>version</code>, the eUI <code>catalogId</code> it targets,{` `}
                <code>icon</code>, <code>description</code>,{` `}
                <code>formatVersion</code> (a client refuses a newer one and
                asks you to update instead of showing a broken screen).
              </li>
              <li>
                <strong>templates</strong>: screens, each a flat list of eUI
                components with its data and bindings.
              </li>
              <li>
                <strong>sources</strong> and <strong>functions</strong>: the
                allowlists. A binding or a call that isn&apos;t listed is
                refused.
              </li>
              <li>
                <strong>records</strong>: the app&apos;s own data kinds.
              </li>
              <li>
                <strong>exposes</strong> and <strong>upstreams</strong>: the
                MCP tools it offers and the MCP servers it calls.
              </li>
            </ul>
            <p>
              Limits: a package up to 128 KB, 500 components per template, 16
              record kinds, 32 exposed tools. The format is eUI&apos;s
              (format version 2,{` `}
              <a href={`${LINKS.github.repo}/tree/master/packages/exponential-ui/docs/package-format.md`}>
                package-format.md
              </a>
              ); a package never carries scripts, and Exponential refuses one
              with <code>tools</code>.
            </p>
          </DocsSection>

          {/* ── 04 Templates and bindings ── */}
          <DocsSection id="templates" num="04" label="Templates and bindings">
            <h2>Templates and bindings</h2>
            <p>
              A template is an eUI surface: components from the{` `}
              <a href="/ui/components/">core catalog</a> (layout, forms,
              tables, charts, stat tiles, …) plus Exponential&apos;s own
              extension (issue rows, session rows, status glyphs). An app
              can&apos;t invent a widget: what isn&apos;t in the catalog
              isn&apos;t in an app, so every app looks like the rest of
              Exponential on every platform.
            </p>
            <p>
              A <strong>binding</strong> fills a path of the template&apos;s
              data live from a source. Every client resolves it the same way
              from the rows it already syncs, so an app never waits on a
              request:
            </p>
            <ul>
              <li>
                <code>exp:records?kind=…</code>: the app&apos;s own records
                (<a href="#records">below</a>).
              </li>
              <li>
                <code>exp:issues</code> (filtered by board, status, assignee,
                priority or a search), <code>exp:boards</code>,{` `}
                <code>exp:members</code>, <code>exp:sessions</code>,{` `}
                <code>exp:comments</code>, <code>exp:attachments</code> and{` `}
                <code>exp:devices</code>.
              </li>
            </ul>
            <DocsCode language="json">{BINDING}</DocsCode>
            <p>
              The preview paints the app&apos;s first template.
            </p>
            <DocsCallout kind="note" title="Coming">
              Several screens per app with a sidebar on desktop and a bottom
              bar on phones, deep links, a phone-only layout per screen, record
              forms and tables generated from a kind&apos;s schema, and read
              bindings over an upstream MCP server.
            </DocsCallout>
          </DocsSection>

          {/* ── 05 Records ── */}
          <DocsSection id="records" num="05" label="Records">
            <h2>Records</h2>
            <p>
              An app keeps its own data as <strong>records</strong>. Its
              package declares up to 16 kinds (a day of working hours, a todo,
              a customer), each with a flat JSON Schema: strings, numbers,
              integers, booleans and arrays; the formats date, date-time and
              time; enum, min/max, maxLength and required. The server checks
              every write against it; a record is at most 16 KB.
            </p>
            <p>
              The app reads them live through{` `}
              <code>exp:records?kind=&lt;kind&gt;</code> and writes them with{` `}
              <code>harness.records.upsert</code> and{` `}
              <code>harness.records.delete</code>, without a consent card:
              they are its own rows. Every member sees the same records on
              every device.
            </p>
            <ul>
              <li><code>mine=1</code> or <code>user=&lt;id&gt;</code>: only your rows, or one member&apos;s.</li>
              <li><code>&lt;field&gt;=v</code> and <code>&lt;field&gt;.gt</code>, <code>.gte</code>, <code>.lt</code>, <code>.lte</code>: filter on a field.</li>
              <li><code>sort</code>, <code>order</code>, <code>limit</code>: order and cut (at most 500 rows).</li>
              <li><code>sum=&lt;field&gt;</code>: the total over every match.</li>
              <li><code>by=&lt;dateField&gt;&amp;month=YYYY-MM</code>: a month view, every day with its count and sum, ready for a chart.</li>
              <li><code>app=&lt;slug&gt;</code>: another app&apos;s records.</li>
            </ul>
            <p>
              The editor&apos;s <strong>Data</strong> tab lists and edits them.
              Agents fill them too, with the{` `}
              <code>exponential_apps</code> tool&apos;s{` `}
              <code>records_list</code>, <code>records_upsert</code> and{` `}
              <code>records_delete</code>: a session can log your hours for you
              or seed sample data while it builds the app. Free teams keep up
              to 1,000 records, Team plans 25,000; the app&apos;s settings show
              the count. A deleted record is purged after 30 days.
            </p>
            <DocsCallout kind="note" title="Coming">
              Records that know their author (only the author or an owner
              edits them, owners can write on behalf of a member), private and
              owners-only kinds, relations between kinds, history with undo,
              grouping and maths over any field, date maths in the team&apos;s
              time zone, and writes that queue while offline.
            </DocsCallout>
          </DocsSection>

          {/* ── 06 Functions ── */}
          <DocsSection id="functions" num="06" label="Functions">
            <h2>Functions</h2>
            <p>
              A button calls a <strong>function</strong>, and only the ones
              the package lists in <code>functions</code>. Side effects are
              exactly these: record writes, Exponential operations, toasts,
              navigation and links. There is no <code>fetch</code> and no
              background code.
            </p>
            <DocsCode language="json">{ACTION}</DocsCode>
            <ul>
              <li>
                <code>harness.*</code>: record writes, a toast, open an issue
                or a device, an MCP call. Record writes never ask;{` `}
                <code>harness.mcp</code> asks every time.
              </li>
              <li>
                <code>exponential.&lt;noun&gt;.&lt;op&gt;</code>: any
                Exponential operation, as you: set a status, file an issue,
                post a comment, start a session. It asks unless an owner
                switched it on.
              </li>
              <li>
                <code>bind</code>, <code>set</code>, <code>openUrl</code>:
                re-bind a source, set data, open a link (private hosts are
                refused). They never ask.
              </li>
            </ul>
            <p>
              The app&apos;s settings show its Exponential operations as{` `}
              <strong>Tools</strong>, with a switch each. A switched-on tool
              runs as you without asking; anything else asks every time, worded
              for the operation and with the arguments shown. Only a team owner
              flips the switches, and an operation that deletes, merges,
              closes, ends, removes or trashes can never be switched on: it
              reads <strong>Always asks</strong>. A new version from a member
              who is not an owner starts with nothing switched on.
            </p>
          </DocsSection>

          {/* ── 07 Exposes and upstreams ── */}
          <DocsSection id="exposes" num="07" label="Exposes and upstreams">
            <h2>Exposes and upstreams</h2>
            <p>
              An app is also an MCP server. Its package lists tools under{` `}
              <code>exposes</code>, as data: a name, an input schema,{` `}
              <code>read</code> or <code>write</code>, and exactly one thing
              it runs:
            </p>
            <ul>
              <li>its own <strong>records</strong> (list, get or upsert);</li>
              <li>
                an Exponential <strong>action</strong> that never deletes,
                merges or closes;
              </li>
              <li>
                a tool on one of its <strong>upstreams</strong>: the
                team&apos;s MCP servers (Settings › MCP) it names, called with
                each member&apos;s own account.
              </li>
            </ul>
            <DocsCode language="json">{EXPOSES}</DocsCode>
            <p>
              Any MCP client (Claude Code, Codex, OpenClaw) then calls them
              as you at{` `}
              <a href="/docs/mcp/#app-tools"><code>/api/mcp/apps</code></a>,
              named <code>&lt;app&gt;__&lt;tool&gt;</code> (
              <code>stunden__log_hours</code>), with no device online. One
              consent covers every app: <strong>Off</strong>,{` `}
              <strong>Read only</strong> or <strong>Full</strong> per app,
              upstreams connected inline. The app&apos;s settings list its
              tools under <strong>MCP tools</strong>, with an owner&apos;s
              switch each, and the servers they call under{` `}
              <strong>Accounts</strong>, with your connection to each.
            </p>
            <p>
              Results carry where they came from, descriptions are frozen per
              version, a delete is never exposed and a <code>read</code> tool
              can&apos;t write. An app sees only its own upstreams and records.
            </p>
          </DocsSection>

          {/* ── 08 Triggers ── */}
          <DocsSection id="triggers" num="08" label="Triggers">
            <h2>Triggers</h2>
            <DocsCallout kind="note" title="Coming">
              Triggers are specified and not built yet. Today an app runs while
              someone looks at it, and its MCP tools run when a client calls
              them.
            </DocsCallout>
            <p>
              An app will run on its own when something happens: a schedule
              (the Monday on-call rotation), a record change (a new request
              pings its approver), a team event (a merged pull request) or an
              inbound webhook (a failed Stripe payment). The rule:{` `}
              <strong>one trigger runs one binding</strong>, the same kinds
              as <code>exposes</code>. No chains, no branches, no loops;
              anything that needs judgement is handed to an agent, with the
              app&apos;s data passed as data, never as instructions.
            </p>
            <DocsCode language="json">{TRIGGERS}</DocsCode>
            <p>
              Triggers run in the server broker on the published version, act
              as the member who published them, and show up in the audit like
              any other call.
            </p>
          </DocsSection>

          {/* ── 09 Share as a file ── */}
          <DocsSection id="share" num="09" label="Share as a file">
            <h2>Share as a file</h2>
            <p>
              Export an app from its menu, its editor or its settings: you get
              one <code>.app.json</code> file with its name, icon, colour,
              current version and, if you switch Include records on, its
              records. Files can be signed, and the import says who signed
              them. To use one, choose <strong>Import…</strong> on the Apps
              list and paste the file or pick it. Anything wrong with it is
              listed under the field before you import. An imported app starts
              with no tools granted.
            </p>
            <p>
              Records leave through the file and through MCP, not as a
              spreadsheet: an agent reads them with an exposed tool or{` `}
              <code>records_list</code> and writes whatever file you need.
            </p>
          </DocsSection>

          {/* ── 10 Desktop ── */}
          <DocsSection id="desktop" num="10" label="Desktop">
            <h2>Desktop</h2>
            <p>
              The desktop app is a native Rust IDE, the client that runs{` `}
              <a href="/docs/coding/">coding sessions</a> and the git IDE
              around them. Grab it from the{` `}
              <a href={LINKS.downloadPage}>download page</a>:
            </p>
            <ul>
              <li>
                <strong>macOS</strong>: a notarized <code>.dmg</code>.
              </li>
              <li>
                <strong>Windows</strong>: a portable <code>.exe</code>, no
                installer.
              </li>
              <li>
                <strong>Linux</strong>: an <code>AppImage</code> for x86_64,
                glibc 2.39 or newer (Ubuntu 24.04 and later).
              </li>
            </ul>
            <p>
              All three come from{` `}
              <a href={LINKS.github.releases}>GitHub Releases</a>, with
              checksums published alongside. On first launch, sign in to the
              cloud or point it at your{` `}
              <a href="/docs/self-host/">self-hosted</a> instance URL.
            </p>
            <h3>Self-updating</h3>
            <p>
              The app checks the latest release at launch and every 30
              minutes while running, and downloads it in the background,
              verified against the published checksums. It restarts on its
              own once every session it runs pauses between turns (at most
              10 minutes), and only while no window has focus; sessions cut
              by the restart continue by themselves. At the app, the banner&apos;s{` `}
              <strong>Restart to update</strong> is yours. No package manager,
              no manual downloads.
            </p>
            <h3>The layout</h3>
            <p>
              The sidebar mirrors the web app: the team switcher,{` `}
              <strong>Search</strong> and <strong>New issue</strong> at the
              top, then <strong>Agent</strong>, Inbox,{` `}
              <strong>Chats</strong>, Devices and Drafts (while you have
              any), the team&apos;s <strong>Agents</strong>, its{` `}
              <strong>Apps</strong>, the <strong>Boards</strong>, and your{` `}
              <strong>Active</strong> chats. At the bottom sit the{` `}
              <strong>What&apos;s new</strong> card,{` `}
              <strong>Getting started</strong> and your account, whose menu
              holds What&apos;s new, About and Sign out. On its right, the
              {` `}<strong>Computer</strong> button opens this machine&apos;s
              tools (a new <strong>Terminal</strong>, its{` `}
              <strong>Files</strong> and <strong>Source Control</strong>),
              then the settings gear.
            </p>
            <p>
              The sidebar never leaves. Open an issue or a session from a list, or
              open Settings, and it folds into a narrow icon rail with that
              list beside it, so every entry stays one click away.
            </p>
            <p>
              The working area is a rounded card on a darker ground, the same
              cutout layout the web app uses. Work opens as tabs along its
              top: an issue, or a <strong>chat</strong> with its own strip
              (Conversation, the issue chips, the session chips)
              under the title. Terminals fill the card instead, and open
              terminals get a bar of tabs under it — a bar that is not there
              at all while none is open.
            </p>

            <DocShot
              view="steering"
              platform="desktop"
              caption="A session in the desktop IDE: the chat's tab along the top, its strip, and the sidebar beside it"
            />
            <h3>System notifications</h3>
            <p>
              The desktop app raises real macOS, Windows and XDG
              notifications when something lands in your inbox, coalescing a
              burst into one toast and staying quiet while the window is
              already focused. It is per machine:{` `}
              <strong>Settings → Notifications → Desktop notifications</strong>
              , with the per-type switches below it applying to those too.
            </p>
            <h3>Voice</h3>
            <p>
              <strong>Settings → Assistant → Voice</strong> picks your{` `}
              <strong>Voice device</strong> (one of your desktops) and the{` `}
              <strong>Agent</strong> you talk to, with the <strong>Language</strong>,
              and a <strong>Voice file</strong> with its{` `}
              <strong>Transcript</strong>. While a voice device is picked, a
              second group, <strong>On &lt;device&gt; (tray)</strong>, holds{` `}
              <strong>Push to talk</strong>, its <strong>Shortcut</strong>,{` `}
              <strong>Wake word</strong> and <strong>Wake phrase</strong>, and
              the IDE on that desktop sits in the system tray. Push to talk
              and spoken replies in English and German, calls and the wake
              word are in beta. More on{` `}
              <a href="/talk/">eTalk</a>.
            </p>
            <DocShot
              view="settings-assistant"
              platform="desktop"
              caption="Settings → Assistant: where your Assistant runs, its check-in, and its Voice block with the tray group"
            />
          </DocsSection>

          {/* ── 11 Mobile ── */}
          <DocsSection id="mobile" num="11" label="Mobile">
            <h2>Mobile</h2>
            <p>
              Native <a href={LINKS.downloads.ios}>iOS</a> and{` `}
              <a href={LINKS.downloads.android}>Android</a> apps with
              everything synced in real time, for one team at a time. The
              bar along the bottom has four tabs (the mobile web has the
              same):
            </p>
            <ul>
              <li>
                <strong>Board</strong>, named after the board you are on:
                its issues, with <strong>All issues</strong>,{` `}
                <strong>My issues</strong> and <strong>Drafts</strong>{` `}
                (while you have any) as chips under the title. Tap the tab
                again on the board and the bar turns into a menu of the
                team&apos;s boards, with <strong>New board</strong> at the
                end.
              </li>
              <li>
                <strong>Chats</strong>: the team&apos;s{` `}
                <a href="/docs/actions/#triggers">agents</a> as bubbles on
                top, the <strong>All</strong>, <strong>Needs you</strong>,{` `}
                <strong>Running</strong> and <strong>Unread</strong> chips,
                then one row per chat with its last message, its time and
                its unread count.
              </li>
              <li>
                <strong>Apps</strong>: the team&apos;s{` `}
                <a href="#team-apps">apps</a>.
              </li>
              <li>
                <strong>Team</strong>, named after the active team: your
                teams (a tap switches), then the team&apos;s settings, each
                row its own page (General for owners, Members, Labels,
                Boards, Agents, Repositories, Prompts and{` `}
                <strong>Devices</strong>: your machines, the team&apos;s
                shared ones and the agent accounts they hold), then your
                account.
              </li>
            </ul>
            <p>
              Every page has the same big title with round search and inbox
              buttons; the inbox is a page of its own, with{` `}
              <strong>Mark all read</strong>, and search looks in the active
              team. Beside the bar, the round chat button opens the{` `}
              <a href="/docs/coding/#start-coding">Agent page</a>: the one
              composer, where the app opens. <strong>New issue</strong> sits
              beside it. Select
              issues and the bulk bar takes the tab bar&apos;s place, with
              status, priority, assignee, labels and delete on it. Full
              onboarding runs on mobile too: create boards, connect GitHub,
              and manage repos without ever opening a laptop.
            </p>

            <div className="docs-shot-row">
              <DocShot view="board" platform="ios" caption="iOS" />
              <DocShot view="board" platform="android" caption="Android" />
            </div>
          </DocsSection>

          {/* ── 12 Push notifications ── */}
          <DocsSection id="push" num="12" label="Push notifications">
            <h2>Push notifications</h2>
            <p>
              Everything that lands in your <a href="/docs/issues/#notifications">inbox</a>{` `}
              (new feedback, assignments, comments, mentions, status changes,
              PR opened / merged, reporter replies, blocked sessions) arrives as
              a push notification the moment it happens. Tapping one deep-links
              straight to the issue. Notification preferences from{` `}
              <strong>Settings → Notifications</strong> apply to push too.
            </p>
            <DocsCallout kind="note" title="Push on self-hosted">
              The cloud has push wired up out of the box. The App Store / Play
              Store apps push only for cloud accounts; self-hosted instances
              get web, desktop system notifications and email instead. Why, and the
              build-it-yourself escape hatch:{` `}
              <a href="/docs/self-host/#push">
                self-host docs: push notifications
              </a>
              .
            </DocsCallout>
          </DocsSection>

          {/* ── 13 Steer from anywhere ── */}
          <DocsSection id="steer" num="13" label="Steer from anywhere">
            <h2>Steer from anywhere</h2>
            <p>
              A coding session started on your desktop doesn&apos;t chain you
              to the desk. <strong>Chats</strong>, on the phone and on the
              web, lists every chat with a{` `}
              <strong>live activity feed</strong> one tap away. Watch the
              agent work in real time, and{` `}
              <strong>send steer messages</strong> mid-session: answer its
              question, veto an approach, add a constraint you forgot. What
              needs you (a <strong>Question</strong>, a{` `}
              <strong>Plan ready</strong>) floats as a card above the
              composer; tap it and it opens as a sheet from the bottom. The
              feed and your messages go straight between your phone and the
              device running the session over eNet, end-to-end encrypted;
              no relay can read them.
            </p>
            <p>
              You can also start one from here: the Agent composer&apos;s{` `}
              <strong>+</strong> menu, its device, account and model pickers,
              and the session composer — <code>/</code> for
              the prompts and commands, <code>@</code> for a teammate,{` `}
              <code>#</code> for an issue and <code>:</code> for an emoji —
              are the same on the phone as on the desktop. On the phone,{` `}
              <strong>Stop</strong> in a chat&apos;s top bar ends the chat,
              the composer&apos;s stop button interrupts only the turn, and
              the context ring beside the composer opens{` `}
              <strong>Usage</strong> (how much of the agent&apos;s limits this
              session and this week have spent). Long-press a chat in the
              list to <strong>Stop</strong> it, <strong>Stop and delete</strong>{` `}
              it, or <strong>Delete</strong> an ended one. A chat is one screen
              paged by its strip: the <strong>Conversation</strong>, each
              linked issue and each session&apos;s work, swiped or tapped,
              with the composer floating over whichever page is up. The top
              bar carries <strong>Merge PR</strong> while the pull request is
              open (<strong>Merge stack</strong> for a stacked one), so a PR
              the chat opened merges from the phone too. The agents&apos;{` `}
              <strong>stories</strong> open full screen from the rings on top
              of Chats.
            </p>
            <p>
              The full flow (starting sessions, plan mode, several issues at
              once, review) is in <a href="/docs/coding/">Coding agents</a>.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
