import { useAppNames } from "@/hooks/use-apps"
import { withViaApp } from "@exp/domain-contract/via-app"
import { useCallback, useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { APPS_COPY } from "@exp/domain-contract/apps"
import {
  APP_DATA_COPY,
  APP_DATA_TAB,
  APP_HISTORY_COPY,
  historyRows,
  authorOptions,
  cellText,
  columnWidth,
  dataColumns,
  dataKinds,
  dataRows,
  formData,
  formFields,
  formAuthor,
  formInput,
  recordByline,
  type DataFormField,
  type DataSchema,
  type RecordEvent,
} from "@exp/domain-contract/app-records"
import { recordPolicy, recordRefusal, type AppAccessLevel, type RecordOp } from "@exponential_app/eui/host"
import {
  Button,
  conceptIcon,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  EmptyState,
  FEEDBACK_WORDS,
  GlassSectionHeader,
  Input,
  Label,
  ListRow,
  Menu,
  Prompt,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SETTINGS_LIST_CLASS,
  Switch,
} from "@exp/ui"
import type { User } from "@/db/schema"
import { relativeTime } from "@/components/comment-rows/format"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { toastRecordUndo } from "@/lib/apps/record-undo"
import { appRecordCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import { cn } from "@/lib/utils"

// VAPP-178 (fixtures/app-records.json `dataTab`): THE DATA TAB. The HEAD
// package's record kinds (a Select, `{kind} · {n}`) + Add record over the
// picked kind's rows: on md+ ONE grouped list used as a table (a header row,
// every row on the same grid), on phones one row per record (first column
// over the others). Add / Edit = a form (the Dialog; a sheet on phones);
// a BAD_REQUEST shows inline under it, anything else toasts. Delete asks.
// VAPP-215 (`history`): every write toasts Undo for 10 s; Edit lists the
// record's History (who · when · what) with Restore on older rows.

const NavAppsIcon = conceptIcon(`nav-apps`)
const UiAddIcon = conceptIcon(`ui-add`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

const T = APP_DATA_TAB.testIds
const NONE = `__none__`

interface DataRecord {
  id: string
  kind: string
  data: Record<string, unknown>
  createdBy: string | null
  authorId: string | null
  /** VAPP-229: the app whose tool created it ("via <app>"). */
  viaAppId: string | null
  updatedAt: string
}

type Inputs = Record<string, string | boolean>
type FormState = { kind: string; record: DataRecord | null }

const iso = (value: Date | string) => (typeof value === `string` ? value : value.toISOString())

export function AppDataTab({
  appId,
  pkg,
  users,
  me,
  role,
  access = `write`,
  isMobile,
}: {
  appId: string
  /** The HEAD package (null = a draft: no kinds). */
  pkg: Record<string, unknown> | null
  users: readonly User[]
  /** VAPP-187: the viewer and their team role (null = not a member). */
  me: string | null
  role: string | null
  /** eApps R3: the viewer's app access (no row = write). */
  access?: AppAccessLevel
  isMobile: boolean
}) {
  const { data: synced } = useLiveQuery(
    (query) => query.from({ r: appRecordCollection }).where(({ r }) => eq(r.appId, appId)),
    [appId]
  )
  const records = useMemo<DataRecord[]>(
    () =>
      (synced ?? []).map((row) => ({
        id: row.id,
        kind: row.kind,
        data: (row.data ?? {}) as Record<string, unknown>,
        createdBy: row.createdBy ?? null,
        authorId: row.authorId ?? null,
        viaAppId: row.viaAppId ?? null,
        updatedAt: iso(row.updatedAt),
      })),
    [synced]
  )
  const manifest = pkg as { records?: Record<string, { schema: DataSchema }> } | null
  const kinds = useMemo(() => dataKinds(manifest, records), [manifest, records])
  const [picked, setPicked] = useState<string | null>(null)
  const kind = picked && kinds.some((k) => k.kind === picked) ? picked : (kinds[0]?.kind ?? null)
  const schema = kind ? (manifest?.records?.[kind]?.schema ?? null) : null
  // eApps R3: the kind's policy under the viewer's app access
  // (record-policy.json): what the server would refuse stays disabled.
  const refusal = (op: RecordOp, record?: DataRecord) => (kind ? recordRefusal(recordPolicy(manifest?.records?.[kind]), op, { id: me, role, access }, record) : null)
  const addRefusal = refusal(`create`)
  const columns = useMemo(() => (schema ? dataColumns(schema) : []), [schema])
  const rows = useMemo(() => (kind ? dataRows(records, kind) : []), [records, kind])

  const [form, setForm] = useState<FormState | null>(null)
  const [deleting, setDeleting] = useState<DataRecord | null>(null)
  const [deleteBusy, setDeleteBusy] = useState(false)

  if (kinds.length === 0) {
    return <EmptyState icon={NavAppsIcon} title={APP_DATA_COPY.noKinds} size="list" />
  }

  const confirmDelete = async () => {
    if (!deleting) return
    setDeleteBusy(true)
    try {
      const result = await trpc.apps.records.delete.mutate({ id: appId, recordId: deleting.id, via: `editor` }, SKIP_ERROR_TOAST)
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      setDeleting(null)
      toastRecordUndo(appId, `delete`, [result.eventId], `editor`)
    } catch (error) {
      failedAction(`delete`, `the record`, error)
    } finally {
      setDeleteBusy(false)
    }
  }

  const rowMenu = (record: DataRecord) => (
    <Menu
      align="end"
      aria-label={APP_DATA_COPY.edit}
      entries={[
        ...(refusal(`update`, record) === null ? [{ kind: `item` as const, id: `edit`, label: APP_DATA_COPY.edit, icon: UiEditIcon, onSelect: () => kind && setForm({ kind, record }) }] : []),
        ...(refusal(`delete`, record) === null
          ? [
              {
                kind: `item` as const,
                id: `delete`,
                label: APP_DATA_COPY.delete,
                icon: UiDeleteIcon,
                destructive: true,
                onSelect: () => setDeleting(record),
              },
            ]
          : []),
      ]}
      trigger={
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`${APP_DATA_COPY.edit} / ${APP_DATA_COPY.delete}`}
          data-testid={T.menu}
          disabled={refusal(`update`, record) !== null && refusal(`delete`, record) !== null}
        >
          <UiMoreIcon />
        </Button>
      }
    />
  )

  // VAPP-188: the author, `{writer} for {author}` on an on-behalf row.
  const names = useMemo(() => Object.fromEntries(users.map((u) => [u.id, u.name])), [users])
  // VAPP-229 (via-app.json): an app's tool wrote it = `· via <app>`.
  const appNames = useAppNames()
  const author = (record: DataRecord) => withViaApp(recordByline(record, names, APPS_COPY.byAgent), record.viaAppId ? appNames.get(record.viaAppId) : null)

  const grid = {
    gridTemplateColumns: [
      ...columns.map((name) => {
        const width = schema ? columnWidth(schema.properties[name]!) : null
        return width === null ? `minmax(${APP_DATA_TAB.columns.minWidth}px, 1fr)` : `${width}px`
      }),
      `${APP_DATA_TAB.columns.byWidth}px`,
      `${APP_DATA_TAB.columns.updatedWidth}px`,
      `${APP_DATA_TAB.columns.menuWidth}px`,
    ].join(` `),
    columnGap: APP_DATA_TAB.columns.gap,
  }

  // Wider than the pane = the table scrolls sideways (the menu stays reachable).
  const table = (
    <div className="overflow-x-auto">
    <div className={cn(SETTINGS_LIST_CLASS, `min-w-fit`)}>
      <ListRow className="grid px-3 py-1.5 text-xs text-muted-foreground" style={grid}>
        {columns.map((name) => (
          <span key={name} className="truncate">
            {name}
          </span>
        ))}
        <span className="truncate">{APP_DATA_COPY.by}</span>
        <span className="truncate">{APP_DATA_COPY.updated}</span>
        <span />
      </ListRow>
      {rows.map((record) => (
        <ListRow key={record.id} className="grid px-3 py-1.5 text-sm" style={grid} data-testid={T.row}>
          {columns.map((name) => (
            <span key={name} className="truncate">
              {cellText(record.data[name])}
            </span>
          ))}
          <span className="truncate text-muted-foreground">{author(record)}</span>
          <span className="truncate text-muted-foreground">{relativeTime(record.updatedAt)}</span>
          <span className="flex justify-end">{rowMenu(record)}</span>
        </ListRow>
      ))}
    </div>
    </div>
  )

  const list = (
    <div className={SETTINGS_LIST_CLASS}>
      {rows.map((record) => {
        const title = columns.length > 0 ? cellText(record.data[columns[0]!]) : ``
        const subtitle = columns
          .slice(1)
          .map((name) => cellText(record.data[name]))
          .filter((text) => text !== ``)
          .join(` · `)
        return (
          <ListRow key={record.id} className="gap-3 px-3 py-2 text-sm" data-testid={T.row}>
            <div className="min-w-0 flex-1">
              <div className={cn(`truncate`, title ? `font-medium` : `text-muted-foreground`)}>{title || APP_DATA_COPY.emptyTitle}</div>
              {subtitle && <div className="mt-0.5 truncate text-xs text-muted-foreground">{subtitle}</div>}
            </div>
            <span className="shrink-0 text-xs text-muted-foreground">{relativeTime(record.updatedAt)}</span>
            {rowMenu(record)}
          </ListRow>
        )
      })}
    </div>
  )

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Select value={kind ?? undefined} onValueChange={setPicked}>
          <SelectTrigger size={isMobile ? `sm` : `default`} className="min-w-0" data-testid={T.kind}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {kinds.map((k) => (
              <SelectItem key={k.kind} value={k.kind}>
                {APP_DATA_COPY.kindOption.replace(`{kind}`, k.kind).replace(`{n}`, String(k.count))}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex-1" />
        <Button
          variant="outline"
          size={isMobile ? `sm` : `default`}
          onClick={() => kind && setForm({ kind, record: null })}
          disabled={addRefusal !== null}
          title={addRefusal ?? undefined}
          data-testid={T.add}
        >
          <UiAddIcon />
          {APP_DATA_COPY.addRecord}
        </Button>
      </div>
      {rows.length === 0 ? (
        <EmptyState icon={NavAppsIcon} title={APP_DATA_COPY.empty} size="list" />
      ) : isMobile ? (
        list
      ) : (
        table
      )}
      <RecordForm
        appId={appId}
        state={form}
        schema={form ? (manifest?.records?.[form.kind]?.schema ?? null) : null}
        me={me}
        authors={role === `owner` ? users : null}
        names={names}
        onClose={() => setForm(null)}
      />
      <Prompt
        open={deleting !== null}
        onOpenChange={(next) => !next && !deleteBusy && setDeleting(null)}
        busy={deleteBusy}
        title={APP_DATA_COPY.deleteTitle}
        body={APP_DATA_COPY.deleteBody}
        actions={[
          { label: APP_DATA_COPY.cancel, role: `cancel` },
          { label: APP_DATA_COPY.delete, role: `destructive`, busy: deleteBusy, onSelect: confirmDelete },
        ]}
      />
    </div>
  )
}

/** Add / Edit: one labelled field per column (`formFields`), pre-filled with
 *  `formInput`; Save turns the inputs into data with `formData`. */
function RecordForm({
  appId,
  state,
  schema,
  me,
  authors,
  names,
  onClose,
}: {
  appId: string
  state: FormState | null
  schema: DataSchema | null
  me: string | null
  /** VAPP-188: the team's members for an OWNER's Author picker (null = no picker). */
  authors: readonly User[] | null
  /** VAPP-215: member names for the History rows. */
  names: Readonly<Record<string, string>>
  onClose: () => void
}) {
  const open = state !== null && schema !== null
  const fields = useMemo(() => (schema ? formFields(schema) : []), [schema])
  const [inputs, setInputs] = useState<Inputs>({})
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [author, setAuthor] = useState<string | null>(null)
  const editing = state?.record != null
  const authorChoices = useMemo(() => (authors ? authorOptions(authors) : null), [authors])

  // Seed on OPEN only.
  useEffect(() => {
    if (!open) return
    const seeded: Inputs = {}
    for (const field of fields) seeded[field.name] = formInput(field.control, state?.record?.data[field.name])
    setInputs(seeded)
    setAuthor(formAuthor(me, state?.record ?? null, null).default)
    setError(null)
    setSaving(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, state])

  const set = (name: string, value: string | boolean) => {
    setInputs((prev) => ({ ...prev, [name]: value }))
    if (error) setError(null)
  }

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!state || !schema || saving) return
    setSaving(true)
    setError(null)
    try {
      const data = formData(schema, inputs, editing)
      const send = authorChoices ? formAuthor(me, state.record, author).send : null
      const result = await trpc.apps.records.upsert.mutate(
        { id: appId, kind: state.kind, data, ...(state.record ? { recordId: state.record.id } : {}), ...(send ? { author: send } : {}), via: `editor` },
        SKIP_ERROR_TOAST
      )
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      onClose()
      toastRecordUndo(appId, result.created ? `create` : `update`, [result.eventId], `editor`)
    } catch (err) {
      if (trpcErrorCode(err) === `BAD_REQUEST`) setError(trpcErrorMessage(err, `record is invalid`))
      else failedAction(`save`, `the record`, err)
    } finally {
      setSaving(false)
    }
  }

  const title = state ? (editing ? APP_DATA_COPY.editTitle : APP_DATA_COPY.newTitle).replace(`{kind}`, state.kind) : ``

  return (
    <Dialog open={open} onOpenChange={(next) => !next && !saving && onClose()}>
      <DialogContent mobile="sheet-full" className="sm:max-h-[85dvh] sm:max-w-lg" data-testid={T.form}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col gap-3">
          <DialogBody className="min-h-0 space-y-3 overflow-y-auto">
            {authorChoices && (
              <FieldRow
                field={{ name: APP_DATA_COPY.author, control: `select`, required: true, options: authorChoices.map((u) => u.id) }}
                optionLabel={(id) => authorChoices.find((u) => u.id === id)?.name ?? id}
                value={author ?? ``}
                onChange={(value) => typeof value === `string` && setAuthor(value)}
              />
            )}
            {fields.map((field) => (
              <FieldRow key={field.name} field={field} value={inputs[field.name]} onChange={(value) => set(field.name, value)} />
            ))}
            {error && (
              <p className="text-sm text-destructive" role="alert" data-testid={T.error}>
                {error}
              </p>
            )}
            {open && state?.record && <RecordHistory appId={appId} recordId={state.record.id} names={names} onRestored={onClose} />}
          </DialogBody>
          <DialogFooter>
            <DialogCancel disabled={saving}>{APP_DATA_COPY.cancel}</DialogCancel>
            <Button type="submit" pending={saving} pendingLabel={FEEDBACK_WORDS.pending.save} data-testid={T.save}>
              {APP_DATA_COPY.save}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** VAPP-215: a record's History (read on demand): newest first, who · when
 *  under the label, Restore on every older row but deletes. */
function RecordHistory({ appId, recordId, names, onRestored }: { appId: string; recordId: string; names: Readonly<Record<string, string>>; onRestored: () => void }) {
  const [events, setEvents] = useState<RecordEvent[] | null>(null)
  const [restoring, setRestoring] = useState<string | null>(null)
  const load = useCallback(async () => {
    try {
      const result = await trpc.apps.records.history.query({ id: appId, recordId })
      setEvents(result.events.map((e) => ({ ...e, createdAt: iso(e.createdAt) })))
    } catch {
      setEvents([])
    }
  }, [appId, recordId])
  useEffect(() => {
    setEvents(null)
    void load()
  }, [load])

  const restore = async (eventId: string) => {
    setRestoring(eventId)
    try {
      const result = await trpc.apps.records.restore.mutate({ id: appId, recordId, eventId, via: `editor` }, SKIP_ERROR_TOAST)
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      onRestored()
      toastRecordUndo(appId, `update`, result.eventIds, `editor`)
    } catch (error) {
      failedAction(`restore`, `the record`, error)
    } finally {
      setRestoring(null)
    }
  }

  const rows = events ? historyRows(events, names, APPS_COPY.byAgent) : []
  return (
    <div className="space-y-1.5 pt-2" data-testid={T.history}>
      <GlassSectionHeader label={APP_HISTORY_COPY.history} loading={events === null} />
      {events !== null && rows.length === 0 ? (
        <EmptyState title={APP_HISTORY_COPY.empty} size="list" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {rows.map((row) => (
            <ListRow key={row.id} className="gap-3 px-3 py-2 text-sm" data-testid={T.historyRow}>
              <div className="min-w-0 flex-1">
                <div className="truncate">{row.label}</div>
                <div className="mt-0.5 truncate text-xs text-muted-foreground">
                  {row.who} · {relativeTime(row.at)}
                </div>
              </div>
              {row.restorable && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  pending={restoring === row.id}
                  disabled={restoring !== null}
                  onClick={() => void restore(row.id)}
                  data-testid={T.restore}
                >
                  {APP_HISTORY_COPY.restore}
                </Button>
              )}
            </ListRow>
          ))}
        </div>
      )}
    </div>
  )
}

function FieldRow({
  field,
  value,
  onChange,
  optionLabel = (option) => option,
}: {
  field: DataFormField
  value: string | boolean | undefined
  onChange: (value: string | boolean) => void
  /** VAPP-188: the Author picker's options are user ids shown by name. */
  optionLabel?: (option: string) => string
}) {
  const id = `app-data-field-${field.name}`
  const label = (
    <Label htmlFor={id} className="gap-1.5">
      {field.name}
      {!field.required && <span className="font-normal text-muted-foreground">{APP_DATA_COPY.optional}</span>}
    </Label>
  )
  if (field.control === `switch`) {
    return (
      <div className="flex items-center justify-between gap-3">
        {label}
        <Switch id={id} checked={value === true} onCheckedChange={(checked) => onChange(checked)} />
      </div>
    )
  }
  const text = typeof value === `string` ? value : ``
  let control
  if (field.control === `select`) {
    control = (
      <Select value={text === `` ? (field.required ? undefined : NONE) : text} onValueChange={(next) => onChange(next === NONE ? `` : next)}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {!field.required && <SelectItem value={NONE}>{APP_DATA_COPY.none}</SelectItem>}
          {(field.options ?? []).map((option) => (
            <SelectItem key={option} value={option}>
              {optionLabel(option)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    )
  } else {
    control = (
      <Input
        id={id}
        value={text}
        inputMode={field.control === `number` ? `decimal` : undefined}
        placeholder={field.control === `list` ? APP_DATA_COPY.listHint : field.placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
    )
  }
  return (
    <div className="space-y-1.5">
      {label}
      {control}
    </div>
  )
}
