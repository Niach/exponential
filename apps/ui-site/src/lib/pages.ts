/* Path → page module, by pattern, with NO data imports: the client entry
   uses this so the main bundle stays tiny and each page type is its own
   chunk (the manifest with every SEO entry, src/lib/routes.ts, is for the
   prerender and the nav). routes.test.ts holds the two in lockstep. */
import type { PageModule } from "./routes"

const PAGES: [RegExp, () => Promise<PageModule>][] = [
  [/^\/$/, () => import("../pages/Home")],
  [/^\/concepts\/$/, () => import("../pages/Concepts")],
  [/^\/components\/$/, () => import("../pages/ComponentsIndex")],
  [/^\/components\/[a-z0-9-]+\/$/, () => import("../pages/Component")],
  [/^\/themes\/$/, () => import("../pages/Themes")],
  [/^\/themes\/builder\/$/, () => import("../pages/ThemeBuilder")],
  [/^\/guides\/$/, () => import("../pages/Guides")],
  [/^\/guides\/[a-z0-9-]+\/$/, () => import("../pages/Guide")],
  [/^\/playground\/$/, () => import("../pages/Playground")],
  [/^\/conformance\/$/, () => import("../pages/Conformance")],
]

export function pageLoaderFor(path: string): (() => Promise<PageModule>) | undefined {
  const normalized = path.endsWith(`/`) ? path : `${path}/`
  return PAGES.find(([pattern]) => pattern.test(normalized))?.[1]
}

export const normalizePath = (path: string) => (path.endsWith(`/`) ? path : `${path}/`)
