// EXP-792: Settings › MCP servers. The team's MCP server registry, made easy:
// every member sees the list and connects THEIR OWN account once (the server
// holds the credential, encrypted, per member — it then works on every
// device, remote starts and triggers); owners add, edit and remove.
//
// A row = name, where it lives, "N of M connected" and ONE action for the
// viewer: Connect (OAuth: `mcpServers.connect` → the provider's consent page
// → the server-side callback → back here with `?mcp=connected|failed`), Set
// key (an API-key server), or Connected (menu: Test connection, Share with
// team / Stop sharing, Disconnect). FEED-73: a member may SHARE their own
// connection with the team (like a shared device) — the row then counts
// "K shared" and wears a "Shared" pill for the viewer whose connection it is.
// Adding a server starts from a catalog tile or a pasted URL; the server
// probes it to detect the sign-in kind, so the owner only confirms a name.
// Everything technical (auth override, transport, header/env names, scopes)
// hides behind "Advanced".
import { useEffect, useMemo, useRef, useState } from "react"
import { promptActions, WEB_PROMPTS } from "@/lib/prompts"
import type { McpAuth, McpTransport } from "@exp/db-schema/domain"
import {
  conceptIcon,
  Button,
  BARE_FIELD_CLASS,
  Picker,
  DisclosureHeader,
  Input,
  Pill,
  type PickerItem,
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  GlassToggleRow,
  ListRow,
  PasswordInput,
  Prompt,
  SETTINGS_LIST_CLASS,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Label,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  ExponentialLogo,
  MCP_CATALOG,
  PickerItemBody,
  PickerList,
  getMcpServerIcon,
  mcpServerPickerItems,
  type McpCatalogEntry,
  toast,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  draftFromServer,
  sharedSummary,
  EMPTY_MCP_SERVER_DRAFT,
  isNavigableAuthorizeUrl,
  UNSAFE_AUTHORIZE_URL_MESSAGE,
  MCP_AUTH_LABELS,
  MCP_TRANSPORT_LABELS,
  MCP_VARIABLE_NAME_RE,
  mcpAuthLine,
  mcpServerTarget,
  mcpUrlProblem,
  nameFromUrl,
  sameMcpUrl,
  validateMcpServerDraft,
  type McpServerDraft,
  type McpServerRow,
} from "@/lib/mcp-servers"
import { builtinExpTools } from "@/lib/agent-feed"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { cn } from "@/lib/utils"

const AddIcon = conceptIcon(`ui-add`)
const EditIcon = conceptIcon(`ui-edit`)
const RemoveIcon = conceptIcon(`ui-delete`)
const MoreIcon = conceptIcon(`ui-more`)
const SignInIcon = conceptIcon(`ui-sign-in`)
const KeyIcon = conceptIcon(`ui-private`)
const ConnectedIcon = conceptIcon(`ui-success`)
const WarningIcon = conceptIcon(`ui-warning`)
const TestIcon = conceptIcon(`ui-refresh`)
const DisconnectIcon = conceptIcon(`ui-clear`)
const BackIcon = conceptIcon(`ui-back`)
const CloseIcon = conceptIcon(`ui-close`)
const LoadingIcon = conceptIcon(`ui-loading`)
const ShareIcon = conceptIcon(`ui-share`)

/** A settings deep link's one-shot requests (the route strips them once
 * handed over): `?connect=<id>` starts a connect, `?mcp=…` is the OAuth
 * callback's verdict. */
export interface McpSettingsRequest {
  connect?: string
  mcp?: `connected` | `failed`
  server?: string
  error?: string
}

type Editor = { mode: `add` } | { mode: `edit`; server: McpServerRow }

export function TeamMcpServersSection({
  teamId,
  isOwner,
  request,
  onRequestConsumed,
}: {
  teamId: string
  isOwner: boolean
  request?: McpSettingsRequest
  onRequestConsumed?: () => void
}) {
  const { servers, error, refresh, setShared } = useMcpServers(teamId)
  const [editor, setEditor] = useState<Editor | null>(null)
  const [removeTarget, setRemoveTarget] = useState<McpServerRow | null>(null)
  const [keyTarget, setKeyTarget] = useState<McpServerRow | null>(null)
  const [busy, setBusy] = useState(false)
  const [dialogError, setDialogError] = useState<string | null>(null)
  // An add started from the empty state's inline catalog: the probe already
  // ran, so the dialog opens straight on the form.
  const [seed, setSeed] = useState<ProbeSeed | null>(null)
  // The one row action in flight (a connect navigates away, so it never
  // clears on success).
  const [pending, setPending] = useState<string | null>(null)

  const connect = async (server: McpServerRow) => {
    if (server.auth === `secret`) {
      setKeyTarget(server)
      return
    }
    if (server.auth !== `oauth`) return
    setPending(server.id)
    try {
      const { authorizeUrl } = await trpc.mcpServers.connect.mutate(
        { serverId: server.id, returnTo: window.location.pathname },
        { context: { skipErrorToast: true } }
      )
      if (!isNavigableAuthorizeUrl(authorizeUrl)) {
        setPending(null)
        toast.error(`Could not connect to ${server.name}`, {
          description: UNSAFE_AUTHORIZE_URL_MESSAGE,
        })
        return
      }
      window.location.assign(authorizeUrl)
    } catch (err) {
      setPending(null)
      toast.error(`Could not connect to ${server.name}`, {
        description: trpcErrorMessage(err, `Try again.`),
      })
    }
  }

  const disconnect = async (server: McpServerRow) => {
    setPending(server.id)
    try {
      await trpc.mcpServers.disconnect.mutate({ serverId: server.id })
      toast.success(`Disconnected from ${server.name}`)
      await refresh()
    } catch (err) {
      toast.error(`Could not disconnect`, {
        description: trpcErrorMessage(err, `Try again.`),
      })
    } finally {
      setPending(null)
    }
  }

  const share = async (server: McpServerRow) => {
    const next = !server.connection.shared
    setPending(server.id)
    try {
      if (await setShared(server.id, next)) {
        toast.success(
          next
            ? `Shared ${server.name} with the team: action runs on teammates' machines use your connection`
            : `Stopped sharing ${server.name}`
        )
      }
    } finally {
      setPending(null)
    }
  }

  const test = async (server: McpServerRow) => {
    setPending(server.id)
    try {
      const result = await trpc.mcpServers.test.mutate(
        { serverId: server.id },
        { context: { skipErrorToast: true } }
      )
      if (result.ok) {
        toast.success(
          result.tools === null
            ? `${server.name} answered`
            : `${server.name} answered with ${result.tools} ${result.tools === 1 ? `tool` : `tools`}`
        )
      } else {
        toast.error(`${server.name} did not answer`, {
          description: result.error ?? undefined,
        })
      }
    } catch (err) {
      toast.error(`${server.name} did not answer`, {
        description: trpcErrorMessage(err, `Try again.`),
      })
    } finally {
      setPending(null)
    }
  }

  // The deep link's one-shots, once the list can name the server. Held in a
  // ref so a re-render (or StrictMode's double effect) never fires twice.
  const handledRef = useRef<string | null>(null)
  useEffect(() => {
    if (!request || servers === null) return
    if (!request.connect && !request.mcp) return
    const key = JSON.stringify(request)
    if (handledRef.current === key) return
    handledRef.current = key
    const byId = (id: string | undefined) =>
      id ? (servers.find((server) => server.id === id) ?? null) : null
    if (request.mcp) {
      const name = byId(request.server)?.name ?? `the server`
      if (request.mcp === `connected`) toast.success(`Connected to ${name}`)
      else {
        toast.error(`Could not connect to ${name}`, {
          description: request.error || undefined,
        })
      }
    }
    const target = byId(request.connect)
    onRequestConsumed?.()
    if (!request.connect) return
    if (!target) {
      toast.error(`That MCP server is no longer on the team`)
      return
    }
    const status = target.connection.status
    if (status === `connected` || status === `not_needed`) {
      toast.success(
        status === `connected`
          ? `Already connected to ${target.name}`
          : `${target.name} needs no sign-in`
      )
      return
    }
    void connect(target)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request, servers])

  const save = async (draft: McpServerDraft) => {
    if (busy || editor === null) return
    setBusy(true)
    setDialogError(null)
    const http = draft.transport === `http`
    const fields = {
      name: draft.name.trim(),
      transport: draft.transport,
      auth: draft.auth,
      enabledByDefault: draft.enabledByDefault,
      scopes: draft.scopes,
      ...(http
        ? { url: draft.url.trim(), headerNames: draft.headerNames }
        : {
            command: draft.command.trim(),
            args: draft.args,
            envNames: draft.envNames,
          }),
    }
    try {
      if (editor.mode === `add`) {
        const row = await trpc.mcpServers.create.mutate(
          { teamId, ...fields },
          { context: { skipErrorToast: true } }
        )
        setEditor(null)
        setSeed(null)
        await refresh()
        // Adding Linear = pick the tile, Save, consent, back — connected.
        if (row.auth === `oauth`) {
          setPending(row.id)
          try {
            const { authorizeUrl } = await trpc.mcpServers.connect.mutate(
              { serverId: row.id, returnTo: window.location.pathname },
              { context: { skipErrorToast: true } }
            )
            if (!isNavigableAuthorizeUrl(authorizeUrl)) {
              setPending(null)
              toast.error(`Added ${row.name}, but could not start the sign-in`, {
                description: UNSAFE_AUTHORIZE_URL_MESSAGE,
              })
              return
            }
            window.location.assign(authorizeUrl)
          } catch (err) {
            setPending(null)
            toast.error(`Added ${row.name}, but could not start the sign-in`, {
              description: trpcErrorMessage(err, `Use Connect on its row.`),
            })
          }
        } else {
          toast.success(`Added ${row.name}`)
        }
      } else {
        await trpc.mcpServers.update.mutate(
          { id: editor.server.id, ...fields },
          { context: { skipErrorToast: true } }
        )
        setEditor(null)
        await refresh()
      }
    } catch (err) {
      setDialogError(trpcErrorMessage(err, `That didn't go through. Try again.`))
    } finally {
      setBusy(false)
    }
  }

  const remove = async () => {
    if (!removeTarget || busy) return
    setBusy(true)
    try {
      await trpc.mcpServers.remove.mutate({ id: removeTarget.id })
      setRemoveTarget(null)
      await refresh()
    } catch (err) {
      toast.error(`Couldn't remove the server`, {
        description: trpcErrorMessage(err, `Try again.`),
      })
    } finally {
      setBusy(false)
    }
  }

  const openAdd = () => {
    setDialogError(null)
    setEditor({ mode: `add` })
  }

  const removeCopy = WEB_PROMPTS.removeMcpServer(
    removeTarget?.name ?? `this server`
  )

  return (
    <div className="mb-6">
      <GlassSectionHeader
        label="MCP servers"
        trailing={
          isOwner && servers !== null && servers.length > 0 ? (
            <Pill mode="action" onClick={openAdd}>
              <AddIcon className="size-3" />
              Add server
            </Pill>
          ) : undefined
        }
      />
      <p className="mb-3 px-1 text-xs text-muted-foreground">
        Tools your agents can use in runs. Each member connects their own
        account.
      </p>

      {error && <p className="px-1 pb-2 text-xs text-destructive">{error}</p>}
      {servers === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : servers.length === 0 ? (
        isOwner ? (
          <InlineCatalog
            teamId={teamId}
            onPicked={(picked) => {
              setDialogError(null)
              setSeed(picked)
              setEditor({ mode: `add` })
            }}
          />
        ) : (
          <div className={SETTINGS_LIST_CLASS}>
            <ListRow className="px-3 py-2 text-sm text-muted-foreground">
              No MCP servers yet. A team owner can add them.
            </ListRow>
          </div>
        )
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {servers.map((server) => (
            <ServerRow
              key={server.id}
              server={server}
              isOwner={isOwner}
              pending={pending === server.id}
              onConnect={() => void connect(server)}
              onDisconnect={() => void disconnect(server)}
              onTest={() => void test(server)}
              onToggleShared={() => void share(server)}
              onReplaceKey={() => setKeyTarget(server)}
              onEdit={() => {
                setDialogError(null)
                setEditor({ mode: `edit`, server })
              }}
              onRemove={() => setRemoveTarget(server)}
            />
          ))}
        </div>
      )}

      <BuiltinToolsGroup />

      {isOwner && (
        <McpServerDialog
          key={
            editor === null
              ? `closed`
              : editor.mode === `add`
                ? `add:${seed?.draft.url ?? ``}`
                : editor.server.id
          }
          open={editor !== null}
          teamId={teamId}
          mode={editor?.mode ?? `add`}
          initial={
            editor?.mode === `edit` ? draftFromServer(editor.server) : null
          }
          seed={editor?.mode === `add` ? seed : null}
          existing={servers ?? []}
          busy={busy}
          error={dialogError}
          onOpenChange={(open) => {
            if (!open && !busy) {
              setEditor(null)
              setSeed(null)
            }
          }}
          onSubmit={(draft) => void save(draft)}
        />
      )}

      <SecretDialog
        server={keyTarget}
        onClose={() => setKeyTarget(null)}
        onSaved={async (server) => {
          setKeyTarget(null)
          toast.success(`Key saved for ${server.name}`)
          await refresh()
        }}
      />

      <Prompt
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRemoveTarget(null)
        }}
        busy={busy}
        title={removeCopy.title}
        body={removeCopy.body}
        actions={promptActions(removeCopy, {
          remove: { busy, onSelect: remove },
        })}
      />
    </div>
  )
}

// ── Probe + catalog ──────────────────────────────────────────────────────────

/** What a probe hands the form: the prefilled draft and, when the server
 * could not be checked, a soft note (the owner may still save). A TEMPLATE
 * pick (a `{host}` catalog URL, never probed) asks the form to focus the URL
 * field: the host is the one thing the owner has to type. */
interface ProbeSeed {
  draft: McpServerDraft
  note: string | null
  focus?: `url`
}

/** A template catalog entry's seed: the form opens on the template itself,
 * name from the id, OAuth (every self-managed entry is), no probe. */
function templateSeed(entry: McpCatalogEntry): ProbeSeed {
  return {
    draft: {
      ...EMPTY_MCP_SERVER_DRAFT,
      url: entry.url,
      name: entry.id,
      auth: `oauth`,
    },
    note: null,
    focus: `url`,
  }
}

/** `mcpServers.probe` → a prefilled draft. A failure never blocks: the draft
 * keeps the URL, a name off its host and `none` auth, and the note says why
 * (Advanced can set the auth by hand). */
function useProbe(teamId: string) {
  const [probing, setProbing] = useState<string | null>(null)
  const probe = async (url: string, fallbackName: string): Promise<ProbeSeed> => {
    setProbing(url)
    const base: McpServerDraft = {
      ...EMPTY_MCP_SERVER_DRAFT,
      url,
      name: fallbackName || nameFromUrl(url),
    }
    try {
      const result = await trpc.mcpServers.probe.mutate(
        { teamId, url },
        { context: { skipErrorToast: true } }
      )
      return {
        draft: {
          ...base,
          url: result.url || url,
          name: result.suggestedName || base.name,
          auth: result.auth,
          scopes: result.scopes,
        },
        note: result.reachable
          ? result.error
          : (result.error ?? `The server did not answer.`),
      }
    } catch (err) {
      return {
        draft: base,
        note: trpcErrorMessage(err, `The server could not be checked.`),
      }
    } finally {
      setProbing(null)
    }
  }
  return { probing, probe }
}

function hostLabel(url: string): string {
  try {
    return new URL(url).host
  } catch {
    return url
  }
}

/** ONE search box over the well-known servers: type to filter them, or paste
 * any server's URL and pick its "Use …" row. The rows are `@exp/ui`'s MCP
 * picker rows in a `PickerList` (the dialog is already a surface). Shared by
 * the empty state (inline) and the Add dialog's first step. */
function CatalogPicker({
  teamId,
  existing,
  onPicked,
  onManual,
}: {
  teamId: string
  existing: readonly McpServerRow[]
  onPicked: (seed: ProbeSeed) => void
  onManual?: () => void
}) {
  const { probing, probe } = useProbe(teamId)
  const [query, setQuery] = useState(``)
  const typed = query.trim()
  const typedUrl = /^https?:\/\//i.test(typed) ? typed : null
  const urlProblem = typedUrl ? mcpUrlProblem(typedUrl) : null
  // Rows are keyed by catalog id (a template and a hosted entry may share a
  // mark, never an id); the pasted-URL row by the URL itself. A template is
  // never "Added": any number of self-managed hosts may exist.
  const catalog = mcpServerPickerItems(
    MCP_CATALOG.map((entry) => {
      const added =
        !entry.template &&
        existing.some((server) => sameMcpUrl(server.url, entry.url))
      return {
        id: entry.id,
        name: entry.name,
        url: entry.url,
        description: added ? `Added` : entry.template ? `Your own host` : undefined,
        disabled: added,
      }
    })
  )
  const items: PickerItem[] = typedUrl
    ? [
        {
          value: typedUrl,
          label: `Use ${hostLabel(typedUrl)}`,
          icon: getMcpServerIcon({ url: typedUrl }),
          description: urlProblem ?? typedUrl,
          disabled: urlProblem !== null,
          keywords: [typed],
        },
        ...catalog,
      ]
    : catalog
  const pick = async (value: string) => {
    if (probing) return
    const entry = MCP_CATALOG.find((candidate) => candidate.id === value)
    if (entry?.template) {
      onPicked(templateSeed(entry))
      return
    }
    const url = entry ? entry.url : value
    onPicked(await probe(url, entry ? entry.id : ``))
  }
  return (
    <div className="flex flex-col gap-2">
      <PickerList
        mode="single"
        items={items}
        value={null}
        onChange={(value) => void pick(value)}
        search
        inputVariant="field"
        searchPlaceholder="Search or paste a server URL"
        query={query}
        onQueryChange={setQuery}
        emptyText="No match. Paste the server's URL instead."
        listClassName="max-h-[60dvh] sm:max-h-96"
        renderItem={(item) => (
          <span
            data-testid="mcp-catalog-row"
            className="flex min-w-0 flex-1 items-center gap-2.5"
          >
            <PickerItemBody item={item} />
            {probing === item.value && (
              <LoadingIcon className="ml-auto size-4 shrink-0 animate-spin text-muted-foreground" />
            )}
          </span>
        )}
      />
      {onManual && (
        <Button
          type="button"
          variant="text"
          size="inline"
          className="self-start px-1"
          onClick={onManual}
        >
          Run a local command instead
        </Button>
      )}
    </div>
  )
}

/** The empty team, for an owner: the catalog right on the page. */
function InlineCatalog({
  teamId,
  onPicked,
}: {
  teamId: string
  onPicked: (seed: ProbeSeed) => void
}) {
  return (
    <div className="rounded-md border border-glass-stroke bg-glass-row p-4">
      <p className="mb-3 text-sm text-muted-foreground">
        No MCP servers yet. Pick one to add it, or paste any server's URL.
      </p>
      <CatalogPicker teamId={teamId} existing={[]} onPicked={onPicked} />
    </div>
  )
}

// ── Rows ─────────────────────────────────────────────────────────────────────

function ServerRow({
  server,
  isOwner,
  pending,
  onConnect,
  onDisconnect,
  onTest,
  onToggleShared,
  onReplaceKey,
  onEdit,
  onRemove,
}: {
  server: McpServerRow
  isOwner: boolean
  pending: boolean
  onConnect: () => void
  onDisconnect: () => void
  onTest: () => void
  onToggleShared: () => void
  onReplaceKey: () => void
  onEdit: () => void
  onRemove: () => void
}) {
  const target = mcpServerTarget(server)
  const Glyph = getMcpServerIcon(server)
  const needsSignIn = server.auth !== `none`
  return (
    <ListRow className="gap-3 px-3 py-2" data-testid="mcp-server-row">
      <Glyph className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-baseline gap-1.5">
          <span className="min-w-0 truncate text-sm font-medium">
            {server.name}
          </span>
          {server.enabledByDefault && (
            <Pill size="sm" title="Pre-selected in new runs">
              Default
            </Pill>
          )}
          {server.connection.shared && <Pill size="sm">Shared</Pill>}
        </div>
        {/* Phones drop the host beside the count; the name identifies it. */}
        <div className="flex min-w-0 text-xs text-muted-foreground">
          <span
            className={cn(
              `min-w-0 truncate`,
              needsSignIn && `max-sm:hidden`
            )}
            title={server.url ?? target}
          >
            {target}
          </span>
          {needsSignIn && (
            <span className="shrink-0 whitespace-pre">
              <span className="max-sm:hidden">{` · `}</span>
              {sharedSummary(server)}
            </span>
          )}
        </div>
      </div>
      <ConnectionAction
        server={server}
        pending={pending}
        onConnect={onConnect}
        onDisconnect={onDisconnect}
        onTest={onTest}
        onToggleShared={onToggleShared}
        onReplaceKey={onReplaceKey}
      />
      {isOwner && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Server menu for ${server.name}`}
            >
              <MoreIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={onEdit}>
              <EditIcon />
              Edit
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onSelect={onRemove}>
              <RemoveIcon />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </ListRow>
  )
}

/** The viewer's ONE action on a row, from their own `connection`. */
function ConnectionAction({
  server,
  pending,
  onConnect,
  onDisconnect,
  onTest,
  onToggleShared,
  onReplaceKey,
}: {
  server: McpServerRow
  pending: boolean
  onConnect: () => void
  onDisconnect: () => void
  onTest: () => void
  onToggleShared: () => void
  onReplaceKey: () => void
}) {
  const { status, error } = server.connection
  const spinner = pending ? <LoadingIcon className="animate-spin" /> : null

  if (status === `not_needed`) {
    return (
      <span className="shrink-0 text-xs text-muted-foreground">
        No sign-in needed
      </span>
    )
  }

  if (status === `connected`) {
    const http = server.transport === `http`
    return (
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            disabled={pending}
            className="text-emerald-500 hover:text-emerald-400"
            aria-label={`Connected to ${server.name}`}
          >
            {spinner ?? <ConnectedIcon />}
            Connected
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {http && (
            <DropdownMenuItem onSelect={onTest}>
              <TestIcon />
              Test connection
            </DropdownMenuItem>
          )}
          {server.auth === `secret` && (
            <DropdownMenuItem onSelect={onReplaceKey}>
              <KeyIcon />
              Replace key
            </DropdownMenuItem>
          )}
          {/* A stdio server's secret stays on this machine (the server
              refuses the share); only an already-shared row offers undo. */}
          {(http || server.connection.shared) && (
            <DropdownMenuItem onSelect={onToggleShared}>
              <ShareIcon />
              {server.connection.shared ? `Stop sharing` : `Share with team`}
            </DropdownMenuItem>
          )}
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onSelect={onDisconnect}>
            <DisconnectIcon />
            Disconnect
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    )
  }

  if (status === `expired` || status === `error`) {
    const reason =
      error ??
      (status === `expired`
        ? `Your sign-in expired.`
        : `The last refresh failed.`)
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            disabled={pending}
            onClick={onConnect}
          >
            {spinner ?? <WarningIcon className="text-amber-500" />}
            Reconnect
          </Button>
        </TooltipTrigger>
        <TooltipContent>{reason}</TooltipContent>
      </Tooltip>
    )
  }

  const secret = server.auth === `secret`
  return (
    <Button size="sm" disabled={pending} onClick={onConnect}>
      {spinner ?? (secret ? <KeyIcon /> : <SignInIcon />)}
      {secret ? `Set key` : `Connect`}
    </Button>
  )
}

/** "Set key": the member's OWN API key for a secret server, stored by the
 * server encrypted, never shown again. */
function SecretDialog({
  server,
  onClose,
  onSaved,
}: {
  server: McpServerRow | null
  onClose: () => void
  onSaved: (server: McpServerRow) => Promise<void>
}) {
  const [value, setValue] = useState(``)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (server) {
      setValue(``)
      setError(null)
    }
  }, [server])
  const name = server
    ? (server.transport === `http` ? server.headerNames : server.envNames)[0]
    : undefined
  const save = async () => {
    if (!server || busy || value.trim().length === 0) return
    setBusy(true)
    setError(null)
    try {
      await trpc.mcpServers.setSecret.mutate(
        { serverId: server.id, value: value.trim() },
        { context: { skipErrorToast: true } }
      )
      await onSaved(server)
    } catch (err) {
      setError(trpcErrorMessage(err, `That didn't go through. Try again.`))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open={server !== null}
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent mobile="alert" className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{`API key for ${server?.name ?? ``}`}</DialogTitle>
          <DialogDescription>
            Only your runs use it. Stored encrypted; nobody can read it back.
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            void save()
          }}
        >
          <PasswordInput
            autoFocus
            value={value}
            maxLength={4096}
            placeholder={name ?? `Key`}
            autoComplete="off"
            onChange={(event) => setValue(event.target.value)}
          />
          {name && (
            <p className="px-1 text-[11px] text-muted-foreground">
              {`Sent as ${name}.`}
            </p>
          )}
          {error && <p className="px-1 text-xs text-destructive">{error}</p>}
        </form>
        <DialogFooter>
          <DialogCancel disabled={busy} onClick={onClose} />
          <Button
            disabled={busy || value.trim().length === 0}
            onClick={() => void save()}
          >
            {busy && <LoadingIcon className="animate-spin" />}
            Save key
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/** EXP-862: what EVERY run already has, next to the servers a team adds —
 * the Exponential MCP tools themselves, collapsed. One row per contract tool
 * (`expToolDisplay`): the Exponential mark, the tool's title and its blurb,
 * with the raw wire name only as a tooltip. */
function BuiltinToolsGroup() {
  const [expanded, setExpanded] = useState(false)
  const tools = useMemo(() => builtinExpTools(), [])
  return (
    <div className="mt-6">
      <GlassSectionHeader
        label="Built-in Exponential tools"
        count={tools.length}
        expanded={expanded}
        onToggle={() => setExpanded((open) => !open)}
      />
      {expanded && (
        <div className={SETTINGS_LIST_CLASS}>
          {tools.map((tool) => (
            <ListRow key={tool.name} className="gap-2 px-3 py-2" title={tool.name}>
              <ExponentialLogo
                variant="light"
                size={12}
                className="size-3 shrink-0 text-muted-foreground/60"
              />
              <span className="shrink-0 text-sm font-medium">{tool.title}</span>
              <span className="min-w-0 truncate text-xs text-muted-foreground">
                {tool.blurb}
              </span>
            </ListRow>
          ))}
        </div>
      )}
    </div>
  )
}

// ── Add / edit dialog ────────────────────────────────────────────────────────

const TRANSPORT_OPTIONS: PickerItem[] = [
  { value: `http`, label: MCP_TRANSPORT_LABELS.http },
  { value: `stdio`, label: MCP_TRANSPORT_LABELS.stdio },
]

/** Add = two steps (catalog/URL → confirm); Edit = the confirm form alone,
 * prefilled. Remounted per open (`key`), so its state starts fresh. */
function McpServerDialog({
  open,
  teamId,
  mode,
  initial,
  seed,
  existing,
  busy,
  error,
  onOpenChange,
  onSubmit,
}: {
  open: boolean
  teamId: string
  mode: `add` | `edit`
  /** Edit: the server's current config. */
  initial: McpServerDraft | null
  /** Add from the inline catalog: the probe already ran. */
  seed: ProbeSeed | null
  existing: readonly McpServerRow[]
  busy: boolean
  error: string | null
  onOpenChange: (open: boolean) => void
  onSubmit: (draft: McpServerDraft) => void
}) {
  const start = initial ?? seed?.draft ?? null
  const [step, setStep] = useState<`pick` | `form`>(start ? `form` : `pick`)
  const [draft, setDraft] = useState<McpServerDraft>(
    start ?? EMPTY_MCP_SERVER_DRAFT
  )
  const [note, setNote] = useState<string | null>(seed?.note ?? null)
  const [focus, setFocus] = useState<`url` | null>(seed?.focus ?? null)
  const [advanced, setAdvanced] = useState(start?.transport === `stdio`)
  const patch = (fields: Partial<McpServerDraft>) =>
    setDraft((current) => ({ ...current, ...fields }))
  const http = draft.transport === `http`
  const validation = validateMcpServerDraft(draft)
  const authOptions: PickerItem[] = [
    { value: `none`, label: MCP_AUTH_LABELS.none },
    ...(http ? [{ value: `oauth`, label: MCP_AUTH_LABELS.oauth }] : []),
    { value: `secret`, label: MCP_AUTH_LABELS.secret },
  ]

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        mobile="sheet-full"
        className="gap-4 sm:max-w-lg"
        aria-describedby={undefined}
      >
        <DialogHeader>
          <DialogTitle>
            {mode === `edit` ? `Edit MCP server` : `Add MCP server`}
          </DialogTitle>
        </DialogHeader>

        {step === `pick` ? (
          <DialogBody>
            <CatalogPicker
              teamId={teamId}
              existing={existing}
              onPicked={(picked) => {
                setDraft(picked.draft)
                setNote(picked.note)
                setFocus(picked.focus ?? null)
                setAdvanced(false)
                setStep(`form`)
              }}
              onManual={() => {
                setDraft({
                  ...EMPTY_MCP_SERVER_DRAFT,
                  transport: `stdio`,
                })
                setNote(null)
                setAdvanced(true)
                setStep(`form`)
              }}
            />
          </DialogBody>
        ) : (
          <DialogBody className="flex flex-col gap-2">
            <GlassGroup>
              <GlassInputRow
                id="mcp-name"
                label="Name"
                value={draft.name}
                maxLength={64}
                placeholder="linear"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                onChange={(event) => patch({ name: event.target.value })}
              />
              {http && (
                <GlassInputRow
                  id="mcp-url"
                  label="URL"
                  value={draft.url}
                  placeholder="https://mcp.example.com/mcp"
                  autoCapitalize="off"
                  autoCorrect="off"
                  spellCheck={false}
                  inputMode="url"
                  // A template pick: the host is what the owner types next.
                  autoFocus={focus === `url`}
                  onChange={(event) => patch({ url: event.target.value })}
                />
              )}
              <GlassToggleRow
                id="mcp-enabled-by-default"
                label="Pre-select in new runs"
                checked={draft.enabledByDefault}
                onCheckedChange={(enabledByDefault) => patch({ enabledByDefault })}
              />
            </GlassGroup>

            <div className="flex flex-col gap-1 px-1" data-testid="mcp-auth-line">
              <p className="flex items-center gap-1.5 text-xs text-foreground/80">
                {draft.auth === `none` ? (
                  <ConnectedIcon className="size-3.5 shrink-0 text-muted-foreground" />
                ) : draft.auth === `oauth` ? (
                  <SignInIcon className="size-3.5 shrink-0 text-muted-foreground" />
                ) : (
                  <KeyIcon className="size-3.5 shrink-0 text-muted-foreground" />
                )}
                {mcpAuthLine(draft.auth)}
              </p>
              {note && (
                <p className="text-[11px] text-muted-foreground">
                  {`Couldn't check the server (${note.replace(/\.$/, ``)}). You can still save it.`}
                </p>
              )}
            </div>

            <DisclosureHeader
              open={advanced}
              onToggle={() => setAdvanced((value) => !value)}
              className="mt-1 px-1 text-xs"
            >
              Advanced
            </DisclosureHeader>
            {advanced && (
              <GlassGroup>
                <Picker
                  mode="single"
                  triggerVariant="row"
                  mobileTitle="Sign-in"
                  value={draft.auth}
                  items={authOptions}
                  onChange={(value) => {
                    if (value !== null) patch({ auth: value as McpAuth })
                  }}
                />
                <Picker
                  mode="single"
                  triggerVariant="row"
                  mobileTitle="Transport"
                  value={draft.transport}
                  items={TRANSPORT_OPTIONS}
                  onChange={(value) => {
                    if (value === null) return
                    const transport = value as McpTransport
                    patch({
                      transport,
                      // OAuth is an HTTP-only kind; a stdio row falls back.
                      auth:
                        transport === `stdio` && draft.auth === `oauth`
                          ? `none`
                          : draft.auth,
                    })
                  }}
                />
                {!http && (
                  <GlassInputRow
                    id="mcp-command"
                    label="Command"
                    value={draft.command}
                    placeholder="npx"
                    autoCapitalize="off"
                    autoCorrect="off"
                    spellCheck={false}
                    onChange={(event) => patch({ command: event.target.value })}
                  />
                )}
                {!http && (
                  <ChipsRow
                    id="mcp-args"
                    label="Arguments"
                    values={draft.args}
                    placeholder="-y @acme/mcp"
                    onChange={(args) => patch({ args })}
                  />
                )}
                <ChipsRow
                  id={http ? `mcp-header-names` : `mcp-env-names`}
                  label={
                    draft.auth === `secret`
                      ? http
                        ? `Header that carries the key`
                        : `Variable that carries the key`
                      : http
                        ? `Header names`
                        : `Variable names`
                  }
                  values={http ? draft.headerNames : draft.envNames}
                  placeholder={http ? `Authorization` : `ACME_TOKEN`}
                  pattern={MCP_VARIABLE_NAME_RE}
                  onChange={(names) =>
                    patch(http ? { headerNames: names } : { envNames: names })
                  }
                />
                {draft.auth === `oauth` && (
                  <ChipsRow
                    id="mcp-scopes"
                    label="Scopes"
                    values={draft.scopes}
                    placeholder="read"
                    onChange={(scopes) => patch({ scopes })}
                  />
                )}
              </GlassGroup>
            )}
            {(validation || error) && (
              <p className="px-1 text-xs text-destructive">{error ?? validation}</p>
            )}
          </DialogBody>
        )}

        <DialogFooter>
          {step === `form` && mode === `add` && !seed ? (
            <Button
              variant="ghost"
              className="mr-auto"
              disabled={busy}
              onClick={() => setStep(`pick`)}
            >
              <BackIcon />
              Back
            </Button>
          ) : null}
          <DialogCancel disabled={busy} />
          {step === `form` && (
            <Button
              disabled={busy || validation !== null}
              onClick={() => onSubmit(draft)}
            >
              {busy && <LoadingIcon className="animate-spin" />}
              {mode === `edit`
                ? `Save`
                : draft.auth === `oauth`
                  ? `Save and connect`
                  : `Add server`}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/** A label-leading row of chips with an inline field that adds one on Enter,
 * comma, space or blur. `pattern` refuses a chip at the field (the server
 * would refuse the same value at submit). */
function ChipsRow({
  id,
  label,
  values,
  placeholder,
  pattern,
  onChange,
}: {
  id: string
  label: string
  values: string[]
  placeholder: string
  pattern?: RegExp
  onChange: (values: string[]) => void
}) {
  const [text, setText] = useState(``)
  const [invalid, setInvalid] = useState(false)
  const add = (raw: string) => {
    const value = raw.trim().replace(/,$/, ``)
    if (!value) return
    if (pattern && !pattern.test(value)) {
      setInvalid(true)
      return
    }
    if (!values.includes(value)) onChange([...values, value])
    setText(``)
    setInvalid(false)
  }
  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      <Label htmlFor={id} className="font-normal">
        {label}
      </Label>
      <div className="flex flex-wrap items-center gap-1.5">
        {values.map((value) => (
          <Pill key={value} size="sm">
            <span className="font-mono">{value}</span>
            <Button
              type="button"
              variant="text"
              size="inline"
              className="-mr-0.5"
              aria-label={`Remove ${value}`}
              onClick={() => onChange(values.filter((v) => v !== value))}
            >
              <CloseIcon className="size-3" />
            </Button>
          </Pill>
        ))}
        <Input
          id={id}
          value={text}
          placeholder={values.length === 0 ? placeholder : ``}
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          className={cn(
            BARE_FIELD_CLASS,
            `h-6 min-w-[8rem] font-mono text-xs md:text-xs`,
            invalid && `text-destructive`
          )}
          onChange={(event) => {
            setInvalid(false)
            const next = event.target.value
            if (next.endsWith(`,`) || next.endsWith(` `)) {
              add(next.slice(0, -1))
              return
            }
            setText(next)
          }}
          onBlur={() => add(text)}
          onKeyDown={(event) => {
            if (event.key === `Enter`) {
              event.preventDefault()
              add(text)
            } else if (event.key === `Backspace` && text === `` && values.length > 0) {
              onChange(values.slice(0, -1))
            }
          }}
        />
      </div>
      {invalid && (
        <p className="text-[11px] text-destructive">
          Use letters, digits, `_` or `-`, starting with a letter or `_`.
        </p>
      )}
    </div>
  )
}
