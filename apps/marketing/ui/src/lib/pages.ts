/* Path → page module, by pattern, with NO data imports: the client entry
   uses this so the main bundle stays tiny and each page type is its own
   chunk (the manifest with every SEO entry, src/lib/routes.ts, is for the
   prerender and the nav). routes.test.ts holds the two in lockstep. */
import type { PageModule } from "./routes"

const PAGES: [RegExp, () => Promise<PageModule>][] = [
  [/^\/ui\/$/, () => import("../pages/Home")],
  [/^\/ui\/concepts\/$/, () => import("../pages/Concepts")],
  [/^\/ui\/components\/$/, () => import("../pages/ComponentsIndex")],
  [/^\/ui\/components\/[a-z0-9-]+\/$/, () => import("../pages/Component")],
  [/^\/ui\/themes\/$/, () => import("../pages/Themes")],
  [/^\/ui\/themes\/builder\/$/, () => import("../pages/ThemeBuilder")],
  [/^\/ui\/guides\/$/, () => import("../pages/Guides")],
  [/^\/ui\/guides\/[a-z0-9-]+\/$/, () => import("../pages/Guide")],
  [/^\/ui\/playground\/$/, () => import("../pages/Playground")],
  [/^\/ui\/conformance\/$/, () => import("../pages/Conformance")],
  [/^\/ui\/eval\/$/, () => import("../pages/Eval")],
  [/^\/ui\/numbers\/$/, () => import("../pages/Numbers")],
  /* NEW-15: the Talk section (src/talk) rides the same app. */
  [/^\/talk\/$/, () => import("../talk/TalkHome")],
  [/^\/talk\/how-it-works\/$/, () => import("../talk/HowItWorks")],
  [/^\/talk\/install\/$/, () => import("../talk/Install")],
  [/^\/talk\/guides\/$/, () => import("../talk/Guides")],
  [/^\/talk\/guides\/[a-z0-9-]+\/$/, () => import("../talk/Guide")],
  [/^\/talk\/api\/$/, () => import("../talk/Api")],
  [/^\/talk\/numbers\/$/, () => import("../talk/Numbers")],
  [/^\/talk\/licences\/$/, () => import("../talk/Licences")],
  /* EXP-1421: the one-page libs. */
  [/^\/pilot\/$/, () => import("../libs/Pilot")],
  [/^\/net\/$/, () => import("../libs/Net")],
  [/^\/apps\/$/, () => import("../libs/Apps")],
  [/^\/mcp\/$/, () => import("../libs/Mcp")],
  [/^\/agent\/$/, () => import("../libs/Agent")],
  [/^\/core\/$/, () => import("../libs/Core")],
]

export function pageLoaderFor(path: string): (() => Promise<PageModule>) | undefined {
  const normalized = path.endsWith(`/`) ? path : `${path}/`
  return PAGES.find(([pattern]) => pattern.test(normalized))?.[1]
}

export const normalizePath = (path: string) => (path.endsWith(`/`) ? path : `${path}/`)
