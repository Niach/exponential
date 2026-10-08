/* Every guide, by the path's slug. The example files are the ones CI
   compiles from a fresh project (guides/**, guides/check.ts), shown through
   guideSource(), never pasted. */
import type { ReactNode } from "react"
import promptBudget from "@exponential-at/ui/fixtures/prompt-budget.json"
import { DocsCallout, DocsCode, DocsLayout, DocsSection } from "@exp/site-shell"
import { DocsTable, IcArrow } from "../components/Content"
import { GuideFile } from "../components/GuideFile"
import { CORE_CATALOG as CATALOG_ID, FUNCTION_COUNT } from "../lib/catalog-facts"
import { CHECK_DIR, GUIDE_CHECK, GUIDES_NAV } from "../lib/content"
import { GUIDES, guidePath } from "../lib/guides"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

type Section = { id: string; label: string; body: ReactNode }

/** "Not published yet": the one honest install note every guide shares. */
const Unpublished = ({ children }: { children: ReactNode }) => (
  <DocsCallout kind="warn" title="Not on the registries yet">
    Nothing is published yet: every package publishes on the first <code>ui-v*</code> tag. {children}
  </DocsCallout>
)

const SDK_README = (path: string, label: string) => <a href={LINKS.source(path)}>{label}</a>

const GUIDE_SECTIONS: Record<string, Section[]> = {
  react: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            Two packages: <code>@exponential-at/ui</code> (the catalog, themes, reducer and host runtime) and{` `}
            <code>@exponential-at/ui-react</code> (the renderer). React 18 or 19; the renderer ships its own styles,
            so there is no stylesheet or Tailwind setup.
          </p>
          <DocsCode>{`npm install @exponential-at/ui @exponential-at/ui-react`}</DocsCode>
          <Unpublished>
            Until then, pack the tarballs from a checkout of the repo and install those:
          </Unpublished>
          <DocsCode>{`bun samples/exponential-ui/pack-local.ts      # → samples/exponential-ui/.packs/*.tgz
npm install ../exponential/samples/exponential-ui/.packs/exponential-at-ui-0.1.0.tgz \\
            ../exponential/samples/exponential-ui/.packs/exponential-at-ui-react-0.1.0.tgz`}</DocsCode>
          <GuideFile path="react/package.json" caption="the whole dependency list" />
        </>
      ),
    },
    {
      id: `paint`,
      label: `Paint a surface`,
      body: (
        <>
          <p>
            One <code>ExponentialHost</code> per app: it reads A2UI messages from a transport, routes them to
            surfaces and posts the user's actions back. <code>&lt;HostSurface&gt;</code> paints one surface by id,
            in a theme and a mode, with a fallback until the agent has created it.
          </p>
          <GuideFile path="react/src/App.tsx" />
          <p>
            The transport here reads a JSONL stream and posts actions to <code>/action</code>. Swap in{` `}
            <code>SseTransport</code>, <code>WebSocketTransport</code>, <code>McpTransport</code> (A2UI inside MCP tool
            results) or <code>MemoryTransport</code> (you push the messages) without touching the surface.{` `}
            <code>theme</code> takes a built-in id, a theme JSON or a resolved theme.
          </p>
        </>
      ),
    },
    {
      id: `without-host`,
      label: `Without a host`,
      body: (
        <>
          <p>
            When you already have the messages (a chat transcript, a test), skip the host: <code>useSurface</code>{` `}
            reduces messages into a tree and <code>&lt;ExponentialSurface&gt;</code> paints it. The plugin object
            takes icons, <code>onAction</code>, <code>onInput</code>, <code>openUrl</code> and extra functions.
          </p>
          <DocsCode language="tsx">{`const surface = useSurface({ surfaceId: "main" })
useEffect(() => messages.forEach(surface.apply), [messages])
return <ExponentialSurface surface={surface} theme="neutral" mode="light" host={{ onAction }} />`}</DocsCode>
          <p>
            More in the {SDK_README(`packages/exponential-ui-react/README.md`, `React renderer's README`)}: how a
            theme becomes scoped CSS, overlays, windowed lists and painter overrides.
          </p>
        </>
      ),
    },
  ],

  swiftui: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            The Swift package <code>ExponentialUI</code> (iOS 17, macOS 14) carries the painter and the Rust core as
            an xcframework, so your app never builds Rust. Published, it is a SwiftPM dependency on{` `}
            <code>github.com/Niach/exponential-ui-swift</code>.
          </p>
          <Unpublished>
            Until then, build the xcframework once from a checkout and depend on the package by path.
          </Unpublished>
          <DocsCode>{`bash apps/desktop/crates/exponential-ui-ffi/build-ios.sh   # → packages/exponential-ui-swift/Binaries/`}</DocsCode>
          <GuideFile path="swift/Package.swift" />
        </>
      ),
    },
    {
      id: `paint`,
      label: `Paint a surface`,
      body: (
        <>
          <p>
            The same shape as on the web: one <code>ExponentialHost</code> with a transport, a theme and a mode, and
            a <code>HostSurface</code> per surface id. The surface is as tall as its content, so you choose the
            scroller.
          </p>
          <GuideFile path="swift/Sources/GuideApp/ContentView.swift" />
        </>
      ),
    },
    {
      id: `host`,
      label: `Host plugin`,
      body: (
        <>
          <p>
            For lower-level use, <code>SurfaceModel</code> + <code>ExponentialSurface</code> take a{` `}
            <code>HostPlugin</code>; every method has a default and <code>ClosureHost</code> builds one from
            closures:
          </p>
          <ul>
            <li>
              <code>icon(name, size)</code>: the catalog's icon names to your views.
            </li>
            <li>
              <code>onAction</code> gets every A2UI action. <code>onInput</code> gets text edits (debounced{` `}
              <code>change</code> with a revision, <code>commit</code> on blur or Return).
            </li>
            <li>
              <code>openUrl</code>, <code>resolveUrl</code>, <code>fontFamily</code>, <code>markdown</code>.
            </li>
          </ul>
          <p>
            Register the font files a theme names with <code>ExponentialUI.registerFont(at:)</code>; switch themes
            with <code>ThemeHandle.load(json:)</code> and <code>setTheme</code>. Details in the{` `}
            {SDK_README(`packages/exponential-ui-swift/README.md`, `Swift package's README`)}.
          </p>
        </>
      ),
    },
  ],

  compose: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            One Maven artifact, <code>at.exponential:ui-compose</code> (minSdk 26). Its POM brings the primitives,
            Compose, coroutines and JNA, and the AAR carries the Rust core for arm64-v8a, armeabi-v7a and x86_64. R8
            needs no extra rules.
          </p>
          <Unpublished>Until it is on Maven Central, publish it to your local Maven repository from a checkout:</Unpublished>
          <DocsCode>{`bash apps/desktop/crates/exponential-ui-ffi/build-android.sh
cd packages/exponential-ui-compose
./gradlew :ui-compose:publishReleasePublicationToMavenLocal :ui-compose-primitives:publishReleasePublicationToMavenLocal`}</DocsCode>
          <p>
            Then add <code>mavenLocal()</code> to your repositories and the one dependency:
          </p>
          <GuideFile path="compose/app/build.gradle.kts" />
        </>
      ),
    },
    {
      id: `paint`,
      label: `Paint a surface`,
      body: (
        <>
          <p>
            One <code>ExponentialHost</code> per activity, connected in <code>onCreate</code> and disposed in{` `}
            <code>onDestroy</code>; <code>HostSurface</code> paints a surface by id. The emulator reaches the host
            machine's server at <code>10.0.2.2</code>.
          </p>
          <GuideFile path="compose/app/src/main/kotlin/at/exponential/guide/MainActivity.kt" />
        </>
      ),
    },
    {
      id: `host`,
      label: `Host plugin`,
      body: (
        <>
          <p>
            The lower-level <code>SurfaceModel</code> + <code>ExponentialSurface</code> take a{` `}
            <code>HostPlugin</code> (every member has a default; <code>ClosureHost</code> builds one from lambdas):
            icons painted under <code>LocalContentColor</code>, <code>onAction</code>, <code>onInput</code>{` `}
            (<code>Change</code> debounced with a revision, <code>Commit</code> on blur or IME Done),{` `}
            <code>openUrl</code>, <code>fontFamily</code>, <code>markdown</code>. Register theme fonts with{` `}
            <code>ExponentialUi.registerFont(family, FontFamily)</code>. Details in the{` `}
            {SDK_README(`packages/exponential-ui-compose/README.md`, `Compose library's README`)}.
          </p>
        </>
      ),
    },
  ],

  gpui: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            Two crates: <code>exponential-ui</code> (the core) and <code>exponential-ui-gpui</code> (the painter and
            host runtime). The painter depends on gpui and gpui-component as git dependencies, which crates.io
            refuses, so it stays a git or path dependency until both are published there.
          </p>
          <Unpublished>Until then, depend on both crates by path or git, as below.</Unpublished>
          <GuideFile path="gpui/Cargo.toml" caption="path dependencies standing in for crates.io" />
        </>
      ),
    },
    {
      id: `paint`,
      label: `Paint a surface`,
      body: (
        <>
          <p>
            The host is a gpui entity. <code>host.surface(id)</code> returns the surface's view once the agent has
            created it; it is full width and as tall as its content, so wrap it in your own scroller.
          </p>
          <GuideFile path="gpui/src/main.rs" />
        </>
      ),
    },
    {
      id: `host`,
      label: `Host plugin`,
      body: (
        <>
          <p>
            Underneath, a <code>SurfaceView</code> takes a <code>HostPlugin</code> implementation: catalog icon names
            to SVG asset paths, font families, actions and input. Register the theme's fonts with the text system
            before the first frame, and extension natives with <code>register_painter</code>. Details in the{` `}
            {SDK_README(`apps/desktop/crates/exponential-ui-gpui/README.md`, `gpui painter's README`)}.
          </p>
        </>
      ),
    },
  ],

  themes: [
    {
      id: `file`,
      label: `The theme file`,
      body: (
        <>
          <p>
            A theme is one JSON file. Start from a built-in with <code>extends</code> and override only what changes:
            colours per mode, a few tokens, and recipes for the parts you want to look different.
          </p>
          <GuideFile path="theme/brand.theme.json" />
          <p>
            The <code>$schema</code> gives your editor completion and validation (
            {SDK_README(`packages/exponential-ui/catalog/theme.schema.json`, `theme.schema.json`)}).
          </p>
        </>
      ),
    },
    {
      id: `tokens`,
      label: `Tokens`,
      body: (
        <>
          <p>
            Tokens are the named values every component uses: <code>$color.primary</code>, <code>$spacing.md</code>,{` `}
            <code>$radius.lg</code>, type sizes and line heights, control heights, shadows, borders and motion.
            Colours and shadows live under <code>modes.light</code> and <code>modes.dark</code>; everything else
            under <code>tokens</code>. Colours are <code>#rrggbb</code> or <code>#rrggbbaa</code> only; the theme
            builder converts oklch, hsl and rgb when it imports a shadcn <code>globals.css</code>.
          </p>
          <p>
            Fonts are referenced by family name. Each platform registers the files (the host's job), and{` `}
            <code>fonts</code> says what to fall back to.
          </p>
        </>
      ),
    },
    {
      id: `recipes`,
      label: `Recipes`,
      body: (
        <>
          <p>
            A recipe styles one <strong>part</strong> of a component (<code>Button.root</code>,{` `}
            <code>Switch.thumb</code>, <code>Tabs.tab</code>) as a list of rules. A rule applies when every entry of
            its <code>when</code> matches: recipe props like <code>variant</code> and <code>size</code>, and states (
            <code>hover</code>, <code>pressed</code>, <code>focus</code>, <code>disabled</code>,{` `}
            <code>checked</code>, <code>open</code>, <code>selected</code>).
          </p>
          <p>
            Rules merge by specificity: one point per condition, later wins on ties. A child theme's base rule
            therefore never hides its parent's hover or variant rules. The <a href="/themes/">Themes</a> page lists
            every component's parts, and the <a href="/themes/builder/">builder</a> shows them live.
          </p>
        </>
      ),
    },
    {
      id: `load`,
      label: `Load it`,
      body: (
        <>
          <p>
            Themes load at runtime, so the file can come from your bundle, a URL or a vapp package. Every loader
            flattens the <code>extends</code> chain and reports every problem at once (an unknown token, a key
            outside the whitelist, a misspelled part) without crashing.
          </p>
          <DocsTable
            head={[`Platform`, `Load`]}
            rows={[
              [`React`, <code key="c">{`<HostSurface theme={brandJson} … />`}</code>],
              [`TypeScript`, <code key="c">{`validateTheme(json) · loadTheme(json, { themes: BUILTIN_THEMES })`}</code>],
              [`SwiftUI`, <code key="c">{`ThemeHandle.load(json:) · model.setTheme(…)`}</code>],
              [`Compose`, <code key="c">{`ThemeHandle.load(json, parents) · model.setTheme(…)`}</code>],
              [`gpui / Rust`, <code key="c">{`exponential_ui::theme::load_theme(&json, &options)`}</code>],
            ]}
          />
        </>
      ),
    },
  ],

  extensions: [
    {
      id: `catalog`,
      label: `The catalog`,
      body: (
        <>
          <p>
            An extension catalog has its own id, extends the core and adds components. It may never shadow a core
            name. Each component has a model-facing description and props, like the core catalog. This one adds a
            native <code>Sparkline</code> and a macro <code>Metric</code>:
          </p>
          <GuideFile path="react/src/sparkline.extension.json" />
          <p>
            A <strong>macro</strong> expands into existing components in the reducer, on every platform, so{` `}
            <code>Metric</code> needs no painter anywhere. If your component can be built from core components, a
            macro is all you need. Every part of a macro stays themable through recipes.
          </p>
        </>
      ),
    },
    {
      id: `painter`,
      label: `A painter per platform`,
      body: (
        <>
          <p>
            A <strong>native</strong> needs a painter on each platform you ship. On React, a component receives the
            resolved props, the theme and the mode, and spreads <code>rootProps</code> on its root element:
          </p>
          <GuideFile path="react/src/Sparkline.tsx" />
          <DocsTable
            head={[`Platform`, `Register`]}
            rows={[
              [`React`, <code key="c">{`defineReactExtension({ catalog, components }) · registerExtension(…)`}</code>],
              [`SwiftUI`, <code key="c">{`ExponentialUI.register(extension: json, painters: [kind: painter])`}</code>],
              [`Compose`, <code key="c">{`ExponentialUi.register(extensionJson, mapOf(kind to painter))`}</code>],
              [`gpui`, <code key="c">{`view.register_painter("Sparkline", Box::new(MySparkline))`}</code>],
            ]}
          />
          <p>
            Native painters answer <code>measure</code> (the box the layout uses) and <code>paint</code> (the content
            inside the frame the core gives them). A painter for a <em>core</em> component name overrides that
            component, as long as it keeps the measure contract.
          </p>
        </>
      ),
    },
    {
      id: `agent`,
      label: `Teach the model`,
      body: (
        <>
          <p>
            Pass the extension to the prompt (<code>catalogPrompt({`{ extensions: [metricsCatalog] }`})</code>) and
            to the host (<code>new ExponentialHost({`{ extensions: [metricsCatalog] }`})</code>), so the client
            advertises its id and the model knows the new components. The Exponential app's issue rows, run rows and
            chips are an extension built exactly this way.
          </p>
        </>
      ),
    },
  ],

  "host-plugins": [
    {
      id: `plugin`,
      label: `The plugin`,
      body: (
        <>
          <p>
            A host plugin is what your app lends a surface: <strong>functions</strong> it may call,{` `}
            <strong>sources</strong> its data model may follow, and the <strong>policy</strong> that gates both. All
            three are plain options of <code>ExponentialHost</code>:
          </p>
          <GuideFile path="react/src/host-plugin.ts" />
        </>
      ),
    },
    {
      id: `functions`,
      label: `Functions`,
      body: (
        <>
          <p>
            A surface calls a function with <code>{`{"functionCall": {"call": "cart.add", "args": {…}}}`}</code> on
            an event. The catalog's {FUNCTION_COUNT} functions are built in; any other name must be registered, and passes the gate
            first: <strong>deny</strong> wins, then <strong>allow</strong>, then <strong>ask</strong> (your{` `}
            <code>onFunctionCall</code> consent hook), then <code>default</code>. Patterns are exact names or prefixes
            ending in <code>*</code>. A function may be async; the button stays pending until it settles.
          </p>
        </>
      ),
    },
    {
      id: `bindings`,
      label: `Data bindings`,
      body: (
        <>
          <p>
            <code>bindDataModel</code> points a path at a source URI, <code>&lt;scheme&gt;:&lt;name&gt;?k=v</code>.
            The host parses it and calls your resolver for the scheme, which emits values whenever it likes (a
            poll, a socket, a local store) and returns a cancel function. Each emit lands at the bound path, and the
            surface repaints.
          </p>
        </>
      ),
    },
    {
      id: `policy`,
      label: `URL and media policy`,
      body: (
        <>
          <p>
            <code>openUrl</code> and <code>Link</code> go through the URL policy: allowed schemes (default{` `}
            <code>https http mailto tel</code>), an optional host allowlist, relative URLs against a base. Media
            rules add headers to matching image requests, for example an authorization header for your own API and
            nothing for anyone else's.
          </p>
          <p>
            The rules are the same on every platform: they are written once in{` `}
            {SDK_README(`packages/exponential-ui/catalog/host.json`, `host.json`)} and replayed from fixtures by every
            renderer. The Exponential web app is a host built on exactly this API: <code>harness.*</code> functions,
            an <code>exp:</code> source scheme over its synced data, a consent card for <code>ask</code>.
          </p>
        </>
      ),
    },
  ],

  vapps: [
    {
      id: `package`,
      label: `The package`,
      body: (
        <>
          <p>
            A vapp is data, never code: templates (components, initial data, bindings), the functions it may call
            and a theme, in one JSON package. This one shows a greeting that follows a weather source and can toast:
          </p>
          <GuideFile path="react/src/hello.vapp.json" />
          <p>
            Its <code>functions</code> list is the policy for its surfaces: listed names are allowed, everything else
            is denied, whatever the host allows elsewhere. <code>theme</code> is a built-in id or a whole theme JSON.
          </p>
        </>
      ),
    },
    {
      id: `run`,
      label: `Run it`,
      body: (
        <>
          <p>
            <code>validatePackage</code> lists every problem with a path; <code>createVappHost</code> installs the
            package in a host that lends it the functions and sources it names. <code>open</code> applies a
            template onto a surface, which you paint like any other:
          </p>
          <GuideFile path="react/src/vapp.ts" />
        </>
      ),
    },
    {
      id: `embed`,
      label: `Embed and connect`,
      body: (
        <>
          <p>
            Because a package is data, the same file runs in every host: the web, the SwiftUI and Compose hosts, the
            gpui host. Against an Exponential instance, <code>ExponentialConnector</code> does the MCP OAuth grant
            (discovery, dynamic registration, PKCE) and serves the instance's issues, boards, teams and members as{` `}
            <code>exp:</code> sources, so a vapp can show live data the user has consented to. Hosted vapps, which
            run elsewhere and stream their UI, embed through the same <code>Transport</code> interface.
          </p>
          <p>
            The Exponential app ships its Devices screen this way: a package of templates that its web app and
            desktop IDE both render.
          </p>
        </>
      ),
    },
  ],

  agents: [
    {
      id: `prompt`,
      label: `The catalog prompt`,
      body: (
        <>
          <p>
            <code>catalogPrompt()</code> writes the system prompt that teaches a model the catalog: every component,
            prop and enum in one sentence each, the style whitelist and the token names. It is budgeted and gated in
            CI:
          </p>
          <DocsTable
            head={[`Variant`, `Components`, `Size (tokens, estimated)`]}
            rows={[
              [<code key="v">catalogPrompt()</code>, promptBudget.full.components, `≈ ${promptBudget.full.tokens.toLocaleString(`en-US`)}`],
              [<code key="v">{`{ lite: true }`}</code>, promptBudget.lite.components, `≈ ${promptBudget.lite.tokens.toLocaleString(`en-US`)}`],
              [<code key="v">{`{ terse: true }`}</code>, promptBudget.terse.components, `≈ ${promptBudget.terse.tokens.toLocaleString(`en-US`)}`],
            ]}
          />
          <p>
            Pass <code>extensions</code> to include yours. The <a href="/playground/">playground</a> shows the
            prompt in full.
          </p>
        </>
      ),
    },
    {
      id: `mcp`,
      label: `A2UI over MCP`,
      body: (
        <>
          <p>
            This MCP server has one tool whose description is the catalog prompt. A valid answer goes back as A2UI
            over MCP: a resource with the mime type <code>application/json+a2ui</code> holding the JSONL messages.
            The user's actions come back as a <code>tools/call</code> of <code>a2ui_event</code>.
          </p>
          <GuideFile path="agent/server.ts" />
          <p>
            On the client, <code>McpTransport</code> unpacks those resources into surfaces and sends actions back as
            tool calls, so any host from the render guides can show what this agent draws.
          </p>
        </>
      ),
    },
    {
      id: `validate`,
      label: `Validate`,
      body: (
        <>
          <p>
            Never trust a model's JSON. <code>reduceSurface(components, {`{ catalogId }`})</code> never throws: it
            returns the tree and a list of issues (an unknown component, a bad prop, a missing child). Send the
            issues back as a tool error and the model retries. For structured outputs or a validator of your own,{` `}
            <code>coreSchema()</code> is the whole catalog as JSON Schema, with the core catalog id{` `}
            <code>{CATALOG_ID}</code>.
          </p>
        </>
      ),
    },
  ],
}

function CiSection({ slug }: { slug: string }) {
  const check = GUIDE_CHECK[slug]
  const dir = CHECK_DIR[check]
  return (
    <>
      <p>
        Every file on this page is a real file in{` `}
        <a href={LINKS.source(`apps/ui-site/guides/${dir}`)}>apps/ui-site/guides/{dir}</a>. CI copies it into an empty
        directory and builds it from scratch, the way you would, so the example cannot drift from the SDK:
      </p>
      <DocsCode>{`bun apps/ui-site/guides/check.ts ${check}`}</DocsCode>
      <p>
        For a complete app on this platform, see the{` `}
        <a href={LINKS.source(`samples/exponential-ui`)}>samples</a>: four hosts rendering the same surface from a
        local A2UI server, in a third-party theme, with a custom extension component.
      </p>
    </>
  )
}

export default function GuidePage({ path }: PageProps) {
  const slug = path.split(`/`).filter(Boolean)[1] ?? ``
  const index = GUIDES.findIndex((guide) => guide.slug === slug)
  const guide = GUIDES[index]
  const body = GUIDE_SECTIONS[slug]
  if (!guide || !body) return <div className="shell">Unknown guide.</div>

  const sections = [...body, { id: `ci`, label: `Compiled in CI`, body: <CiSection slug={slug} /> }].map((s, i) => ({
    ...s,
    num: String(i + 1).padStart(2, `0`),
  }))
  const next = GUIDES[index + 1]

  return (
    <>
      <section className="docs-hero content-hero">
        <div className="shell docs-hero-content">
          <div className="section-eyebrow">
            Guide {index + 1} of {GUIDES.length}
          </div>
          <h1>{guide.title}</h1>
          <p>{guide.blurb}</p>
        </div>
      </section>

      <DocsLayout nav={GUIDES_NAV} title="Guides" sections={sections} currentPath={path}>
        {sections.map((s) => (
          <DocsSection key={s.id} id={s.id} num={s.num} label={s.label}>
            <h2>{s.label}</h2>
            {s.body}
          </DocsSection>
        ))}
        {next && (
          <a className="guide-next" href={guidePath(next.slug)}>
            <span className="guide-next-label">Next guide</span>
            <span className="guide-next-title">
              {next.title} <IcArrow />
            </span>
          </a>
        )}
      </DocsLayout>
    </>
  )
}
