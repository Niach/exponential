/* Concepts: A2UI in short, then the epic's vocabulary (catalog, theme,
   extension, host plugin, vapp) and how the four renderers agree. Written to
   stay true after round 1 (VAPP-98) lands: stable ideas first, the round-1
   specifics marked as such. */
import basicMap from "@exponential-at/ui/catalog/basic-map.json"
import { DocsCallout, DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import { COMPONENT_COUNT, CORE_CATALOG as CATALOG_ID, CORE_LITE_CATALOG as LITE_CATALOG_ID, MACRO_COUNT, NATIVE_COUNT, componentHref } from "../lib/catalog-facts"
import { LEARN_NAV } from "../lib/content"
import { guidePath } from "../lib/guides"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `generative-ui`, num: `01`, label: `Generative UI` },
  { id: `surfaces`, num: `02`, label: `Surfaces and messages` },
  { id: `data`, num: `03`, label: `Data model` },
  { id: `actions`, num: `04`, label: `Actions and functions` },
  { id: `catalog`, num: `05`, label: `Catalog` },
  { id: `basic`, num: `06`, label: `A2UI basic, mapped` },
  { id: `themes`, num: `07`, label: `Themes` },
  { id: `extensions`, num: `08`, label: `Extensions` },
  { id: `host-plugins`, num: `09`, label: `Host plugins` },
  { id: `vapps`, num: `10`, label: `Vapps` },
  { id: `renderers`, num: `11`, label: `How renderers agree` },
]

/** `Button` → a link to its component page (catalog-facts.test.ts checks every name used here). */
const C = ({ name }: { name: string }) => (
  <a href={componentHref(name)}>
    <code>{name}</code>
  </a>
)

const MESSAGES = `{"version":"v0.9","createSurface":{"surfaceId":"main","catalogId":"${CATALOG_ID}"}}
{"version":"v0.9","updateComponents":{"surfaceId":"main","components":[
  {"id":"root","component":"Card","title":"Deploy 0.18.84?","children":["note","go"]},
  {"id":"note","component":"Text","text":{"path":"/summary"}},
  {"id":"go","component":"Button","label":"Deploy","on":{"press":{"event":{"name":"deploy","context":{"version":"0.18.84"}}}}}
]}}
{"version":"v0.9","updateDataModel":{"surfaceId":"main","path":"/summary","value":"3 platforms, all checks green."}}`

const TEMPLATE = `{"id":"people","component":"List","children":{"componentId":"person","path":"/people"}}
{"id":"person","component":"ListRow","title":{"path":"name"},"subtitle":{"path":"role"}}`

const ACTION_OUT = `{"version":"v0.9","action":{"name":"deploy","surfaceId":"main","sourceComponentId":"go",
  "timestamp":"2026-10-08T09:14:02Z","context":{"version":"0.18.84"}}}`

const FUNCTION_CALL = `{"id":"docs","component":"Button","label":"Open the docs",
 "on":{"press":{"functionCall":{"call":"openUrl","args":{"url":"https://ui.exponential.at/"}}}}}`

const THEME = `{
  "id": "brand", "name": "Brand", "extends": "neutral",
  "modes": { "dark": { "color": { "primary": "#818cf8" } } },
  "tokens": { "radius": { "md": 10 } },
  "recipes": {
    "Button": { "root": [
      { "style": { "borderRadius": "$radius.full" } },
      { "when": { "variant": "outline", "state": "hover" }, "style": { "backgroundColor": "$color.accent" } }
    ] }
  }
}`

const EXTENSION = `{
  "id": "https://example.com/catalogs/metrics/v1",
  "extends": "${CATALOG_ID}",
  "components": {
    "Sparkline": { "kind": "native", "props": { "values": { "type": "array", "required": true } }, … },
    "Metric":    { "kind": "macro",  "props": { "label": …, "value": …, "history": … }, … }
  },
  "macros": { "Metric": { "root": { "component": "Card", "children": [ …Text, Text, Sparkline ] } } }
}`

const PACKAGE = `{
  "id": "com.example.hello", "name": "Hello", "version": "1.0.0",
  "catalogId": "${CATALOG_ID}",
  "theme": "neutral",
  "functions": ["app.toast"],
  "templates": {
    "main": {
      "components": [ { "id": "root", "component": "Card", … } ],
      "data": { "title": "Hello, world" },
      "bindings": [ { "path": "/weather", "source": "weather:current?city=Vienna" } ]
    }
  }
}`

export default function ConceptsPage({ path }: PageProps) {
  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <h1>Concepts</h1>
          <p>
            A2UI in five ideas, then the five words the SDK is built from: catalog, theme, extension, host plugin
            and vapp.
          </p>
        </div>
      </section>

      <DocsLayout nav={LEARN_NAV} title="Exponential UI" sections={SECTIONS} currentPath={path}>
        <DocsSection id="generative-ui" num="01" label="Generative UI">
          <h2>Generative UI</h2>
          <p>
            A chat answer is text. A <strong>generative UI</strong> answer is an interface: a card with the numbers,
            a form for the missing fields, a button that does the next step. The model decides <em>what</em> to
            show. The app decides <em>how</em> it looks and what it is allowed to do.
          </p>
          <p>
            <a href={LINKS.a2ui}>A2UI</a> is the open protocol for this. The agent sends UI as <strong>data</strong>,
            never as code: components from a catalog both sides agree on, a data model, and the actions it wants
            back. A client renders that with its own components. Exponential UI is a set of A2UI clients: one
            catalog, a TypeScript reference and a Rust core, and native renderers for React, SwiftUI, Jetpack Compose
            and gpui. It speaks A2UI v0.9 (the pinned v0.9.1 spec, vendored byte for byte).
          </p>
          <DocsTable
            head={[`Word`, `What it is`]}
            rows={[
              [<strong key="w">Surface</strong>, `One rendered UI: a component tree, its data model and its theme.`],
              [<strong key="w">Catalog</strong>, `The components a surface may use and their props. The model reads it; the renderer implements it.`],
              [<strong key="w">Theme</strong>, `A JSON file of tokens and component recipes. Anyone can write one; renderers load it at runtime.`],
              [<strong key="w">Extension</strong>, `Extra components: an extension catalog plus a painter per platform, registered with the renderer.`],
              [<strong key="w">Host plugin</strong>, `What the embedding app lends a surface: transport, functions, data bindings, policy.`],
              [<strong key="w">Vapp</strong>, `An app built on all of the above: templates, data, bindings and a theme in one declarative package.`],
            ]}
          />
        </DocsSection>

        <DocsSection id="surfaces" num="02" label="Surfaces and messages">
          <h2>Surfaces and messages</h2>
          <p>
            An agent talks to a client in four messages: <code>createSurface</code> (an id and the catalog it will
            use), <code>updateComponents</code> (components, added or replaced by id), <code>updateDataModel</code>{` `}
            (a value at a JSON pointer) and <code>deleteSurface</code>. They stream one per line as JSONL, over SSE, a
            WebSocket or inside an MCP tool result.
          </p>
          <DocsCode language="jsonl">{MESSAGES}</DocsCode>
          <p>
            Components arrive as a <strong>flat list</strong> that references children by id, so a model can stream
            them in any order and patch one without resending the tree. The renderer rebuilds the tree, maps A2UI
            basic components onto the core catalog, expands macros and paints. A component it does not know becomes
            a visible placeholder and a reported issue, never a crash.
          </p>
        </DocsSection>

        <DocsSection id="data" num="03" label="Data model">
          <h2>Data model</h2>
          <p>
            Every surface has one JSON data model. A prop marked bindable can take <code>{`{"path": "/summary"}`}</code>{` `}
            instead of a literal, and it follows the model: after an <code>updateDataModel</code>, every node
            bound to that path shows the new value. Inputs bind both ways, so what the user types lands at the same path.
          </p>
          <p>
            A list repeats a template per item. The <code>children</code> name a component and an array path, and
            relative paths inside the template read the current item:
          </p>
          <DocsCode language="jsonl">{TEMPLATE}</DocsCode>
          <p>
            Bindings resolve when the renderer binds the tree to the data, after macros have expanded. That is why
            the result is the same on every platform, macros included.
          </p>
          <DocsCallout kind="note" title="Coming with round 1">
            Round 1 adds a <code>visible</code> condition on every component, a <code>key</code> per template item
            (reordering keeps each row's focus and input), per-surface settings for locale, strings, mode, density
            and contrast, and host commands for focus and announcements.
          </DocsCallout>
        </DocsSection>

        <DocsSection id="actions" num="04" label="Actions and functions">
          <h2>Actions and functions</h2>
          <p>
            Interactive components fire events, and <code>on.&lt;event&gt;</code> says what happens. An{` `}
            <strong>event action</strong> goes back to the agent with its context resolved against the data model:
          </p>
          <DocsCode language="json">{ACTION_OUT}</DocsCode>
          <p>
            A <strong>function call</strong> runs on the client. The 14 A2UI basic functions are built in on every
            renderer (validation checks, formatting, <code>openUrl</code>, <code>and</code>/<code>or</code>/
            <code>not</code>), and a host can register its own behind a policy gate (see{` `}
            <a href="#host-plugins">host plugins</a>):
          </p>
          <DocsCode language="json">{FUNCTION_CALL}</DocsCode>
          <p>
            Buttons are optimistic: they stay pending until the host has handled the action. Text fields are
            host-owned: edits go out debounced with a revision, so a slow round trip never drops a keystroke.
          </p>
        </DocsSection>

        <DocsSection id="catalog" num="05" label="Catalog">
          <h2>The core catalog</h2>
          <p>
            The <strong>catalog</strong> is the components a surface may use. The core catalog,{` `}
            <code>{CATALOG_ID}</code>, has <a href="/components/">{COMPONENT_COUNT} components</a> with
            shadcn's names, variants and sizes (<C name="Button" />, <C name="Card" />, <C name="Tabs" />,{` `}
            <C name="Select" />, <C name="Dialog" />, <C name="Chart" /> …). Every component and prop has one sentence
            written for the model; <code>catalogPrompt()</code> turns them into a compact system prompt.
          </p>
          <ul>
            <li>
              <strong>{NATIVE_COUNT} natives</strong> each renderer paints itself, such as <C name="Box" />,{` `}
              <C name="Text" />, <C name="Input" /> and <C name="Popover" />.
            </li>
            <li>
              <strong>{MACRO_COUNT} macros</strong> expand into natives before painting, such as <C name="Card" />,{` `}
              <C name="Badge" />, <C name="Alert" /> and <C name="Table" />. A renderer never implements a macro, and
              every part it expands into stays themable.
            </li>
            <li>
              <strong>Core lite</strong> (<code>{LITE_CATALOG_ID}</code>) is the subset without overlays, media and
              charts, for small screens and smaller prompts.
            </li>
          </ul>
          <p>
            A client advertises the catalog ids it supports (core, core lite, A2UI basic, then its extensions), and
            a surface names one in <code>createSurface</code>. A surface that asks for a catalog the client lacks is
            refused with an error the agent can read.
          </p>
        </DocsSection>

        <DocsSection id="basic" num="06" label="A2UI basic, mapped">
          <h2>A2UI basic, mapped</h2>
          <p>
            A2UI ships a small <strong>basic catalog</strong>. Exponential UI vendors it unchanged and maps it onto
            the core catalog through one table, so a surface written for any A2UI client renders here too and
            painters only ever learn one vocabulary. Dynamic values pass through untouched, the basic icon names map
            onto the icon registry, and the 14 functions are the same.
          </p>
          <DocsTable
            className="concepts-map"
            head={[`A2UI basic`, `Core`, `Mapping`]}
            rows={Object.entries(basicMap.components).map(([basic, rule]) => {
              const r = rule as { to: string; transform?: string }
              return [<code key="b">{basic}</code>, <C key="c" name={r.to} />, r.transform ? <code key="t">{r.transform}</code> : `props renamed`]
            })}
          />
          <p>
            Named transforms (<code>textVariant</code> turns <code>h1</code>…<code>h4</code> into a{` `}
            <C name="Heading" />, for example) are listed in{` `}
            <a href={LINKS.source(`packages/exponential-ui/catalog/basic-map.json`)}>basic-map.json</a> so a second
            implementation knows the rules.
          </p>
        </DocsSection>

        <DocsSection id="themes" num="07" label="Themes">
          <h2>Themes</h2>
          <p>
            A <strong>theme</strong> is data, never code: one JSON file of <strong>tokens</strong> (colours per
            mode, radius, spacing, type, control heights, shadows, motion) and <strong>recipes</strong> (per
            component part, a style for each combination of variant and state). Every renderer loads it at
            runtime, so a theme can come from a URL, a vapp or a server.
          </p>
          <DocsCode language="json">{THEME}</DocsCode>
          <p>
            <code>extends</code> overrides only what changes. Three themes are built in: <code>neutral</code>{` `}
            (stock shadcn, the root), <code>exponential</code> (the zinc glass of the Exponential apps) and{` `}
            <code>playful</code> (deliberately different, the one conformance renders everywhere). See them side by
            side on <a href="/themes/">Themes</a>, build one in the <a href="/themes/builder/">theme builder</a>, or
            follow <a href={guidePath(`themes`)}>Write your own theme</a>.
          </p>
        </DocsSection>

        <DocsSection id="extensions" num="08" label="Extensions">
          <h2>Extensions</h2>
          <p>
            An <strong>extension</strong> adds components. It is an extension catalog (its own id, extending the
            core, never shadowing a core name) plus, for each native component, a <strong>painter per
            platform</strong> registered with the renderer. A macro component needs no painter at all: it expands
            into core components everywhere.
          </p>
          <DocsCode language="json">{EXTENSION}</DocsCode>
          <p>
            The model learns the new components from the same prompt (<code>catalogPrompt({`{extensions}`})</code>
            ), and the client advertises the extension's id. The Exponential app's issue rows, run rows and chips
            are the first extension, painted by the app's own components on all four clients. See{` `}
            <a href={guidePath(`extensions`)}>Write an extension</a>.
          </p>
        </DocsSection>

        <DocsSection id="host-plugins" num="09" label="Host plugins">
          <h2>Host plugins</h2>
          <p>
            A surface needs a few things from the app that embeds it, the <strong>host</strong>. It is the same
            contract on all four platforms:
          </p>
          <ul>
            <li>
              <strong>Transport</strong>: messages in, actions out. JSONL streams, SSE, WebSocket, MCP or in memory.
            </li>
            <li>
              <strong>Functions</strong>: the names a surface may call beyond the built-ins (<code>cart.add</code>,{` `}
              <code>app.toast</code>).
            </li>
            <li>
              <strong>Data bindings</strong>: <code>bindDataModel</code> points a path at a source URI such as{` `}
              <code>weather:current?city=Vienna</code>; the host's resolver for that scheme keeps it filled.
            </li>
            <li>
              <strong>Policy hooks</strong>: which functions run (allow, ask, deny), which URLs open, which media
              requests carry credentials.
            </li>
          </ul>
          <p>
            The function gate is strict: deny wins, then allow, then ask (your consent UI), then the default. The
            same rules are fixtures every renderer replays, so a policy means the same thing everywhere. See{` `}
            <a href={guidePath(`host-plugins`)}>Write a host plugin</a>.
          </p>
        </DocsSection>

        <DocsSection id="vapps" num="10" label="Vapps">
          <h2>Vapps</h2>
          <p>
            A <strong>vapp</strong> is an app built on all of the above and shipped as data: a declarative package
            of templates (components, initial data, bindings), the functions it may call and its theme. Any host
            runs it with <code>createVappHost</code>, and its <code>functions</code> list becomes the policy for its
            surfaces: listed names are allowed, everything else is denied.
          </p>
          <DocsCode language="json">{PACKAGE}</DocsCode>
          <p>
            Against an Exponential instance, a connector does the MCP OAuth grant and serves the instance's issues,
            boards, teams and members as <code>exp:</code> sources. Vapps run on the vapps platform; to build one,
            see <a href={guidePath(`vapps`)}>Build and embed a vapp</a>.
          </p>
        </DocsSection>

        <DocsSection id="renderers" num="11" label="How renderers agree">
          <h2>How the renderers agree</h2>
          <p>
            Two implementations hold the rules: the TypeScript reference in <code>@exponential-at/ui</code> and the
            Rust core <code>exponential-ui</code>, which replays the same fixtures byte for byte. The React renderer
            runs on the TypeScript side. SwiftUI and Compose reach the Rust core through a UniFFI facade, and gpui
            links it directly.
          </p>
          <DocsCode language="text">{`A2UI messages ─▶ reducer ─▶ UiNode tree ─▶ layout (taffy) ─▶ frames + resolved visuals ─▶ painter
               (basic map,     (bindings, parts,       (text measured in batches
                macros,         layers, lists)          by the platform)
                extensions)`}</DocsCode>
          <p>
            The core lays out every native surface. The platform only measures text, in batches with at most three
            calls per pass, and paints each node at its frame with its own views. The web follows the same layout
            with CSS, held within a pixel of the core's frames. Painters never read a theme; they get resolved
            visuals per part and state. The <a href="/conformance/">conformance suite</a> checks all of it, case by
            case, on every renderer.
          </p>
        </DocsSection>
      </DocsLayout>
    </>
  )
}
