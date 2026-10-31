/* Every guide, by the path's slug. The example files are the ones CI
   compiles from a fresh project (guides/**, guides/check.ts), shown through
   guideSource(), never pasted. */
import type { ReactNode } from "react"
import promptBudget from "@exponential_app/eui/fixtures/prompt-budget.json"
import { DocsCallout, DocsCode, DocsLayout, DocsSection } from "@exp/site-shell"
import { DocsTable, IcArrow } from "../components/Content"
import { GuideFile } from "../components/GuideFile"
import { LIMITS, MEDIA_LIMITS } from "@exponential_app/eui"
import { CORE_CATALOG as CATALOG_ID, FUNCTION_COUNT } from "../lib/catalog-facts"
import { CHECK_DIR, GUIDE_CHECK, GUIDES_NAV } from "../lib/content"
import { GUIDES, guidePath } from "../lib/guides"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

type Section = { id: string; label: string; body: ReactNode }

const SDK_README = (path: string, label: string) => <a href={LINKS.source(path)}>{label}</a>

const GUIDE_SECTIONS: Record<string, Section[]> = {
  react: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            Two packages: <code>@exponential_app/eui</code> (the catalog, themes, reducer and host runtime) and{` `}
            <code>@exponential_app/eui-react</code> (the renderer). React 19; the renderer ships its own styles,
            so there is no stylesheet or Tailwind setup.
          </p>
          <DocsCode>{`npm install @exponential_app/eui @exponential_app/eui-react`}</DocsCode>
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
    {
      id: `primitives`,
      label: `Primitives`,
      body: (
        <>
          <p>
            The app around a surface can use the same components: <code>@exponential_app/eui-react/primitives</code>{` `}
            is the shadcn set on Radix (button, dialog, select, table, command, toast, calendar…). Import its compiled
            stylesheet once. Inside a surface they wear its theme; outside one,{` `}
            <code>globalsCss(theme)</code> from <code>@exponential_app/eui/web</code> writes the theme as shadcn
            variables.
          </p>
          <DocsCode language="tsx">{`import "@exponential_app/eui-react/styles.css"
import { builtinTheme } from "@exponential_app/eui"
import { globalsCss } from "@exponential_app/eui/web"
import { Button, Dialog, DialogContent, DialogTitle, DialogTrigger, Toaster, toast } from "@exponential_app/eui-react/primitives"

const globals = globalsCss(builtinTheme("orange"))   // :root + .dark, put it in a <style>
<Dialog>
  <DialogTrigger asChild><Button>Open</Button></DialogTrigger>
  <DialogContent><DialogTitle>Saved views</DialogTitle></DialogContent>
</Dialog>
<Button variant="outline" onClick={() => toast.success("Saved")}>Save</Button>
<Toaster />`}</DocsCode>
        </>
      ),
    },
  ],

  solid: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            Two packages: <code>@exponential_app/eui</code> (the catalog, themes, reducer and host runtime) and{` `}
            <code>@exponential_app/eui-solid</code> (the renderer). Solid 1.9 with{` `}
            <code>vite-plugin-solid</code>; the surface injects its own styles, so there is no stylesheet setup.
          </p>
          <DocsCode>{`npm install @exponential_app/eui @exponential_app/eui-solid solid-js`}</DocsCode>
          <GuideFile path="solid/package.json" caption="the whole dependency list" />
          <GuideFile path="solid/vite.config.ts" />
        </>
      ),
    },
    {
      id: `paint`,
      label: `Paint a surface`,
      body: (
        <>
          <p>
            <code>createSurface</code> reduces A2UI messages into a tree and a data model (a Solid store);{` `}
            <code>&lt;ExponentialSurface&gt;</code> paints it in a theme and a mode. The plugin object takes icons,{` `}
            <code>onAction</code>, <code>onInput</code>, <code>openUrl</code> and extra functions.
          </p>
          <GuideFile path="solid/src/App.tsx" />
          <p>
            The DOM, the compiled stylesheet and the themes are the React renderer's, checked case by case in
            headless Chromium (DOM, computed styles, frames and pixels).
          </p>
        </>
      ),
    },
    {
      id: `with-host`,
      label: `With a host`,
      body: (
        <>
          <p>
            One <code>ExponentialHost</code> per app reads messages from a transport and posts actions back;{` `}
            <code>&lt;HostSurface&gt;</code> paints one of its surfaces by id, with a fallback until the agent has
            created it.
          </p>
          <DocsCode language="tsx">{`const host = new ExponentialHost({ transport: new JsonlStreamTransport({ url: "/a2ui.jsonl", postUrl: "/action" }) })
onMount(() => host.connect()); onCleanup(() => host.close())
return <HostSurface host={host} surfaceId="main" theme="orange" fallback={<p>Waiting for the agent…</p>} />`}</DocsCode>
          <p>
            Extensions use <code>defineSolidExtension</code>; the hooks have Solid names (<code>createSurface</code>,{` `}
            <code>createHostSurface</code>, <code>createField</code>). More in the{` `}
            {SDK_README(`packages/exponential-ui-solid/README.md`, `Solid renderer's README`)}.
          </p>
          <GuideFile path="solid/src/TrendLine.tsx" />
        </>
      ),
    },
    {
      id: `define`,
      label: `A surface in TypeScript`,
      body: (
        <>
          <p>
            <code>defineSurface</code> from <code>@exponential_app/eui</code> works unchanged in a Solid project: the
            typed tree becomes the same messages an agent sends, and <code>createSurface</code> applies them. Nothing
            Solid-specific is needed; the file compiles under this project&apos;s tsconfig and runs under Node.
          </p>
          <GuideFile path="solid/src/defined.ts" caption="the typed DSL, as in the TypeScript guide" />
          <GuideFile path="solid/src/Defined.tsx" caption="painted by the Solid renderer" />
        </>
      ),
    },
    {
      id: `primitives`,
      label: `Primitives`,
      body: (
        <>
          <p>
            <code>@exponential_app/eui-solid/primitives</code> is the React primitive set on Solid, module for module, with the same DOM and
            classes (one set of recipes, checked against React in headless Chromium).{` `}
            <code>@exponential_app/eui-solid/styles.css</code> is the same stylesheet as React's;{` `}
            <code>globalsCss(theme)</code> from <code>@exponential_app/eui/web</code> themes them outside a surface.
            The names differ in three ways: <code>class</code> for <code>className</code>, <code>as</code> for{` `}
            <code>asChild</code>, and <code>create*</code> for the <code>use*</code> hooks.
          </p>
          <DocsCode language="tsx">{`import "@exponential_app/eui-solid/styles.css"
import { builtinTheme } from "@exponential_app/eui"
import { globalsCss } from "@exponential_app/eui/web"
import { Button, Dialog, DialogContent, DialogTitle, DialogTrigger, Toaster, toast } from "@exponential_app/eui-solid/primitives"

const globals = globalsCss(builtinTheme("orange"))   // :root + .dark, put it in a <style>
<Dialog>
  <DialogTrigger as={Button}>Open</DialogTrigger>
  <DialogContent><DialogTitle>Saved views</DialogTitle></DialogContent>
</Dialog>
<Button variant="outline" class="ml-2" onClick={() => toast.success("Saved")}>Save</Button>
<Toaster />`}</DocsCode>
        </>
      ),
    },
  ],

  element: [
    {
      id: `install`,
      label: `Install`,
      body: (
        <>
          <p>
            Two packages: <code>@exponential_app/eui</code> (the catalog, themes, reducer and host runtime) and{` `}
            <code>@exponential_app/eui-element</code> (the custom element <code>{`<exponential-surface>`}</code>). No
            framework in the page and no framework plugin in Vite; the element brings the Solid painter and its own
            styles.
          </p>
          <DocsCode>{`npm install @exponential_app/eui @exponential_app/eui-element`}</DocsCode>
          <GuideFile path="element/package.json" caption="the whole dependency list" />
          <GuideFile path="element/vite.config.ts" />
          <p>
            Without a bundler, <code>@exponential_app/eui-element/standalone</code> is one self-contained ES module:
          </p>
          <DocsCode language="html">{`<script type="module" src="/vendor/exponential-ui-element/standalone.js"></script>`}</DocsCode>
        </>
      ),
    },
    {
      id: `paint`,
      label: `Paint a surface`,
      body: (
        <>
          <p>
            Importing <code>@exponential_app/eui-element/define</code> registers the element. Its attributes are the
            surface's options (<code>theme</code>, <code>mode</code>, <code>density</code>, <code>locale</code>,{` `}
            <code>direction</code>, <code>width</code>, <code>surface-id</code>); the host box is{` `}
            <code>display: contents</code>, so the surface root is the box.
          </p>
          <GuideFile path="element/index.html" />
          <p>
            <code>apply(message)</code> takes A2UI v0.9 server messages; the element keeps the tree and the data
            model.
          </p>
          <GuideFile path="element/src/main.ts" />
          <p>
            Everything the user does arrives as a bubbling <code>CustomEvent</code>: <code>xui-action</code>,{` `}
            <code>xui-input</code> and <code>xui-upload</code> (<code>detail.waitUntil(promise)</code> keeps the
            control pending), <code>xui-function-call</code> (<code>detail.respond(value)</code>),{` `}
            <code>xui-open-url</code> (cancelable), <code>xui-ready</code>, <code>xui-status</code>,{` `}
            <code>xui-paint-error</code>. The DOM, the compiled stylesheet and the themes are the React renderer's,
            checked case by case in headless Chromium (DOM, computed styles, frames and pixels).
          </p>
        </>
      ),
    },
    {
      id: `with-host`,
      label: `With a host`,
      body: (
        <>
          <p>
            <code>src</code> builds an <code>ExponentialHost</code> on the matching transport (<code>transport</code>{` `}
            = <code>jsonl</code>, <code>sse</code> or <code>ws</code>; <code>ws:</code> URLs pick WebSocket),
            connects when the element is attached and closes when it is removed; <code>post</code> is where actions
            go.
          </p>
          <DocsCode language="html">{`<exponential-surface src="/a2ui.jsonl" post="/action" surface-id="main" theme="orange"></exponential-surface>`}</DocsCode>
          <p>
            Or hand it your own host, a theme JSON and a host plugin as properties:
          </p>
          <DocsCode language="ts">{`el.host = new ExponentialHost({ transport: new JsonlStreamTransport({ url: "/a2ui.jsonl", postUrl: "/action" }) })
el.theme = brandJson
el.plugin = { icons: iconsFromSvg((name) => svgFor(name)), openUrl: (url) => window.open(url) }`}</DocsCode>
          <p>
            Extensions use <code>defineElementExtension</code>: a painter is a plain function from a context (props,
            theme, mode, <code>applyRootProps</code>, <code>children()</code>) to an <code>Element</code>, run again
            when they change. More in the{` `}
            {SDK_README(`packages/exponential-ui-element/README.md`, `web component's README`)}.
          </p>
          <GuideFile path="element/src/TrendLine.ts" />
        </>
      ),
    },
    {
      id: `define`,
      label: `A surface in TypeScript`,
      body: (
        <>
          <p>
            <code>defineSurface</code> from <code>@exponential_app/eui</code> works unchanged in a plain Vite project:
            the typed tree becomes the same messages an agent sends, and <code>apply()</code> on the element takes
            them. Nothing element-specific is needed; the file compiles under this project&apos;s tsconfig and runs
            under Node.
          </p>
          <GuideFile path="element/src/defined.ts" caption="the typed DSL, as in the TypeScript guide" />
          <GuideFile path="element/src/mount-defined.ts" caption="painted by <exponential-surface>" />
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
            an xcframework, so your app never builds Rust. It is a SwiftPM dependency on{` `}
            <code>github.com/Niach/exponential-ui-swift</code>, whose tag <code>0.3.0</code> pins the xcframework of
            that release by checksum:
          </p>
          <DocsCode language="swift">{`dependencies: [
    .package(url: "https://github.com/Niach/exponential-ui-swift", from: "0.3.0"),
],
targets: [
    .executableTarget(name: "App", dependencies: [.product(name: "ExponentialUI", package: "exponential-ui-swift")]),
]`}</DocsCode>
          <GuideFile
            path="swift/Package.swift"
            caption="the guide's own manifest builds the monorepo's package by path, so CI checks it without a download"
          />
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
            One Maven artifact, <code>at.exponential:eui-compose</code> (minSdk 26). Its POM brings the primitives,
            Compose, coroutines and JNA, and the AAR carries the Rust core for arm64-v8a, armeabi-v7a and x86_64. R8
            needs no extra rules.
          </p>
          <DocsCode language="kotlin">{`dependencies {
    implementation("at.exponential:eui-compose:0.3.0")
}`}</DocsCode>
          <p>
            It resolves from <code>mavenCentral()</code>; the whole dependency block of the guide's app:
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
            Two crates: <code>eui-core</code> (the core) and <code>eui-gpui</code> (the painter and
            host runtime). The painter depends on gpui and gpui-component as git dependencies, which crates.io
            refuses, so an app takes both crates from the release tag. Your app links the same gpui as the painter:
            the same gpui-component rev, zed locked at the SDK's rev and the ztracing patch, all copied from one tag
            (the {SDK_README(`apps/desktop/crates/eui-gpui/README.md`, `gpui revision policy`)}). gpui
            needs Rust 1.96: pin it in <code>rust-toolchain.toml</code>.
          </p>
          <DocsCode language="toml">{`[dependencies]
eui-core = { git = "https://github.com/Niach/exponential", tag = "eui-v0.3.0" }
eui-gpui = { git = "https://github.com/Niach/exponential", tag = "eui-v0.3.0" }
gpui = { version = "0.2.2", git = "https://github.com/zed-industries/zed" }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "da4f93696dc2b2b4d91bcc42412b9053a3d24de8" }

[patch."https://github.com/zed-industries/zed"]
ztracing = { git = "https://github.com/Niach/exponential", tag = "eui-v0.3.0" }
ztracing_macro = { git = "https://github.com/Niach/exponential", tag = "eui-v0.3.0" }`}</DocsCode>
          <DocsCode language="sh">{`cargo update gpui --precise cc053a4a6fa2fd0e8793201ed9099466af1be0b1`}</DocsCode>
          <GuideFile path="gpui/Cargo.toml" caption="path dependencies standing in for the git ones" />
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
            {SDK_README(`apps/desktop/crates/eui-gpui/README.md`, `gpui painter's README`)}.
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
            therefore never hides its parent's hover or variant rules. The <a href="/ui/themes/">Themes</a> page lists
            every component's parts, and the <a href="/ui/themes/builder/">builder</a> shows them live.
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
            Themes load at runtime, so the file can come from your bundle, a URL or an app package. Every loader
            flattens the <code>extends</code> chain and reports every problem at once (an unknown token, a key
            outside the whitelist, a misspelled part) without crashing.
          </p>
          <DocsTable
            head={[`Platform`, `Load`]}
            rows={[
              [`React`, <code key="c">{`<HostSurface theme={brandJson} … />`}</code>],
              [`Solid`, <code key="c">{`<ExponentialSurface theme={brandJson} … />`}</code>],
              [`Web component`, <code key="c">{`<exponential-surface theme="orange"> · el.theme = brandJson`}</code>],
              [`TypeScript`, <code key="c">{`validateTheme(json) · loadTheme(json, { themes: BUILTIN_THEMES })`}</code>],
              [`SwiftUI`, <code key="c">{`ThemeHandle.load(json:) · model.setTheme(…)`}</code>],
              [`Compose`, <code key="c">{`ThemeHandle.load(json, parents) · model.setTheme(…)`}</code>],
              [`gpui / Rust`, <code key="c">{`eui_core::theme::load_theme(&json, &options)`}</code>],
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
            native <code>TrendLine</code> and a macro <code>Metric</code>:
          </p>
          <GuideFile path="react/src/trendline.extension.json" />
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
          <GuideFile path="react/src/TrendLine.tsx" />
          <DocsTable
            head={[`Platform`, `Register`]}
            rows={[
              [`React`, <code key="c">{`defineReactExtension({ catalog, components }) · registerExtension(…)`}</code>],
              [`Solid`, <code key="c">{`defineSolidExtension({ catalog, components }) · registerExtension(…)`}</code>],
              [`Web component`, <code key="c">{`defineElementExtension({ catalog, components }) · el.extensions = […]`}</code>],
              [`SwiftUI`, <code key="c">{`ExponentialUI.register(extension: json, painters: [kind: painter])`}</code>],
              [`Compose`, <code key="c">{`ExponentialUi.register(extensionJson, mapOf(kind to painter))`}</code>],
              [`gpui`, <code key="c">{`view.register_painter("TrendLine", Box::new(MyTrendLine))`}</code>],
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
            advertises its id and the model knows the new components. The Exponential app's issue rows, session rows and
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
            an event. The catalog's {FUNCTION_COUNT} functions are built in; any other name is namespaced (<code>cart.add</code>), must be registered, and passes the gate
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
            <code>https http mailto tel</code>; <code>schemes: ["https"]</code> for https only), an optional host
            allowlist, relative URLs against a base. Media rules add headers to matching image requests, for example
            an authorization header for your own API and nothing for anyone else's.
          </p>
          <p>
            Private hosts are denied by default, for links and media alike: <code>localhost</code>, <code>*.local</code>
            , loopback, link-local (the cloud metadata address included) and the private IPv4 and IPv6 ranges. A host
            opts names back in with <code>allowPrivate: ["localhost"]</code> (a dev setup) or <code>["*"]</code>; a
            <code>hosts</code> allowlist alone does not. The check is on the literal address after URL normalization
            and never resolves a name. The whole model, limits included, is on the{` `}
            <a href="/ui/guides/security/">security page</a>.
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

  apps: [
    {
      id: `package`,
      label: `The package`,
      body: (
        <>
          <p>
            An app is data, never code: templates (components, initial data, bindings), the functions it may call
            and a theme, in one JSON package. This one shows a greeting that follows a weather source and can toast:
          </p>
          <GuideFile path="react/src/hello.app.json" />
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
            <code>validatePackage</code> lists every problem with a path; <code>createAppHost</code> installs the
            package in a host that lends it the functions and sources it names. <code>open</code> applies a
            template onto a surface, which you paint like any other:
          </p>
          <GuideFile path="react/src/app-host.ts" />
        </>
      ),
    },
    {
      id: `records`,
      label: `Records: an app's own data`,
      body: (
        <>
          <p>
            An app may keep data of its own. The manifest declares every <code>records</code> kind with a small JSON
            Schema subset (string, number, integer, boolean, array; enum, min, max, maxLength, a date format);{` `}
            <code>validateRecordSchema</code> checks a kind at install and <code>validateRecord</code> a record before
            a host stores it. The rows come back through the neutral binding source{` `}
            <code>records:&lt;kind&gt;?query</code> (with filters, a sort, a sum and a month view), and the app writes
            them with <code>records.upsert</code> / <code>records.delete</code>. Exponential also accepts its own
            spellings, <code>exp:records?kind=…</code> and <code>harness.records.*</code>, which this example uses. This is the Working
            hours app: a month of entries, a target, and a switcher that re-follows the
            binding with the built-in <code>bind</code> action, whose <code>params</code> are appended to the source
            as its query:
          </p>
          <GuideFile path="react/src/working-hours.app.json" />
          <p>
            A host that lends a package records keeps them wherever it likes (Exponential: one synced table, one shape
            per team); the SDK fixes the manifest block, the two validators, the store interface (
            <code>RecordStore</code>, <code>MemoryRecordStore</code>, <code>recordsHost</code>) and the{` `}
            <code>bind</code> action, so the same package runs in every host.
          </p>
          <p>
            Package format version 2 adds three more optional blocks: <code>tools</code> (MCP servers or scripts, run
            only after the host showed the <code>toolConsent</code> card), <code>extensions</code> (custom component
            catalogs with a minimum version, checked against the host&apos;s) and a signed package file (an{` `}
            <code>author</code> and an Ed25519 <code>signature</code>: <code>signPackageFile</code>,{` `}
            <code>verifyPackageFile</code>). The spec is{` `}
            {SDK_README(`packages/exponential-ui/docs/package-format.md`, `package-format.md`)}; more on{` `}
            <a href="/apps/">eApps</a>.
          </p>
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
            <code>exp:</code> sources, so an app can show live data the user has consented to. Hosted apps, which
            run elsewhere and stream their UI, embed through the same <code>Transport</code> interface.
          </p>
          <p>
            Exponential's team-made apps work this way: packages of templates that its web app and desktop IDE
            both render. The SDK keeps a sample package as a benchmark.
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
              [<code key="v">{`{ sections: ["rules", "index", "functions", "style"] }`}</code>, `${promptBudget.index.components} (one line each)`, `≈ ${promptBudget.index.tokens.toLocaleString(`en-US`)}`],
              [<code key="v">{`{ sections: ["rules"] }`}</code>, `–`, `≈ ${promptBudget.rules.tokens.toLocaleString(`en-US`)}`],
              [<code key="v">componentPrompt(name)</code>, `1`, `≈ ${promptBudget.perComponent.meanTokens} on average`],
            ]}
          />
          <p>
            The prompt is sectioned: <code>sections</code> picks what to print, <code>only</code> narrows the component
            blocks to the ones a task needs, and <code>componentPrompt(name)</code> is one component&apos;s block on
            demand. A host serves the rules and the index first and answers a model&apos;s request for a component with
            its block, which is how the <a href="/ui/eval/">ergonomics eval</a> measured the cheapest prompt. Pass{` `}
            <code>extensions</code> to include yours. The <a href="/ui/playground/">playground</a> shows every variant in full.
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
  security: [
    {
      id: `model`,
      label: `What a surface can do`,
      body: (
        <>
          <p>
            A surface is <strong>data</strong>: A2UI messages describing a tree of catalog components, a data model
            and the actions wired to them. There is no script, no HTML passthrough, no iframe or webview, and no
            component that downloads code. The expression language is a closed operator set over the data model; the
            client functions are the catalog's built-ins plus whatever the host registers by name. Themes are data
            too: colours as hex, fonts by family (system or host-supplied), recipes as style objects. Extensions and
            painter overrides are host code you compile in, never something a surface can send.
          </p>
          <p>
            Every renderer replays the same fixtures, so what a surface can and cannot do is identical on React,
            SolidJS, the <code>{`<exponential-surface>`}</code> web component, SwiftUI, Compose and gpui. The web
            renderers share one URL and media filter (the web layer of <code>@exponential_app/eui</code>); the web
            component's <code>src</code> and <code>post</code> pass it too, before a transport exists, and its
            standalone bundle carries it. The Rust core behind the three native painters is fuzzed nightly; the
            TypeScript reference runs property tests on the same modules.
          </p>
          <DocsTable
            head={[`A surface can`, `A surface cannot`]}
            rows={[
              [`Compose any catalog component with props, slots, styles and conditions`, `Run code of any kind, or load a component that is not in the catalog or a registered extension`],
              [`Bind props to its data model and to host sources the host registered (and a package listed)`, `Read anything outside its own data model, or follow a source its package did not list`],
              [`Call the built-in client functions, and host functions by name through the policy gate`, `Call a host function the host did not register, or one the policy denies`],
              [`Link to and load media from URLs the URL and media policy allow`, `Open javascript:, file: or data: links, or reach private, loopback and link-local hosts unless the host opts them in`],
              [`Ask for a theme by id or send one as JSON`, `Make a renderer fetch a font or image from somewhere the media policy denies, or crash it with a bad theme (the default stands in)`],
            ]}
          />
        </>
      ),
    },
    {
      id: `host`,
      label: `The host's responsibilities`,
      body: (
        <>
          <p>
            The host is the boundary. Four things are yours to decide, all plain options of <code>ExponentialHost</code>
            {` `}(see <a href="/ui/guides/host-plugins/">Write a host plugin</a>):
          </p>
          <DocsTable
            head={[`Hook`, `What it decides`, `Default`]}
            rows={[
              [<code key="f">functions</code>, `Which registered host functions a surface may call: deny wins, then allow, then ask (your consent hook), then default; a package's functions list narrows it further.`, `allow for a registered name; built-ins always run`],
              [<code key="s">sources</code>, `Which binding sources a package's surfaces may follow (sources on the package), matched as scheme:name or a prefix.`, `any registered scheme`],
              [<code key="u">urls</code>, `Allowed schemes, an optional host allowlist, the base for relative URLs, and allowPrivate for the private hosts you opt back in.`, `https http mailto tel; private hosts denied`],
              [<code key="m">media</code>, `The same for every src, plus the header rules that add auth to your own API; mediaStreamUrl for authed video.`, `https http data; private hosts denied; file only when listed`],
            ]}
          />
          <p>
            The consent hook is where a person says yes or no to an <strong>ask</strong> decision. The Exponential app's
            card names the tool and shows its arguments; a destructive-sounding call takes the destructive answer.
            Make yours show the same two things: a name alone is not enough to decide.
          </p>
          <DocsCallout kind="warn" title="Private hosts and DNS">
            The private-host rule is on the literal address after URL normalization (<code>2130706433</code> and{` `}
            <code>0x7f.1</code> are loopback too). It never resolves a name: a public name that resolves to a private
            address is your network's job (egress rules, a filtering resolver). Opt in by name, never with{` `}
            <code>["*"]</code> outside a dev setup.
          </DocsCallout>
        </>
      ),
    },
    {
      id: `limits`,
      label: `The limits`,
      body: (
        <>
          <p>
            Every core enforces the same input limits; past one, the input is refused with an issue and no work is
            done. Media loads are capped before anything decodes.
          </p>
          <DocsTable
            head={[`Limit`, `Value`, `What happens past it`]}
            rows={[
              [`Components per surface`, String(LIMITS.maxComponents), `the rest is dropped, one issue`],
              [`Nesting depth`, String(LIMITS.maxDepth), `a deeper node is the Unknown placeholder`],
              [`One server message`, `${Math.round(LIMITS.maxMessageBytes / 1048576)} MiB`, `INVALID_MESSAGE`],
              [`Template items`, String(LIMITS.maxTemplateItems), `the rest is not rendered`],
              [`A data pointer`, `${LIMITS.maxPointerBytes} bytes, ${LIMITS.maxPointerSegments} segments`, `the write is refused`],
              [`An image`, `${Math.round(MEDIA_LIMITS.maxBytes / 1048576)} MiB, ${MEDIA_LIMITS.timeoutMs / 1000} s, ${Math.round(MEDIA_LIMITS.maxPixels / 1048576)} Mpx across every frame`, `fails like a 404; the fallback paints`],
              [`Video / audio without mediaStreamUrl (React, SolidJS, the web component)`, `the same bytes and seconds for the whole download`, `fails like a 404`],
            ]}
          />
          <p>
            The Rust core's recursive passes grow their stack on the heap when a host thread runs low, so an iOS
            secondary thread never overflows. A panic inside the core never crosses the FFI: a call that cannot throw
            answers a safe empty value, a call that can throw answers <code>UiError.Panic</code>, and the surface
            stays usable. Deep or wide inputs are the fuzzers' bread and butter; the nightly job runs every target with
            address sanitizer.
          </p>
        </>
      ),
    },
    {
      id: `report`,
      label: `How to report`,
      body: (
        <>
          <p>
            Found a way for a surface to do something from the <em>A surface cannot</em> column above? Please do not open a public
            issue. Write to <a href="mailto:security@exponential.at">security@exponential.at</a> with the message that
            triggers it (JSON or JSONL) and the renderer and version. You get an answer within three working days, a
            fix lands as a patch release of every affected package, and the fixture that locks it joins the
            conformance suite so it can never regress on any platform.
          </p>
          <p>
            Hardening that is not a vulnerability (a limit you would tune, a policy option you miss) is a regular
            issue on the <a href={LINKS.repo}>repository</a>.
          </p>
        </>
      ),
    },
  ],
}

const DEFINE_SECTIONS: Section[] = [
  {
    id: `why`,
    label: `Why a DSL`,
    body: (
      <>
        <p>
          A surface is a flat list of JSON components: fine for a model, tedious for a person. <code>defineSurface</code>{` `}
          takes a nested, typed tree instead. Every component is a factory on <code>ui</code> whose props autocomplete
          from the catalog (<code>ComponentProps</code>, generated from the same catalog the renderers read), so a
          misspelt prop or a wrong enum is a type error, and the result is the same flat list an agent would send.
        </p>
        <GuideFile path="define/package.json" caption="one dependency" />
      </>
    ),
  },
  {
    id: `write`,
    label: `Write a surface`,
    body: (
      <>
        <p>
          <code>ui.Stack(props, children)</code> nests; the third argument carries what is not a prop: an explicit{` `}
          <code>id</code>, <code>style</code>, <code>on</code>, <code>visible</code>, <code>slots</code>. A List renders one
          template per item of a data path: <code>{`{ template, path, key }`}</code> as its children. <code>bind(path)</code>{` `}
          reads the data model (relative inside a template), <code>call(name, args)</code> computes, <code>event(name)</code>{` `}
          sends an action to you and <code>run(name, args)</code> runs a client function.
        </p>
        <GuideFile path="define/surface.ts" />
      </>
    ),
  },
  {
    id: `validate`,
    label: `Validate and send`,
    body: (
      <>
        <p>
          <code>defineSurface</code> never throws on a bad surface: it returns <code>issues</code> from the same reducer
          the renderers use (a literal-only prop given a binding, an unknown icon, a missing required prop), the flat{` `}
          <code>components</code>, the <code>messages</code> (createSurface, updateComponents, one updateDataModel per
          data path) and their <code>jsonl</code>. Hand the messages to any host from the render guides, or embed them in
          an app template. Ids are assigned top-down (<code>root</code>, then <code>row-1</code>, <code>row-2</code>…)
          unless you name a node.
        </p>
        <DocsCode language="ts">{`const surface = defineSurface(tree, { surfaceId: "working-hours", data: { "/entries": rows } })
if (surface.issues.length) throw new Error(surface.issues.map((i) => i.id + ": " + i.message).join("\n"))
host.receive(surface.messages) // or: transport.send(surface.jsonl)`}</DocsCode>
      </>
    ),
  },
]
GUIDE_SECTIONS.define = DEFINE_SECTIONS

function CiSection({ slug }: { slug: string }) {
  const check = GUIDE_CHECK[slug]
  const dir = CHECK_DIR[check]
  return (
    <>
      <p>
        Every file on this page is a real file in{` `}
        <a href={LINKS.source(`apps/marketing/ui/guides/${dir}`)}>apps/marketing/ui/guides/{dir}</a>. CI copies it into an empty
        directory and builds it from scratch, the way you would, so the example cannot drift from the SDK:
      </p>
      <DocsCode>{`bun apps/marketing/ui/guides/check.ts ${check}`}</DocsCode>
      <p>
        For a complete app on this platform, see the{` `}
        <a href={LINKS.source(`samples/exponential-ui`)}>samples</a>: six hosts rendering the same surface from a
        local A2UI server, in a third-party theme, with a custom extension component.
      </p>
    </>
  )
}

export default function GuidePage({ path }: PageProps) {
  const slug = path.split(`/`).filter(Boolean)[2] ?? ``
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
          <h1>{guide.title}</h1>
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
