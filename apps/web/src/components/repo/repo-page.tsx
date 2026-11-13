import { useCallback, useEffect, useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  REPO_PAGE_COPY as COPY,
  REPO_TABS,
  repoBranches,
  repoPrTree,
  type RepoPr,
  type RepoTab,
} from "@exp/domain-contract/repo-page"
import {
  Button,
  EmptyState,
  GlassSectionHeader,
  GlassToggleRow,
  Input,
  ListRow,
  Pill,
  PrList,
  Prompt,
  SETTINGS_LIST_CLASS,
  SegmentedControl,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SkeletonRows,
  conceptIcon,
} from "@exp/ui"
import { pullRequestCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { PullRequestStatusPill } from "@/components/session-merge-button"
import { checksDotColor, checksLabel } from "@/components/repo/checks-dot"

const UiBackIcon = conceptIcon(`ui-back`)
const RepositoriesIcon = conceptIcon(`settings-repositories`)

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
type Repo = RepoList[number]
type Settings = Awaited<ReturnType<typeof trpc.repositories.settings.query>>

/** The repo a `$name` route param names: the full name's last segment. */
export function repoByName(repos: RepoList, name: string): Repo | null {
  return repos.find((repo) => repo.fullName.split(`/`).pop() === name) ?? null
}

/** `Badge` naming the provider (repo-page.json `copy.provider`). */
export function ProviderBadge({ provider }: { provider: string | null | undefined }) {
  return (
    <Pill size="sm" className="shrink-0" data-testid="repo-provider">
      {provider === `ecode` ? COPY.provider.ecode : COPY.provider.github}
    </Pill>
  )
}

/** The repo page (#EXP-1616, repo-page.json): AppBar + pill Tabs over Pull
 *  requests · Runs · Branches · Settings. */
export function RepoPage({
  teamId,
  teamSlug,
  name,
  tab,
  isOwner,
  isMember,
}: {
  teamId: string
  teamSlug: string
  name: string
  tab: RepoTab
  isOwner: boolean
  isMember: boolean
}) {
  const navigate = useNavigate()
  const [repos, setRepos] = useState<RepoList | null>(null)
  useEffect(() => {
    let live = true
    trpc.repositories.list
      .query({ teamId }, SKIP_ERROR_TOAST)
      .then((rows) => live && setRepos(rows))
      .catch((err: unknown) => {
        if (!live) return
        setRepos([])
        failedAction(`load`, `the repository`, err)
      })
    return () => {
      live = false
    }
  }, [teamId])
  const repo = repos ? repoByName(repos, name) : null

  const { data: prRows = [] } = useLiveQuery(
    (q) => (repo ? q.from({ prs: pullRequestCollection }).where(({ prs }) => eq(prs.repositoryId, repo.id)) : undefined),
    [repo?.id]
  )

  const setTab = (next: RepoTab) =>
    void navigate({ to: `/t/$teamSlug/repos/$name`, params: { teamSlug, name }, search: { tab: next }, replace: true })

  const back = (
    <Button
      variant="ghost"
      size="icon"
      aria-label={`Repositories`}
      className="-ml-2 size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground"
      onClick={() => void navigate({ to: `/t/$teamSlug/settings/repositories`, params: { teamSlug } })}
    >
      <UiBackIcon className="size-5" />
    </Button>
  )

  return (
    <div className="h-full overflow-y-auto" data-testid="repo-page">
      <div className="mx-auto w-full max-w-3xl space-y-4 px-4 py-4 md:max-w-4xl">
        {/* THE AppBar: back, the repo's name, its provider. */}
        <div className="flex min-w-0 items-center gap-2">
          {back}
          <h1 className="min-w-0 truncate text-lg font-semibold">{repo?.fullName ?? name}</h1>
          {repo && <ProviderBadge provider={repo.provider} />}
        </div>
        <SegmentedControl
          value={tab}
          onValueChange={(value) => setTab(value as RepoTab)}
          options={REPO_TABS.map((t) => ({ value: t.id, label: t.label }))}
        />
        {repos === null ? (
          <div className={SETTINGS_LIST_CLASS}>
            <SkeletonRows rowHeight={52} />
          </div>
        ) : !repo ? (
          <EmptyState size="list" icon={RepositoriesIcon} title={`Repository not found`} />
        ) : tab === `pulls` ? (
          <PullsTab prs={prRows} member={isMember} />
        ) : tab === `runs` ? (
          <EmptyState size="list" title={COPY.runs.empty} />
        ) : tab === `branches` ? (
          <BranchesTab repo={repo} prs={prRows} />
        ) : (
          <SettingsTab repo={repo} isOwner={isOwner} />
        )}
      </div>
    </div>
  )
}

type PrRow = (typeof pullRequestCollection)[`toArray`][number]

function PullsTab({ prs, member }: { prs: PrRow[]; member: boolean }) {
  const byId = useMemo(() => new Map(prs.map((pr) => [pr.id, pr])), [prs])
  const rows = repoPrTree(prs as unknown as RepoPr[])
  if (rows.length === 0) return <EmptyState size="list" title={COPY.pulls.empty} />
  return (
    // Section{tree}: the rows nest with tree guides by depth.
    <PrList
      rows={rows.map((row) => {
        const pr = byId.get(row.id)!
        return {
          key: row.id,
          identifier: row.identifier,
          title: row.title,
          depth: row.depth,
          word: row.subtitle,
          trailing: (
            <span className="flex shrink-0 items-center gap-1.5">
              {row.checks && (
                <Pill size="sm" dot={checksDotColor(row.checks)}>
                  {checksLabel(row.checks)}
                </Pill>
              )}
              <PullRequestStatusPill pr={pr} member={member} />
            </span>
          ),
        }
      })}
    />
  )
}

function BranchesTab({ repo, prs }: { repo: Repo; prs: PrRow[] }) {
  const [branches, setBranches] = useState<Array<{ name: string; sha: string }> | null>(null)
  useEffect(() => {
    let live = true
    const load =
      repo.provider === `ecode`
        ? trpc.repositories.branches.query({ repositoryId: repo.id }, SKIP_ERROR_TOAST).then((r) => r.branches)
        : trpc.repositories.listBranches
            .query({ repositoryId: repo.id }, SKIP_ERROR_TOAST)
            .then((r) => r.branches.map((name) => ({ name, sha: `` })))
    load
      .then((rows) => live && setBranches(rows))
      .catch((err: unknown) => {
        if (!live) return
        setBranches([])
        failedAction(`load`, `the branches`, err)
      })
    return () => {
      live = false
    }
  }, [repo.id, repo.provider])
  if (branches === null) return <SkeletonRows rowHeight={52} />
  const rows = repoBranches(branches, repo.defaultBranch, prs as unknown as RepoPr[])
  if (rows.length === 0) return <EmptyState size="list" title={COPY.branches.empty} />
  return (
    <div className={SETTINGS_LIST_CLASS} data-testid="repo-branches">
      {rows.map((row) => (
        <ListRow key={row.name} className="gap-2 px-3 py-2">
          <div className="min-w-0 flex-1">
            <div className="truncate font-mono text-sm">{row.name}</div>
            {row.sha && <div className="truncate font-mono text-xs text-muted-foreground">{row.sha}</div>}
          </div>
          {row.prNumber !== null && <span className="shrink-0 text-xs text-muted-foreground">#{row.prNumber}</span>}
          {row.isDefault && <Pill size="sm">{COPY.branches.default}</Pill>}
        </ListRow>
      ))}
    </div>
  )
}

function SettingsTab({ repo, isOwner }: { repo: Repo; isOwner: boolean }) {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [branches, setBranches] = useState<string[]>([])
  const [restoreId, setRestoreId] = useState<string | null>(null)
  const [mirrorUrl, setMirrorUrl] = useState(``)
  const [busy, setBusy] = useState(false)
  const ecode = repo.provider === `ecode`

  const load = useCallback(async () => {
    if (!ecode) return
    try {
      const [next, list] = await Promise.all([
        trpc.repositories.settings.query({ repositoryId: repo.id }, SKIP_ERROR_TOAST),
        trpc.repositories.branches.query({ repositoryId: repo.id }, SKIP_ERROR_TOAST),
      ])
      setSettings(next)
      setMirrorUrl(next.mirrorUrl ?? ``)
      setBranches(list.branches.map((b) => b.name))
    } catch (err) {
      failedAction(`load`, `the repository settings`, err)
    }
  }, [ecode, repo.id])
  useEffect(() => {
    void load()
  }, [load])

  const act = async (verb: string, thing: string, fn: () => Promise<unknown>) => {
    setBusy(true)
    try {
      await fn()
      await load()
    } catch (err) {
      failedAction(verb, thing, err, { retry: () => void act(verb, thing, fn) })
    } finally {
      setBusy(false)
    }
  }

  if (ecode && !settings) return <SkeletonRows rowHeight={52} />
  const options = branches.length > 0 ? branches : [repo.defaultBranch]

  return (
    <div className="space-y-4" data-testid="repo-settings">
      <div className={SETTINGS_LIST_CLASS}>
        <ListRow className="gap-3 px-4 py-3">
          <span className="flex-1 text-sm">{COPY.settings.provider}</span>
          <ProviderBadge provider={repo.provider} />
        </ListRow>
        <ListRow className="gap-3 px-4 py-3">
          <span className="flex-1 text-sm">{COPY.settings.defaultBranch}</span>
          <Select
            value={settings?.defaultBranch ?? repo.defaultBranch}
            disabled={!isOwner || !ecode || busy}
            onValueChange={(branch) =>
              void act(`set`, `the default branch`, () =>
                trpc.repositories.setDefaultBranch.mutate({ repositoryId: repo.id, branch }, SKIP_ERROR_TOAST)
              )
            }
          >
            <SelectTrigger size="sm" className="w-auto font-mono">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {options.map((name) => (
                <SelectItem key={name} value={name} className="font-mono">
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </ListRow>
        {settings?.remoteUrl && (
          <ListRow className="gap-3 px-4 py-3">
            <span className="flex-1 text-sm">{COPY.settings.cloneUrl}</span>
            <code className="truncate text-xs text-muted-foreground">{settings.remoteUrl}</code>
          </ListRow>
        )}
      </div>

      {ecode && settings && (
        <>
          <div>
            <GlassSectionHeader label={COPY.settings.backups} count={settings.backups.length} />
            <div className={SETTINGS_LIST_CLASS} data-testid="repo-backups">
              {settings.backups.length === 0 ? (
                <EmptyState size="list" title={COPY.settings.noBackups} />
              ) : (
                settings.backups.map((backup) => (
                  <ListRow key={backup.id} className="gap-3 px-4 py-2.5">
                    <span className="flex-1 text-sm">{new Date(backup.createdAt).toLocaleString()}</span>
                    <span className="text-xs text-muted-foreground">{formatBytes(backup.bytes)}</span>
                    {isOwner && (
                      <Pill mode="action" size="sm" disabled={busy} onClick={() => setRestoreId(backup.id)}>
                        {COPY.settings.restore}
                      </Pill>
                    )}
                  </ListRow>
                ))
              )}
            </div>
          </div>
          <div>
            <GlassSectionHeader label={COPY.settings.mirror} />
            <div className={SETTINGS_LIST_CLASS}>
              <GlassToggleRow
                id="repo-mirror"
                label={COPY.add.mirror}
                checked={settings.mirrorEnabled}
                disabled={!isOwner || busy || !mirrorUrl.trim()}
                onCheckedChange={(enabled) =>
                  void act(`save`, `the mirror`, () =>
                    trpc.repositories.setMirror.mutate({ repositoryId: repo.id, mirrorUrl: mirrorUrl.trim() || null, enabled }, SKIP_ERROR_TOAST)
                  )
                }
              />
              <ListRow className="gap-3 px-4 py-3">
                <span className="shrink-0 text-sm">{COPY.settings.mirrorUrl}</span>
                <Input
                  value={mirrorUrl}
                  disabled={!isOwner || busy}
                  placeholder="https://github.com/owner/name"
                  className="h-8 flex-1 text-right"
                  onChange={(e) => setMirrorUrl(e.target.value)}
                  onBlur={() =>
                    mirrorUrl.trim() !== (settings.mirrorUrl ?? ``) &&
                    void act(`save`, `the mirror`, () =>
                      trpc.repositories.setMirror.mutate(
                        { repositoryId: repo.id, mirrorUrl: mirrorUrl.trim() || null, enabled: settings.mirrorEnabled },
                        SKIP_ERROR_TOAST
                      )
                    )
                  }
                />
              </ListRow>
            </div>
          </div>
        </>
      )}

      <Prompt
        open={restoreId !== null}
        onOpenChange={(open) => !open && setRestoreId(null)}
        title={COPY.settings.restoreTitle}
        busy={busy}
        actions={[
          { label: `Cancel`, role: `cancel` },
          {
            label: COPY.settings.restoreConfirm,
            role: `destructive`,
            onSelect: () => {
              const backupId = restoreId!
              setRestoreId(null)
              void act(`restore`, `the backup`, () =>
                trpc.repositories.restore.mutate({ repositoryId: repo.id, backupId }, SKIP_ERROR_TOAST)
              )
            },
          },
        ]}
      />
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
