/* The guides, in nav order. Each page shows the example files the CI compiles
   from a fresh project (guides/check.ts); their text reaches the pages through
   src/generated/guide-sources.ts (scripts/guide-sources.ts). */
export interface GuideEntry {
  slug: string
  title: string
  blurb: string
}

export const GUIDES: readonly GuideEntry[] = [
  { slug: `react`, title: `Render A2UI in React`, blurb: `Install the two npm packages, connect a host, paint a surface with <HostSurface>.` },
  { slug: `swiftui`, title: `Render A2UI in SwiftUI`, blurb: `Add the Swift package, hand the painter a host, show a surface on iOS and macOS.` },
  { slug: `compose`, title: `Render A2UI in Compose`, blurb: `Add the Maven artifact, connect a host, paint a surface in Jetpack Compose.` },
  { slug: `gpui`, title: `Render A2UI in gpui`, blurb: `Add the crates, give the SurfaceView a host plugin, paint natively on the desktop.` },
  { slug: `themes`, title: `Write your own theme`, blurb: `Tokens, recipes and an extends chain: one JSON file every renderer loads at runtime.` },
  { slug: `extensions`, title: `Write an extension`, blurb: `Custom components: a catalog schema plus a painter per platform, or a macro only.` },
  { slug: `host-plugins`, title: `Write a host plugin`, blurb: `Functions behind a policy gate, data bindings by source URI, the URL and media policy.` },
  { slug: `vapps`, title: `Build and embed a vapp`, blurb: `A declarative package of templates, functions and a theme, run in any host.` },
  { slug: `agents`, title: `Connect an agent`, blurb: `The catalog prompt, A2UI over MCP, and validating what the model sends.` },
]

export const guidePath = (slug: string) => `/guides/${slug}/`
