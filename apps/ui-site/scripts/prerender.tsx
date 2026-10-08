/* Postbuild prerender + SEO for ui.exponential.at (after `vite build`). The
   build emits ONE dist/index.html (the client entry routes by path); this
   writes it as dist/<path>/index.html for every route of the manifest
   (src/lib/routes.ts), then the shared engine (@exp/site-shell/prerender,
   the same exponential.at runs) renders each page into its root marker,
   injects the SEO head block and writes sitemap.xml + llms.txt. */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { prerenderSite } from "@exp/site-shell/prerender"
import { App } from "../src/app"
import { COMPONENT_DOCS, componentPath } from "../src/lib/catalog"
import { GUIDES, guidePath } from "../src/lib/guides"
import { ROUTES, type PageModule } from "../src/lib/routes"
import { SITE_NAME, SITE_ORIGIN } from "../src/lib/site"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), `..`)
const DIST = resolve(ROOT, `dist`)

const template = readFileSync(resolve(DIST, `index.html`), `utf8`)
const modules = new Map<string, PageModule>()
for (const route of ROUTES) {
  modules.set(route.path, await route.load())
  const file = resolve(DIST, route.htmlFile)
  mkdirSync(dirname(file), { recursive: true })
  writeFileSync(file, template)
}

prerenderSite({
  root: ROOT,
  origin: SITE_ORIGIN,
  siteName: SITE_NAME,
  pages: [...ROUTES],
  render: (path) => <App path={path} page={modules.get(path)!} />,
  llms: {
    summary: `Exponential UI is an open (Apache-2.0) generative-UI SDK: an agent sends an A2UI surface as JSON and native renderers paint it in React, SwiftUI, Jetpack Compose and gpui, from one core catalog, with runtime themes, extensions and a shared host API.`,
    intro: [
      `The SDK lives in the Exponential monorepo (https://github.com/Niach/exponential): packages/exponential-ui (catalog, themes, host API, conformance), packages/exponential-ui-react, packages/exponential-ui-swift, packages/exponential-ui-compose and apps/desktop/crates/exponential-ui{,-ffi,-gpui}. Exponential itself (https://exponential.at) is built on it.`,
    ],
    sections: [
      { heading: `Start here`, paths: [`/`, `/concepts/`, `/playground/`, `/conformance/`] },
      { heading: `Guides`, paths: [`/guides/`, ...GUIDES.map((g) => guidePath(g.slug))] },
      { heading: `Themes`, paths: [`/themes/`, `/themes/builder/`] },
      { heading: `Components`, paths: [`/components/`, ...COMPONENT_DOCS.map(componentPath)] },
    ],
    extras: [
      {
        heading: `Machine-readable`,
        items: [
          { label: `core.catalog.json`, url: `https://github.com/Niach/exponential/blob/master/packages/exponential-ui/catalog/core.catalog.json`, description: `The core catalog: every component, prop and model-facing description.` },
          { label: `core.schema.json`, url: `https://github.com/Niach/exponential/blob/master/packages/exponential-ui/catalog/core.schema.json`, description: `The catalog as JSON Schema in A2UI's catalog shape.` },
          { label: `theme.schema.json`, url: `https://github.com/Niach/exponential/blob/master/packages/exponential-ui/catalog/theme.schema.json`, description: `The runtime theme format as JSON Schema.` },
        ],
      },
    ],
  },
})
