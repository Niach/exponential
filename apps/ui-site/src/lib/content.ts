/* The content pages' shared data: the packages and their honest release
   status (packages/exponential-ui/release/README.md), the four renderers,
   the docs-style navs, and which guides/check.ts check compiles each guide. */
import type { DocsNavEntry } from "@exp/site-shell"
import { GUIDES, guidePath } from "./guides"

/** The docs sidebar of Concepts and Conformance: the site's reading order. */
export const LEARN_NAV: DocsNavEntry[] = [
  { path: `/`, label: `Overview` },
  { path: `/concepts/`, label: `Concepts` },
  { path: `/components/`, label: `Components` },
  { path: `/themes/`, label: `Themes` },
  { path: `/guides/`, label: `Guides` },
  { path: `/playground/`, label: `Playground` },
  { path: `/conformance/`, label: `Conformance` },
]

/** The guides sidebar: the index, then every guide in nav order. */
export const GUIDES_NAV: DocsNavEntry[] = [
  { path: `/guides/`, label: `All guides` },
  ...GUIDES.map((g) => ({ path: guidePath(g.slug), label: g.title, blurb: g.blurb })),
]

export interface Renderer {
  platform: string
  framework: string
  pkg: string
  /** Where its source lives in the repo. */
  source: string
  guide: string
  note: string
}

export const RENDERERS: readonly Renderer[] = [
  {
    platform: `Web`,
    framework: `React`,
    pkg: `@exponential-at/ui-react`,
    source: `packages/exponential-ui-react`,
    guide: `react`,
    note: `The reference renderer: real CSS on shadcn and Radix, themes compiled to scoped variables, the TypeScript reducer.`,
  },
  {
    platform: `iOS + macOS`,
    framework: `SwiftUI`,
    pkg: `ExponentialUI`,
    source: `packages/exponential-ui-swift`,
    guide: `swiftui`,
    note: `Native SwiftUI views at the frames the Rust core lays out; text measured in batches through the UniFFI facade.`,
  },
  {
    platform: `Android`,
    framework: `Jetpack Compose`,
    pkg: `at.exponential:ui-compose`,
    source: `packages/exponential-ui-compose`,
    guide: `compose`,
    note: `The Compose twin of the SwiftUI painter on the same Rust core. No WebView, Android 8 and later.`,
  },
  {
    platform: `Desktop`,
    framework: `gpui`,
    pkg: `exponential-ui-gpui`,
    source: `apps/desktop/crates/exponential-ui-gpui`,
    guide: `gpui`,
    note: `The Rust core in-process, painted by gpui on macOS, Windows and Linux. No HTML, no webview.`,
  },
]

export interface PackageRow {
  name: string
  registry: string
  what: string
  /** Until the first `ui-v*` tag publishes it: how to install from the repo. */
  fromRepo: string
}

export const PACKAGES: readonly PackageRow[] = [
  {
    name: `@exponential-at/ui`,
    registry: `npm`,
    what: `The catalog, themes, TypeScript reducer, host API and conformance suite.`,
    fromRepo: `bun samples/exponential-ui/pack-local.ts packs the tarball`,
  },
  {
    name: `@exponential-at/ui-react`,
    registry: `npm`,
    what: `The React renderer.`,
    fromRepo: `the same tarballs, installed with npm`,
  },
  {
    name: `exponential-ui`,
    registry: `crates.io`,
    what: `The Rust core: reducer, themes, taffy layout, layers, lists.`,
    fromRepo: `a path or git dependency on the repo`,
  },
  {
    name: `exponential-ui-ffi`,
    registry: `bundled`,
    what: `The UniFFI facade. It ships inside the Swift and Kotlin artifacts, so you never install it on its own.`,
    fromRepo: `built by build-ios.sh / build-android.sh`,
  },
  {
    name: `exponential-ui-gpui`,
    registry: `crates.io (later)`,
    what: `The gpui painter. It stays a git dependency until gpui itself is on crates.io.`,
    fromRepo: `a path or git dependency on the repo`,
  },
  {
    name: `ExponentialUI`,
    registry: `SwiftPM`,
    what: `The SwiftUI painter (iOS 17, macOS 14), from github.com/Niach/exponential-ui-swift.`,
    fromRepo: `the package by path, after build-ios.sh`,
  },
  {
    name: `at.exponential:ui-compose`,
    registry: `Maven Central`,
    what: `The Compose painter (minSdk 26), with ui-compose-primitives.`,
    fromRepo: `publishToMavenLocal, then mavenLocal()`,
  },
]

/** The guides/check.ts check that compiles a guide's example from a fresh project. */
export const GUIDE_CHECK: Record<string, string> = {
  react: `react`,
  swiftui: `swift`,
  compose: `compose`,
  gpui: `gpui`,
  themes: `theme`,
  extensions: `react`,
  "host-plugins": `react`,
  vapps: `react`,
  agents: `agent`,
}

/** The directory under apps/ui-site/guides/ a check builds. */
export const CHECK_DIR: Record<string, string> = {
  react: `react`,
  swift: `swift`,
  compose: `compose`,
  gpui: `gpui`,
  theme: `theme`,
  agent: `agent`,
}
