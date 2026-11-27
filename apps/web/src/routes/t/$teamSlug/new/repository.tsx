import { useEffect, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { REPO_PAGE_COPY as COPY } from "@exp/domain-contract/repo-page"
import { ChoiceTiles } from "@exp/ui"
import { NewPageHeading } from "@/components/team/new-page-heading"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { AddEcodeRepositoryForm } from "@/components/repo/add-ecode-repository"
import { GithubRepoPicker } from "@/components/github-repo-picker"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { repositoryCollection } from "@/lib/collections"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { pageTitle } from "@/lib/page-title"

// #EXP-2171 (repo-page.json NEW › REPOSITORY, sidebar-tree.json THE NEW
// LIST): New › Repository opens this PAGE: two tiles, Host on Exponential
// first (the hosted form + Description and Run CI), then Link from GitHub
// (the connect list). A created repository opens its page. Owner-only like
// the other creation pages: anyone else lands on Home.
export const Route = createFileRoute(`/t/$teamSlug/new/repository`)({
  head: () => ({ meta: [{ title: pageTitle(COPY.new.title) }] }),
  component: NewRepositoryRoute,
})

type Provider = `ecode` | `github`

function NewRepositoryRoute() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { team, permissions, resolved } = useSettingsPage(teamSlug)
  const [provider, setProvider] = useState<Provider>(`ecode`)
  const [linking, setLinking] = useState(false)
  const refused = resolved && !permissions.isOwner
  useEffect(() => {
    if (refused) void navigate({ to: `/home`, replace: true })
  }, [refused, navigate])
  if (!team || !resolved || refused) return null

  const open = async (repository: { id: string; slug: string }) => {
    // The sidebar row and the page read the synced row: wait for it.
    await repositoryCollection.preload().catch(() => undefined)
    void navigate({ to: `/t/$teamSlug/repos/$name`, params: { teamSlug: team.slug, name: repository.slug }, replace: true })
  }

  return (
    <div className={`mx-auto w-full max-w-2xl space-y-6 p-4 sm:p-6 ${PAGE_END_CLEARANCE}`} data-testid="new-repository-page">
      <NewPageHeading teamSlug={teamSlug}>{COPY.new.title}</NewPageHeading>
      <ChoiceTiles
        data-testid="new-repository-provider"
        columns={2}
        value={provider}
        onChange={(value) => setProvider(value === `github` ? `github` : `ecode`)}
        options={[
          { value: `ecode`, label: COPY.new.hosted, description: COPY.new.hostedHint },
          { value: `github`, label: COPY.new.github, description: COPY.new.githubHint },
        ]}
      />
      {provider === `ecode` ? (
        <AddEcodeRepositoryForm page teamId={team.id} onCancel={() => window.history.back()} onCreated={open} />
      ) : (
        <GithubRepoPicker
          teamId={team.id}
          variant="plain"
          listClassName="max-h-[min(28rem,60dvh)]"
          onSelect={(repo) => {
            if (linking) return
            setLinking(true)
            trpc.repositories.add
              .mutate({ teamId: team.id, fullName: repo.fullName, defaultBranch: repo.defaultBranch, private: repo.private }, SKIP_ERROR_TOAST)
              .then(({ repository }) => (repository ? open(repository) : undefined))
              .catch((err: unknown) => failedAction(`add`, `the repository`, err))
              .finally(() => setLinking(false))
          }}
        />
      )}
    </div>
  )
}
