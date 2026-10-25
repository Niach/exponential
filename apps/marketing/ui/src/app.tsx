/* One page = the shell around the route's page module; the client entry and
   the prerender both render through this, so the markup they produce is the
   same and hydration matches. */
import { Shell } from "./components/Shell"
import type { PageModule } from "./lib/routes"

export function App({ path, page }: { path: string; page: PageModule }) {
  const Page = page.default
  return (
    <Shell path={path}>
      <Page path={path} />
    </Shell>
  )
}
