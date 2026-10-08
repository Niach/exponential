/* THE page manifest of ui.exponential.at: every path, its page module, and
   its SEO (title, description, OG image, JSON-LD). The client entry, the
   prerender (scripts/prerender.tsx: one dist/<path>/index.html per entry,
   sitemap.xml, llms.txt) and the nav all read it, so a page cannot exist in
   one and not the other. */
import type { ComponentType } from "react"
import { breadcrumb, type PageSeo } from "@exp/site-shell/seo"
import { COMPONENT_DOCS, componentPath, componentSlug } from "./catalog"
import { GUIDES, guidePath } from "./guides"
import { SITE_NAME, SITE_ORIGIN } from "./site"

export type PageProps = { path: string }
export type PageModule = { default: ComponentType<PageProps> }

export interface Route extends PageSeo {
  load: () => Promise<PageModule>
}

const OG = `/og/og-ui.png`
const crumbs = (...items: { name: string; path: string }[]) =>
  breadcrumb(SITE_ORIGIN, [{ name: SITE_NAME, path: `/` }, ...items])

/* `sources` feed the sitemap's lastmod (git log, relative to apps/ui-site). */
const route = (
  path: string,
  load: () => Promise<PageModule>,
  title: string,
  description: string,
  sources: string[],
  jsonLd?: PageSeo[`jsonLd`]
): Route => ({
  path,
  htmlFile: path === `/` ? `index.html` : `${path.slice(1)}index.html`,
  load,
  title,
  description,
  sources,
  ogImage: OG,
  ...(jsonLd ? { jsonLd } : {}),
})

/* The model-facing sentence, then what the page holds (within a snippet). */
const componentDescription = (sentence: string) => {
  const full = `${sentence} A live React render, the A2UI JSON, the props and native shots on web, iOS, Android and desktop.`
  return full.length <= 220 ? full : sentence
}

const CATALOG_SOURCES = [`../../packages/exponential-ui/catalog`, `../../packages/exponential-ui/docs`]

export const ROUTES: readonly Route[] = [
  route(
    `/`,
    () => import("../pages/Home"),
    `Exponential UI · generative UI, native on every platform`,
    `An open A2UI SDK: an agent sends a surface as JSON, our renderers paint it natively in React, SwiftUI, Jetpack Compose and gpui. Themable at runtime, extensible, Apache-2.0.`,
    [`src/pages/Home.tsx`, `src/sdk`],
    [
      {
        "@context": `https://schema.org`,
        "@type": `SoftwareSourceCode`,
        name: SITE_NAME,
        description: `Open A2UI renderers for React, SwiftUI, Jetpack Compose and gpui with a shared core catalog, runtime themes, extensions and a host API.`,
        codeRepository: `https://github.com/Niach/exponential`,
        programmingLanguage: [`TypeScript`, `Swift`, `Kotlin`, `Rust`],
        license: `https://www.apache.org/licenses/LICENSE-2.0`,
        url: `${SITE_ORIGIN}/`,
      },
    ]
  ),
  route(
    `/concepts/`,
    () => import("../pages/Concepts"),
    `Concepts · ${SITE_NAME}`,
    `A2UI in short (surfaces, components, the data model, actions, functions), the core catalog and A2UI basic, extensions, themes, host plugins and vapps.`,
    [`src/pages/Concepts.tsx`],
    crumbs({ name: `Concepts`, path: `/concepts/` })
  ),
  route(
    `/components/`,
    () => import("../pages/ComponentsIndex"),
    `Components · ${SITE_NAME}`,
    `The ${COMPONENT_DOCS.length} components of the core catalog, each rendered live by the React renderer and photographed on web, iOS, Android and desktop.`,
    [`src/pages/ComponentsIndex.tsx`, ...CATALOG_SOURCES],
    crumbs({ name: `Components`, path: `/components/` })
  ),
  ...COMPONENT_DOCS.map((doc) =>
    route(
      componentPath(doc),
      () => import("../pages/Component"),
      `${doc.name} · Components · ${SITE_NAME}`,
      componentDescription(doc.description),
      [`src/pages/Component.tsx`, ...CATALOG_SOURCES, `../../shots/${doc.specimenId}`],
      crumbs({ name: `Components`, path: `/components/` }, { name: doc.name, path: `/components/${componentSlug(doc)}/` })
    )
  ),
  route(
    `/themes/`,
    () => import("../pages/Themes"),
    `Themes · ${SITE_NAME}`,
    `Runtime themes: tokens and component recipes in one JSON file every renderer loads. The built-in neutral, exponential and playful themes, side by side.`,
    [`src/pages/Themes.tsx`, `../../packages/exponential-ui/themes`],
    crumbs({ name: `Themes`, path: `/themes/` })
  ),
  route(
    `/themes/builder/`,
    () => import("../pages/ThemeBuilder"),
    `Theme builder · ${SITE_NAME}`,
    `Build an Exponential UI theme in the browser: pick a base, edit tokens and recipes over a live preview, import a shadcn globals.css, export the JSON.`,
    [`src/pages/ThemeBuilder.tsx`, `../../packages/exponential-ui/builder`],
    crumbs({ name: `Themes`, path: `/themes/` }, { name: `Builder`, path: `/themes/builder/` })
  ),
  route(
    `/guides/`,
    () => import("../pages/Guides"),
    `Guides · ${SITE_NAME}`,
    `Render A2UI in React, SwiftUI, Compose and gpui; write a theme, an extension or a host plugin; build a vapp; connect an agent. Every example compiles in CI.`,
    [`src/pages/Guides.tsx`, `src/lib/guides.ts`],
    crumbs({ name: `Guides`, path: `/guides/` })
  ),
  ...GUIDES.map((guide) =>
    route(
      guidePath(guide.slug),
      () => import("../pages/Guide"),
      `${guide.title} · Guides · ${SITE_NAME}`,
      guide.blurb,
      [`src/pages/Guide.tsx`, `guides`],
      crumbs({ name: `Guides`, path: `/guides/` }, { name: guide.title, path: guidePath(guide.slug) })
    )
  ),
  route(
    `/playground/`,
    () => import("../pages/Playground"),
    `Playground · ${SITE_NAME}`,
    `Paste A2UI JSON or JSONL and see it render live in any theme, share it by URL, and read the system prompt the catalog generates for a model.`,
    [`src/pages/Playground.tsx`, `src/sdk`],
    crumbs({ name: `Playground`, path: `/playground/` })
  ),
  route(
    `/conformance/`,
    () => import("../pages/Conformance"),
    `Conformance · ${SITE_NAME}`,
    `What it means for a renderer to be Exponential UI conformant, the fixture suites every renderer runs, and how a third-party renderer runs them and checks its report.`,
    [`src/pages/Conformance.tsx`, `../../packages/exponential-ui/conformance`],
    crumbs({ name: `Conformance`, path: `/conformance/` })
  ),
]

export const routeFor = (path: string): Route | undefined => {
  const normalized = path.endsWith(`/`) ? path : `${path}/`
  return ROUTES.find((r) => r.path === normalized)
}
