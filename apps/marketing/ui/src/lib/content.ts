/* The content pages' shared data: the packages and their honest release
   status (packages/exponential-ui/release/README.md), the six renderers,
   the docs-style navs, and which guides/check.ts check compiles each guide. */
import type { DocsNavEntry } from "@exp/site-shell"
import { GUIDES, guidePath } from "./guides"
import { otherLibrariesNav } from "./libraries"

/** The docs sidebar of Concepts, Conformance and Numbers: the site's reading order. */
export const LEARN_NAV: DocsNavEntry[] = [
  { path: `/ui/`, label: `Overview` },
  { path: `/ui/concepts/`, label: `Concepts` },
  { path: `/ui/components/`, label: `Components` },
  { path: `/ui/themes/`, label: `Themes` },
  { path: `/ui/guides/`, label: `Guides` },
  { path: `/ui/playground/`, label: `Playground` },
  { path: `/ui/conformance/`, label: `Conformance` },
  { path: `/ui/eval/`, label: `Ergonomics eval` },
  { path: `/ui/numbers/`, label: `Numbers` },
  ...otherLibrariesNav(`eUI`),
]

/** The guides sidebar: the index, then every guide in nav order. */
export const GUIDES_NAV: DocsNavEntry[] = [
  { path: `/ui/guides/`, label: `All guides` },
  ...GUIDES.map((g) => ({ path: guidePath(g.slug), label: g.title, blurb: g.blurb })),
]

export interface Renderer {
  platform: string
  framework: string
  pkg: string
  /** Where its source lives in the repo. */
  source: string
  guide: string
}

export const RENDERERS: readonly Renderer[] = [
  {
    platform: `Web`,
    framework: `React`,
    pkg: `@exponential_app/ui-react`,
    source: `packages/exponential-ui-react`,
    guide: `react`,
  },
  {
    platform: `Web`,
    framework: `SolidJS`,
    pkg: `@exponential_app/ui-solid`,
    source: `packages/exponential-ui-solid`,
    guide: `solid`,
  },
  {
    platform: `Web`,
    framework: `Web Components`,
    pkg: `@exponential_app/ui-element`,
    source: `packages/exponential-ui-element`,
    guide: `element`,
  },
  {
    platform: `iOS + macOS`,
    framework: `SwiftUI`,
    pkg: `ExponentialUI`,
    source: `packages/exponential-ui-swift`,
    guide: `swiftui`,
  },
  {
    platform: `Android`,
    framework: `Jetpack Compose`,
    pkg: `at.exponential:ui-compose`,
    source: `packages/exponential-ui-compose`,
    guide: `compose`,
  },
  {
    platform: `Desktop`,
    framework: `gpui`,
    pkg: `exponential-ui-gpui`,
    source: `apps/desktop/crates/exponential-ui-gpui`,
    guide: `gpui`,
  },
]

export interface PackageRow {
  name: string
  registry: string
  what: string
  /** The one-line install from its registry (every package publishes together from a `ui-v*` tag). */
  install: string
}

export const PACKAGES: readonly PackageRow[] = [
  {
    name: `@exponential_app/ui`,
    registry: `npm`,
    what: `The catalog, themes, TypeScript reducer, host API and conformance suite.`,
    install: `npm install @exponential_app/ui`,
  },
  {
    name: `@exponential_app/ui-react`,
    registry: `npm`,
    what: `The React renderer and its shadcn primitive set, with a compiled stylesheet.`,
    install: `npm install @exponential_app/ui-react`,
  },
  {
    name: `@exponential_app/ui-solid`,
    registry: `npm`,
    what: `The SolidJS renderer and the same primitives, with the same DOM and stylesheet as React.`,
    install: `npm install @exponential_app/ui-solid`,
  },
  {
    name: `@exponential_app/ui-element`,
    registry: `npm`,
    what: `The custom element <exponential-surface>: the Solid painter inside a web component, no framework in the page.`,
    install: `npm install @exponential_app/ui-element`,
  },
  {
    name: `exponential-ui`,
    registry: `crates.io`,
    what: `The Rust core: reducer, themes, taffy layout, layers, lists.`,
    install: `cargo add exponential-ui`,
  },
  {
    name: `exponential-ui-ffi`,
    registry: `bundled`,
    what: `The UniFFI facade. It ships inside the Swift and Kotlin artifacts, so you never install it on its own.`,
    install: `nothing to install`,
  },
  {
    name: `exponential-ui-gpui`,
    registry: `git`,
    what: `The gpui painter. It stays a git dependency until gpui itself is on crates.io.`,
    install: `a git dependency on github.com/Niach/exponential`,
  },
  {
    name: `ExponentialUI`,
    registry: `SwiftPM`,
    what: `The SwiftUI painter (iOS 17, macOS 14), from github.com/Niach/exponential-ui-swift.`,
    install: `.package(url: "https://github.com/Niach/exponential-ui-swift", from: "0.2.0")`,
  },
  {
    name: `at.exponential:ui-compose`,
    registry: `Maven Central`,
    what: `The Compose painter (minSdk 26), with ui-compose-primitives.`,
    install: `implementation("at.exponential:ui-compose:0.2.0")`,
  },
]

/** The guides/check.ts check that compiles a guide's example from a fresh project. */
export const GUIDE_CHECK: Record<string, string> = {
  react: `react`,
  solid: `solid`,
  element: `element`,
  swiftui: `swift`,
  compose: `compose`,
  gpui: `gpui`,
  themes: `theme`,
  extensions: `react`,
  "host-plugins": `react`,
  apps: `react`,
  agents: `agent`,
  define: `define`,
  security: `react`,
}

/** The directory under apps/marketing/ui/guides/ a check builds. */
export const CHECK_DIR: Record<string, string> = {
  react: `react`,
  solid: `solid`,
  element: `element`,
  swift: `swift`,
  compose: `compose`,
  gpui: `gpui`,
  theme: `theme`,
  agent: `agent`,
  define: `define`,
}
