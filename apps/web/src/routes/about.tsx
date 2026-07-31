import { createFileRoute } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"

const LicensesIcon = conceptIcon(`settings-licenses`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)
const DownloadIcon = conceptIcon(`ui-download`)

const SOURCE_URL = `https://github.com/Niach/exponential`
const LICENSE_URL = `https://github.com/Niach/exponential/blob/master/LICENSE`

// PUBLIC on purpose (EXP-262): the same MIT/BSD-licensed JS bundle is served
// to signed-out visitors (/auth/login, /support/$token, /invite/*), so the
// notice must accompany that distribution too — gating it behind a session
// would be wrong on the merits and would break external/self-host links.
export const Route = createFileRoute(`/about`)({
  ssr: false,
  // The notice is FETCHED, never imported: importing it (or converting it to
  // a generated TS module) would put the ~600 KB blob into a JS chunk. It
  // ships as a Nitro static file (apps/web/public/NOTICES.txt, EXP-375).
  loader: async () => {
    try {
      const res = await fetch(`/NOTICES.txt`)
      if (!res.ok) return { notices: null }
      return { notices: await res.text() }
    } catch {
      return { notices: null }
    }
  },
  component: AboutPage,
})

function AboutPage() {
  const { notices } = Route.useLoaderData()

  return (
    <div className="min-h-screen bg-background px-4 py-10">
      <div className="mx-auto flex max-w-3xl flex-col gap-6">
        <div className="flex flex-col gap-1">
          <h1 className="text-2xl font-semibold">About</h1>
          <p className="text-sm text-muted-foreground">
            Exponential: a real-time issue tracker.
          </p>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" asChild>
            <a href={SOURCE_URL} target="_blank" rel="noreferrer">
              <ExternalLinkIcon className="mr-2 size-4" />
              Source code
            </a>
          </Button>
          <Button variant="outline" size="sm" asChild>
            <a href={LICENSE_URL} target="_blank" rel="noreferrer">
              <ExternalLinkIcon className="mr-2 size-4" />
              License (Apache-2.0)
            </a>
          </Button>
        </div>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <LicensesIcon className="size-4" />
              Third-party licenses
            </CardTitle>
            <CardDescription>
              Exponential is built with open-source software. These licenses
              cover the components bundled in this build.
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-3">
            {notices === null ? (
              <p className="text-sm text-muted-foreground">
                The notices file could not be loaded. You can download it
                directly below.
              </p>
            ) : (
              /* One <pre>, never a per-line .map() and never the TipTap
                 editor — 600 KB through either blocks the main thread. Raw
                 overflow-y-auto is the app's own layout idiom (whats-new). */
              <pre className="max-h-[60vh] overflow-y-auto rounded-md border bg-muted/30 p-4 text-xs whitespace-pre-wrap">
                {notices}
              </pre>
            )}
            <div>
              <Button variant="ghost" size="sm" asChild>
                <a href="/NOTICES.txt" download>
                  <DownloadIcon className="mr-2 size-4" />
                  Download NOTICES.txt
                </a>
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
