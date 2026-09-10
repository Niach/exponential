// EXP-792: Settings › MCP servers. The team's MCP server registry — NON-SECRET
// config only (name, transport, url or command, the header/env NAMES a
// machine must supply, scopes, the auth kind) plus, per row, the readiness
// strip: one chip per device from the server's readiness matrix ("Ready",
// "Signed in until 14:05", or the error the machine reported). Every member
// reads; owners add/edit/remove. An `oauth` row offers "Sign in on <device>"
// for the caller's OWN machines: `beginOAuth` queues the flow, the device
// builds the PKCE authorize URL, this pane polls `getOAuthFlow` and sends the
// browser there, the anonymous callback relays the code back to the device,
// and the device's completion closes the flow. No credential ever passes
// through here. `secret` and stdio env values are typed ON the device (the
// desktop pane or `exponential mcp set-secret`), so those chips only say so.
import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import type {
  McpAuth,
  McpTransport,
} from "@exp/db-schema/domain"
import type { Device, User } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { deviceCollection, userCollection } from "@/lib/collections"
import {
  composeDeviceList,
  deviceIsMine,
  deviceIsOnline,
  deviceSupportsMcp,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  draftFromServer,
  EMPTY_MCP_SERVER_DRAFT,
  MCP_AUTH_LABELS,
  MCP_TRANSPORT_LABELS,
  MCP_VARIABLE_NAME_RE,
  readinessExpired,
  readinessFor,
  readinessLabel,
  secretSetupHint,
  serverReadyOn,
  validateMcpServerDraft,
  type McpServerDraft,
  type McpServerRow,
} from "@/lib/mcp-servers"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useNow } from "@/hooks/use-now"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Pill } from "@/components/ui/pill"
import {
  GlassGroup,
  GlassInputRow,
  GlassPickerRow,
  GlassRow,
  GlassSectionHeader,
  GlassToggleRow,
} from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogBody,
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
import { cn } from "@/lib/utils"

const McpIcon = conceptIcon(`settings-mcp`)
const AddIcon = conceptIcon(`ui-add`)
const EditIcon = conceptIcon(`ui-edit`)
const RemoveIcon = conceptIcon(`ui-delete`)
const MoreIcon = conceptIcon(`ui-more`)
const SignInIcon = conceptIcon(`ui-sign-in`)
const CloseIcon = conceptIcon(`ui-close`)

/** How often the pane asks the server where a sign-in flow stands. */
const OAUTH_POLL_MS = 1_500
/** A flow older than this is dead server-side (`getOAuthFlow` reads it as
 * failed/expired); stop polling a little after that. */
const OAUTH_GIVE_UP_MS = 11 * 60 * 1000

type FlowPhase = `starting` | `waiting` | `browser` | `failed`

interface FlowUi {
  phase: FlowPhase
  error?: string
}

function flowKey(serverId: string, deviceId: string): string {
  return `${serverId}:${deviceId}`
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export function TeamMcpServersSection({
  teamId,
  currentUserId,
  isOwner,
}: {
  teamId: string
  currentUserId: string
  isOwner: boolean
}) {
  const { servers, error, refresh } = useMcpServers(teamId)
  const now = useNow(30_000)

  // The caller's machines + the servers shared with this team, off the synced
  // shape — online-ness for the chips, and which rows may run a sign-in.
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const { data: userRows } = useLiveQuery((query) =>
    query.from({ u: userCollection })
  )
  const devices = useMemo(() => {
    const usersById = new Map(
      ((userRows ?? []) as User[]).map((user) => [user.id, user])
    )
    return composeDeviceList(
      (deviceRows ?? []) as Device[],
      usersById,
      now,
      currentUserId,
      teamId
    )
  }, [deviceRows, userRows, now, currentUserId, teamId])

  const [editTarget, setEditTarget] = useState<McpServerRow | `new` | null>(
    null
  )
  const [removeTarget, setRemoveTarget] = useState<McpServerRow | null>(null)
  const [busy, setBusy] = useState(false)
  const [dialogError, setDialogError] = useState<string | null>(null)
  const [flows, setFlows] = useState<Record<string, FlowUi>>({})
  const mountedRef = useRef(true)
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const setFlow = (key: string, value: FlowUi | null) =>
    setFlows((current) => {
      const next = { ...current }
      if (value) next[key] = value
      else delete next[key]
      return next
    })

  const save = async (draft: McpServerDraft) => {
    if (busy || editTarget === null) return
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
      if (editTarget === `new`) {
        await trpc.mcpServers.create.mutate(
          { teamId, ...fields },
          { context: { skipErrorToast: true } }
        )
      } else {
        await trpc.mcpServers.update.mutate(
          { id: editTarget.id, ...fields },
          { context: { skipErrorToast: true } }
        )
      }
      setEditTarget(null)
      await refresh()
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

  // The web-initiated, device-executed OAuth sign-in. The tab is opened
  // SYNCHRONOUSLY on the click (a popup blocker only allows that) and
  // navigated once the device hands back the authorize URL.
  const signIn = async (server: McpServerRow, device: SteerDevice) => {
    const key = flowKey(server.id, device.deviceId)
    if (flows[key] && flows[key].phase !== `failed`) return
    const label = device.deviceLabel || device.deviceId
    const tab = window.open(``, `_blank`)
    setFlow(key, { phase: `starting` })
    let navigated = false
    try {
      const begun = await trpc.mcpServers.beginOAuth.mutate(
        { serverId: server.id, deviceId: device.deviceId },
        { context: { skipErrorToast: true } }
      )
      setFlow(key, { phase: `waiting` })
      const startedAt = Date.now()
      while (mountedRef.current) {
        await sleep(OAUTH_POLL_MS)
        if (!mountedRef.current) break
        const flow = await trpc.mcpServers.getOAuthFlow.query({
          flowId: begun.flowId,
        })
        if (flow.authorizeUrl && !navigated) {
          navigated = true
          if (tab && !tab.closed) tab.location.href = flow.authorizeUrl
          else window.open(flow.authorizeUrl, `_blank`)
          setFlow(key, { phase: `browser` })
        }
        if (flow.status === `done`) {
          if (tab && !tab.closed && !navigated) tab.close()
          setFlow(key, null)
          toast.success(`Signed in to ${server.name} on ${label}`)
          await refresh()
          return
        }
        if (flow.status === `failed`) {
          if (tab && !tab.closed && !navigated) tab.close()
          const reason = flow.error ?? `The sign-in did not complete.`
          setFlow(key, { phase: `failed`, error: reason })
          toast.error(`Sign-in to ${server.name} failed`, { description: reason })
          await refresh()
          return
        }
        if (Date.now() - startedAt > OAUTH_GIVE_UP_MS) {
          setFlow(key, { phase: `failed`, error: `The sign-in timed out.` })
          return
        }
      }
    } catch (err) {
      if (tab && !tab.closed && !navigated) tab.close()
      const reason = trpcErrorMessage(err, `The sign-in could not be started.`)
      setFlow(key, { phase: `failed`, error: reason })
      toast.error(`Couldn't start the sign-in`, { description: reason })
    }
  }

  return (
    <div className="mb-6">
      <GlassSectionHeader
        label="MCP servers"
        trailing={
          isOwner ? (
            <Pill
              mode="action"
              onClick={() => {
                setDialogError(null)
                setEditTarget(`new`)
              }}
            >
              <AddIcon className="size-3" />
              Add server
            </Pill>
          ) : undefined
        }
      />
      <p className="mb-3 px-1 text-xs text-muted-foreground">
        Servers a coding run can connect to besides Exponential. Credentials
        stay on each machine; the server only keeps names and readiness.
      </p>

      {error && <p className="px-1 pb-2 text-xs text-destructive">{error}</p>}
      {servers === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : servers.length === 0 ? (
        <GlassRow className="text-sm text-muted-foreground">
          {isOwner
            ? `No MCP servers yet. Add one to offer it on the Agent page.`
            : `No MCP servers yet. The team owner can add one.`}
        </GlassRow>
      ) : (
        <div className="flex flex-col gap-2">
          {servers.map((server) => (
            <ServerRow
              key={server.id}
              server={server}
              devices={devices}
              now={now}
              isOwner={isOwner}
              flows={flows}
              onSignIn={(device) => void signIn(server, device)}
              onEdit={() => {
                setDialogError(null)
                setEditTarget(server)
              }}
              onRemove={() => setRemoveTarget(server)}
            />
          ))}
        </div>
      )}

      <McpServerDialog
        key={editTarget === null ? `closed` : editTarget === `new` ? `new` : editTarget.id}
        open={editTarget !== null}
        initial={
          editTarget && editTarget !== `new`
            ? draftFromServer(editTarget)
            : EMPTY_MCP_SERVER_DRAFT
        }
        mode={editTarget === `new` ? `create` : `edit`}
        busy={busy}
        error={dialogError}
        onOpenChange={(open) => {
          if (!open && !busy) setEditTarget(null)
        }}
        onSubmit={(draft) => void save(draft)}
      />

      <Dialog
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setRemoveTarget(null)
        }}
      >
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Remove server</DialogTitle>
            <DialogDescription>
              {`Remove ${removeTarget?.name ?? `this server`} from the team? Runs stop offering it; credentials already on machines are left alone.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel disabled={busy} onClick={() => setRemoveTarget(null)} />
            <Button variant="destructive" disabled={busy} onClick={() => void remove()}>
              {busy && <LoaderCircle className="animate-spin" />}
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function ServerRow({
  server,
  devices,
  now,
  isOwner,
  flows,
  onSignIn,
  onEdit,
  onRemove,
}: {
  server: McpServerRow
  devices: SteerDevice[]
  now: Date
  isOwner: boolean
  flows: Record<string, FlowUi>
  onSignIn: (device: SteerDevice) => void
  onEdit: () => void
  onRemove: () => void
}) {
  const http = server.transport === `http`
  const target = http
    ? server.url
    : [server.command, ...server.args].filter(Boolean).join(` `)
  const auth = server.auth as McpAuth
  const transport = server.transport as McpTransport
  return (
    <GlassRow className="flex-col items-stretch gap-2">
      <div className="flex items-center gap-3">
        <McpIcon className="size-4 shrink-0 text-foreground/70" />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium">
              {server.name}
            </span>
            {server.enabledByDefault && (
              <Pill
                size="sm"
                title="Preselected on the Agent page"
              >
                Default
              </Pill>
            )}
          </div>
          <div className="truncate text-xs text-muted-foreground" title={target ?? undefined}>
            {`${MCP_TRANSPORT_LABELS[transport] ?? server.transport} · ${target ?? ``} · ${MCP_AUTH_LABELS[auth] ?? server.auth}`}
          </div>
        </div>
        {isOwner && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="glass"
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
      </div>
      {auth !== `none` && (
        <ReadinessStrip
          server={server}
          devices={devices}
          now={now}
          flows={flows}
          onSignIn={onSignIn}
        />
      )}
    </GlassRow>
  )
}

/** One chip per device: what the machine last reported for this server, and
 * for OAuth rows the "Sign in on <device>" action on the caller's own
 * machines that run the flow (cap `mcp`, online). */
function ReadinessStrip({
  server,
  devices,
  now,
  flows,
  onSignIn,
}: {
  server: McpServerRow
  devices: SteerDevice[]
  now: Date
  flows: Record<string, FlowUi>
  onSignIn: (device: SteerDevice) => void
}) {
  const oauth = server.auth === `oauth`
  const known = new Set(devices.map((device) => device.deviceId))
  // Readiness rows for devices outside the synced list (a teammate's machine
  // visible through the server's own scoping) still get a plain chip.
  const extra = server.readiness.filter((entry) => !known.has(entry.deviceId))
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {devices.map((device) => {
        const entry = readinessFor(server, device.deviceId)
        const ready = serverReadyOn(server, device.deviceId, now)
        const label = readinessLabel(server, entry, now)
        const online = deviceIsOnline(device)
        const flow = flows[flowKey(server.id, device.deviceId)]
        const canSignIn =
          oauth &&
          deviceIsMine(device) &&
          online &&
          deviceSupportsMcp(device) &&
          (!ready || (entry !== null && readinessExpired(entry, now)))
        const deviceLabel = device.deviceLabel || device.deviceId
        const tone = ready
          ? `text-emerald-500`
          : entry && !entry.ready && entry.error
            ? `text-destructive`
            : `text-muted-foreground`
        const hint =
          !oauth && !ready
            ? secretSetupHint(server)
            : entry
              ? `Checked ${new Date(entry.checkedAt).toLocaleString()}`
              : undefined
        return (
          <span key={device.deviceId} className="flex items-center gap-1">
            <Pill size="sm" title={hint}>
              <span
                className={cn(
                  `size-1.5 rounded-full`,
                  online ? `bg-emerald-500` : `bg-muted-foreground/40`
                )}
                aria-hidden
              />
              <span className="max-w-[10rem] truncate">{deviceLabel}</span>
              <span className={cn(`max-w-[12rem] truncate`, tone)}>
                {!oauth && !ready ? `Set on the device` : label}
              </span>
            </Pill>
            {canSignIn && (
              <Pill
                size="sm"
                mode="action"
                disabled={Boolean(flow) && flow?.phase !== `failed`}
                title={flow?.phase === `failed` ? flow.error : undefined}
                onClick={() => onSignIn(device)}
              >
                {flow && flow.phase !== `failed` ? (
                  <LoaderCircle className="size-3 animate-spin" />
                ) : (
                  <SignInIcon className="size-3" />
                )}
                {flow?.phase === `waiting` || flow?.phase === `starting`
                  ? `Waiting for ${deviceLabel}…`
                  : flow?.phase === `browser`
                    ? `Finish in the browser`
                    : `Sign in on ${deviceLabel}`}
              </Pill>
            )}
          </span>
        )
      })}
      {extra.map((entry) => (
        <Pill key={entry.deviceRowId} size="sm">
          <span className="max-w-[10rem] truncate">{entry.deviceLabel}</span>
          <span
            className={cn(
              `max-w-[12rem] truncate`,
              entry.ready ? `text-emerald-500` : `text-muted-foreground`
            )}
          >
            {readinessLabel(server, entry, now)}
          </span>
        </Pill>
      ))}
    </div>
  )
}

// ── Add / edit dialog ────────────────────────────────────────────────────────

const TRANSPORT_OPTIONS = [
  { value: `http`, label: MCP_TRANSPORT_LABELS.http },
  { value: `stdio`, label: MCP_TRANSPORT_LABELS.stdio },
]

function McpServerDialog({
  open,
  initial,
  mode,
  busy,
  error,
  onOpenChange,
  onSubmit,
}: {
  open: boolean
  initial: McpServerDraft
  mode: `create` | `edit`
  busy: boolean
  error: string | null
  onOpenChange: (open: boolean) => void
  onSubmit: (draft: McpServerDraft) => void
}) {
  const [draft, setDraft] = useState<McpServerDraft>(initial)
  const patch = (fields: Partial<McpServerDraft>) =>
    setDraft((current) => ({ ...current, ...fields }))
  const http = draft.transport === `http`
  const validation = validateMcpServerDraft(draft)
  const authOptions = [
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
            {mode === `create` ? `Add MCP server` : `Edit MCP server`}
          </DialogTitle>
        </DialogHeader>
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
            <GlassPickerRow
              label="Transport"
              value={draft.transport}
              options={TRANSPORT_OPTIONS}
              onValueChange={(value) => {
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
            {http ? (
              <GlassInputRow
                id="mcp-url"
                label="URL"
                value={draft.url}
                placeholder="https://mcp.example.com/mcp"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                inputMode="url"
                onChange={(event) => patch({ url: event.target.value })}
              />
            ) : (
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
              label={http ? `Header names` : `Variable names`}
              values={http ? draft.headerNames : draft.envNames}
              placeholder={http ? `X-Api-Key` : `ACME_TOKEN`}
              pattern={MCP_VARIABLE_NAME_RE}
              onChange={(names) =>
                patch(http ? { headerNames: names } : { envNames: names })
              }
            />
            <GlassPickerRow
              label="Auth"
              value={draft.auth}
              options={authOptions}
              onValueChange={(value) => patch({ auth: value as McpAuth })}
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
            <GlassToggleRow
              id="mcp-enabled-by-default"
              label="Enabled by default"
              description="Preselected on the Agent page."
              checked={draft.enabledByDefault}
              onCheckedChange={(enabledByDefault) => patch({ enabledByDefault })}
            />
          </GlassGroup>
          <p className="px-1 text-[11px] text-muted-foreground">
            {draft.auth === `secret`
              ? http
                ? `Declare the one header that carries the secret. Its value is typed on each machine, never stored here.`
                : `Declare the one variable that carries the secret. Its value is typed on each machine, never stored here.`
              : draft.auth === `oauth`
                ? `Each member signs in on their own machine from this page. Tokens stay on the device.`
                : http
                  ? `Names only: any header value is typed on each machine.`
                  : `Names only: any variable value is typed on each machine.`}
          </p>
          {(validation || error) && (
            <p className="px-1 text-xs text-destructive">{error ?? validation}</p>
          )}
        </DialogBody>
        <DialogFooter>
          <DialogCancel disabled={busy} />
          <Button
            disabled={busy || validation !== null}
            onClick={() => onSubmit(draft)}
          >
            {busy && <LoaderCircle className="animate-spin" />}
            {mode === `create` ? `Add server` : `Save`}
          </Button>
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
  const commit = () => {
    const value = text.trim().replace(/,$/, ``)
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
      <label htmlFor={id} className="text-sm text-foreground">
        {label}
      </label>
      <div className="flex flex-wrap items-center gap-1.5">
        {values.map((value) => (
          <Pill key={value} size="sm">
            <span className="font-mono">{value}</span>
            <button
              type="button"
              className="-mr-0.5 rounded-full text-muted-foreground hover:text-foreground"
              aria-label={`Remove ${value}`}
              onClick={() => onChange(values.filter((v) => v !== value))}
            >
              <CloseIcon className="size-3" />
            </button>
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
            `h-6 min-w-[8rem] flex-1 rounded-none border-0 bg-transparent px-0 font-mono text-xs shadow-none focus-visible:ring-0`,
            invalid && `text-destructive`
          )}
          onChange={(event) => {
            setInvalid(false)
            const next = event.target.value
            if (next.endsWith(`,`) || next.endsWith(` `)) {
              setText(next.slice(0, -1))
              // Commit on the next tick so the trimmed text is what commits.
              queueMicrotask(() => {
                const value = next.slice(0, -1).trim()
                if (!value) return
                if (pattern && !pattern.test(value)) {
                  setInvalid(true)
                  return
                }
                if (!values.includes(value)) onChange([...values, value])
                setText(``)
              })
              return
            }
            setText(next)
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === `Enter`) {
              event.preventDefault()
              commit()
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
