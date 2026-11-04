import { useEffect, useState } from "react"
import { createFileRoute, Navigate, redirect } from "@tanstack/react-router"
import { SkeletonRows } from "@exp/ui"
import { SHARE_TARGETS } from "@exp/domain-contract/share-targets"
import { ShareTargetsPage } from "@/components/share-targets-page"
import { pageTitle } from "@/lib/page-title"
import {
  holdPendingShare,
  loadPendingShare,
  shareIsEmpty,
  type PendingShare,
} from "@/lib/share/pending-share"
import { trpc } from "@/lib/trpc-client"

// EXP-1635: `/share`, where the PWA's `share_target` lands (share-targets.json
// step 1). The service worker has stashed the pending share; a browser without
// one brings its text in the search params (`lib/share/share-target-fallback.ts`).
// With nothing shared there is nothing to pick: home.

export interface ShareSearch {
  title?: string
  text?: string
  url?: string
}

const str = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

async function resolveShare({ title, text, url }: ShareSearch): Promise<PendingShare | null> {
  if (title || text || url) {
    const share = { title: title ?? ``, text: text ?? ``, url: url ?? ``, files: [] }
    holdPendingShare(share)
    return share
  }
  const pending = await loadPendingShare()
  return pending && !shareIsEmpty(pending) ? pending : null
}

export const Route = createFileRoute(`/_authenticated/share`)({
  head: () => ({ meta: [{ title: pageTitle(SHARE_TARGETS.copy.title) }] }),
  validateSearch: (search: Record<string, unknown>): ShareSearch => ({
    title: str(search.title),
    text: str(search.text),
    url: str(search.url),
  }),
  beforeLoad: async () => {
    const { team } = await trpc.teams.getDefault.query()
    if (!team) throw redirect({ to: `/onboarding` })
    return { defaultTeamSlug: team.slug }
  },
  component: ShareRoute,
})

function ShareRoute() {
  const search = Route.useSearch()
  const { defaultTeamSlug, user } = Route.useRouteContext()
  // undefined = still reading the stash.
  const [share, setShare] = useState<PendingShare | null | undefined>(undefined)
  useEffect(() => {
    let current = true
    void resolveShare(search).then((resolved) => {
      if (current) setShare(resolved)
    })
    return () => {
      current = false
    }
  }, [search.title, search.text, search.url])

  if (share === undefined) return <SkeletonRows className="mx-auto w-full max-w-2xl px-4 py-6" />
  if (share === null) return <Navigate to="/" replace />
  return <ShareTargetsPage share={share} defaultTeamSlug={defaultTeamSlug} me={user.id} />
}
