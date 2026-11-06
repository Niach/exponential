/* /desktop/setup.sh: Exponential Desktop's one-line repository setup
   (`curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh`). The
   script lives in apps/ede/packaging/repo/setup.sh; the build fills in
   the host, key and suites from repo.env (the package host serves the same). */
import type { APIRoute } from "astro"
import script from "../../../../ede/packaging/repo/setup.sh?raw"
import { renderSetupScript } from "../../lib/desktop-repo"

export const GET: APIRoute = () => new Response(renderSetupScript(script), { headers: { "Content-Type": `text/plain; charset=utf-8` } })
