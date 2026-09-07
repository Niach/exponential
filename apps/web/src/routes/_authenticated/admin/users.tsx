import { useState } from "react"
import { createFileRoute, Link, useRouter } from "@tanstack/react-router"
import { ArrowDown, ArrowUp, Ellipsis, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { useSession } from "@/hooks/use-session"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn, getInitials } from "@/lib/utils"
import { formatDate, formatRelative, PlatformPills } from "./-shared"

type AdminUser = Awaited<ReturnType<typeof trpc.admin.listUsers.query>>[number]

// EXP-759: client-side sort over the (unpaginated) list; the choice rides
// the URL so a reload or a back-navigation keeps it.
type SortKey = `name` | `joined` | `active` | `teams`
type SortDir = `asc` | `desc`
const SORT_KEYS: SortKey[] = [`name`, `joined`, `active`, `teams`]
const DEFAULT_DIR: Record<SortKey, SortDir> = {
  name: `asc`,
  joined: `desc`,
  active: `desc`,
  teams: `desc`,
}

function timeOf(value: Date | string | null | undefined): number {
  if (!value) return 0
  const t = new Date(value).getTime()
  return Number.isNaN(t) ? 0 : t
}

function compareUsers(a: AdminUser, b: AdminUser, key: SortKey): number {
  switch (key) {
    case `name`:
      return (a.name || a.email).localeCompare(b.name || b.email, undefined, {
        sensitivity: `base`,
      })
    case `joined`:
      return timeOf(a.createdAt) - timeOf(b.createdAt)
    case `active`:
      return timeOf(a.lastActiveAt) - timeOf(b.lastActiveAt)
    case `teams`:
      return a.teamCount - b.teamCount
  }
}

export const Route = createFileRoute(`/_authenticated/admin/users`)({
  validateSearch: (
    search: Record<string, unknown>
  ): { sort?: SortKey; dir?: SortDir } => ({
    sort: SORT_KEYS.includes(search.sort as SortKey)
      ? (search.sort as SortKey)
      : undefined,
    dir:
      search.dir === `asc` || search.dir === `desc`
        ? (search.dir as SortDir)
        : undefined,
  }),
  loader: async () => {
    const users = await trpc.admin.listUsers.query()
    return { users }
  },
  component: AdminUsers,
})

const GRID = `md:grid-cols-[minmax(0,1fr)_120px_130px_60px_95px_100px_60px_40px]`

function SortHeader({
  label,
  sortKey,
  active,
  dir,
}: {
  label: string
  sortKey: SortKey
  active: boolean
  dir: SortDir
}) {
  const nextDir: SortDir = active
    ? dir === `asc`
      ? `desc`
      : `asc`
    : DEFAULT_DIR[sortKey]
  const Arrow = dir === `asc` ? ArrowUp : ArrowDown
  return (
    <Link
      to="/admin/users"
      search={{ sort: sortKey, dir: nextDir }}
      replace
      className={cn(
        `inline-flex items-center gap-1 hover:text-foreground`,
        active && `text-foreground`
      )}
      aria-sort={active ? (dir === `asc` ? `ascending` : `descending`) : `none`}
    >
      {label}
      {active && <Arrow className="h-3 w-3" />}
    </Link>
  )
}

function AdminUsers() {
  const router = useRouter()
  const { users } = Route.useLoaderData()
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const [search, setSearch] = useState(``)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<AdminUser | null>(null)
  const sortSearch = Route.useSearch()
  const sortKey: SortKey = sortSearch.sort ?? `joined`
  const sortDir: SortDir = sortSearch.dir ?? DEFAULT_DIR[sortKey]

  const filteredUsers = users
    .filter((u: AdminUser) => {
      if (!search) return true
      const q = search.toLowerCase()
      return (
        u.name?.toLowerCase().includes(q) || u.email.toLowerCase().includes(q)
      )
    })
    .sort((a, b) => {
      const cmp = compareUsers(a, b, sortKey)
      // Users without the sort value ("—" last-active) sink to the bottom in
      // both directions; ties fall back to newest first.
      if (sortKey === `active`) {
        const aMissing = !a.lastActiveAt
        const bMissing = !b.lastActiveAt
        if (aMissing !== bMissing) return aMissing ? 1 : -1
      }
      const ordered = sortDir === `asc` ? cmp : -cmp
      return ordered !== 0 ? ordered : timeOf(b.createdAt) - timeOf(a.createdAt)
    })

  const handleToggleAdmin = async (user: AdminUser, next: boolean) => {
    setError(null)
    setBusy(user.id)
    try {
      await trpc.admin.setUserAdmin.mutate({
        userId: user.id,
        isAdmin: next,
      })
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  const handleDelete = async () => {
    if (!confirmDelete) return
    setError(null)
    setBusy(confirmDelete.id)
    try {
      await trpc.admin.deleteUser.mutate({ userId: confirmDelete.id })
      setConfirmDelete(null)
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold">Users</h1>
        <p className="text-sm text-muted-foreground">
          {filteredUsers.length} {filteredUsers.length === 1 ? `user` : `users`} on this
          instance.
        </p>
      </div>

      <Input
        placeholder="Search by name or email…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        className="max-w-sm"
      />

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="rounded-md border">
        {/* Desktop column header */}
        <div
          className={`hidden md:grid ${GRID} items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground`}
        >
          <div>
            <SortHeader
              label="User"
              sortKey="name"
              active={sortKey === `name`}
              dir={sortDir}
            />
          </div>
          <div>Providers</div>
          <div>Platforms</div>
          <div>
            <SortHeader
              label="Teams"
              sortKey="teams"
              active={sortKey === `teams`}
              dir={sortDir}
            />
          </div>
          <div>
            <SortHeader
              label="Joined"
              sortKey="joined"
              active={sortKey === `joined`}
              dir={sortDir}
            />
          </div>
          <div>
            <SortHeader
              label="Last active"
              sortKey="active"
              active={sortKey === `active`}
              dir={sortDir}
            />
          </div>
          <div>Admin</div>
          <div />
        </div>
        {filteredUsers.map((user: AdminUser) => {
          const isSelf = user.id === currentUserId
          return (
            <div
              key={user.id}
              className={`flex flex-col md:grid ${GRID} md:items-center gap-3 border-b px-4 py-3 last:border-b-0`}
            >
              <div className="flex items-center gap-3 min-w-0">
                <Link
                  to="/admin/users/$userId"
                  params={{ userId: user.id }}
                  className="flex items-center gap-3 min-w-0 flex-1 group"
                >
                  <Avatar className="h-8 w-8 shrink-0">
                    {user.image && <AvatarImage src={user.image} />}
                    <AvatarFallback className="text-xs" userId={user.id}>
                      {getInitials(user.name || user.email)}
                    </AvatarFallback>
                  </Avatar>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium truncate group-hover:underline">
                      {user.name || user.email}
                      {isSelf && (
                        <span className="text-muted-foreground font-normal">
                          {` (you)`}
                        </span>
                      )}
                    </div>
                    {/* Name-less accounts show the email as the primary line;
                        don't repeat it below. */}
                    {user.name && user.name !== user.email && (
                      <div className="text-xs text-muted-foreground truncate">
                        {user.email}
                      </div>
                    )}
                  </div>
                </Link>
                {/* Mobile: switch + menu on the right of the avatar row */}
                <div className="flex items-center gap-1 md:hidden shrink-0">
                  <Switch
                    checked={user.isAdmin}
                    disabled={busy === user.id}
                    onCheckedChange={(next) => handleToggleAdmin(user, next)}
                    aria-label="Admin"
                  />
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8"
                        disabled={isSelf}
                        aria-label="User actions"
                      >
                        <Ellipsis className="h-4 w-4" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem
                        variant="destructive"
                        onClick={() => setConfirmDelete(user)}
                      >
                        <Trash2 className="mr-2 h-4 w-4" />
                        Delete user
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
              {/* Mobile: meta row beneath the avatar row */}
              <div className="flex md:hidden items-center gap-2 text-xs text-muted-foreground pl-11">
                <span>
                  {user.teamCount}{` `}
                  {user.teamCount === 1 ? `team` : `teams`}
                </span>
                <span aria-hidden>·</span>
                <span>active {formatRelative(user.lastActiveAt)}</span>
                <span aria-hidden>·</span>
                <div className="flex flex-wrap gap-1">
                  {user.providers.length === 0 ? (
                    <span>password</span>
                  ) : (
                    user.providers.map((p: string) => (
                      <Pill key={p}>{p}</Pill>
                    ))
                  )}
                </div>
                {user.platforms.length > 0 && (
                  <>
                    <span aria-hidden>·</span>
                    <PlatformPills platforms={user.platforms} />
                  </>
                )}
              </div>
              {/* Desktop columns */}
              <div className="hidden md:flex flex-wrap gap-1">
                {user.providers.length === 0 ? (
                  <span className="text-xs text-muted-foreground">
                    password
                  </span>
                ) : (
                  user.providers.map((p: string) => (
                    <Pill key={p}>{p}</Pill>
                  ))
                )}
              </div>
              <div className="hidden md:block">
                <PlatformPills platforms={user.platforms} />
              </div>
              <div className="hidden md:block text-sm tabular-nums">
                {user.teamCount}
              </div>
              <div
                className="hidden md:block text-xs text-muted-foreground"
                title={new Date(user.createdAt).toLocaleString()}
              >
                {formatDate(user.createdAt)}
              </div>
              <div
                className="hidden md:block text-xs text-muted-foreground"
                title={user.lastActiveAt ? new Date(user.lastActiveAt).toLocaleString() : undefined}
              >
                {formatRelative(user.lastActiveAt)}
              </div>
              <div className="hidden md:block">
                <Switch
                  checked={user.isAdmin}
                  disabled={busy === user.id}
                  onCheckedChange={(next) => handleToggleAdmin(user, next)}
                />
              </div>
              <div className="hidden md:block">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8"
                      disabled={isSelf}
                      aria-label="User actions"
                    >
                      <Ellipsis className="h-4 w-4" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem
                      variant="destructive"
                      onClick={() => setConfirmDelete(user)}
                    >
                      <Trash2 className="mr-2 h-4 w-4" />
                      Delete user
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>
          )
        })}
      </div>

      <Dialog
        open={Boolean(confirmDelete)}
        onOpenChange={(open) => !open && setConfirmDelete(null)}
      >
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>Delete user?</DialogTitle>
            <DialogDescription>
              This permanently removes <strong>{confirmDelete?.email}</strong>.
              Their sessions, accounts, team memberships, and any issues
              or comments they authored will be deleted. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              variant="outline"
              onClick={() => setConfirmDelete(null)}
              disabled={busy !== null}
            />
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={busy !== null}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
