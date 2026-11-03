import { useEffect, useMemo, useRef, useState } from "react"
import {
  APP_CSV_COPY,
  APP_CSV_IMPORT,
  APP_DATA_COPY,
  cellText,
  csvAuthorColumn,
  csvAutoMap,
  csvParse,
  csvRecords,
  csvSummary,
  dataColumns,
  type CsvRecord,
  type DataSchema,
} from "@exp/domain-contract/app-records"
import { validateRecord, type RecordSchema } from "@exponential_app/eui/host"
import {
  Button,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Label,
  ListRow,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SETTINGS_LIST_CLASS,
  Textarea,
} from "@exp/ui"
import type { User } from "@/db/schema"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { toastRecordUndo } from "@/lib/apps/record-undo"
import { appRecordCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"

// VAPP-228 (X2, fixtures/app-records.json `csvImport`): the Data tab's CSV
// import, the app import dialog's pattern: paste or choose a CSV, map the
// columns onto the kind's fields (auto-matched), preview the first rows with
// their issues, then ONE `apps.records.batch` (`via: import`) of the ready
// rows; the toast's Undo deletes them again.

const T = APP_CSV_IMPORT.testIds
const SKIP = `__skip__`

export function AppCsvImportDialog({
  appId,
  kind,
  schema,
  me,
  members,
  open,
  onOpenChange,
}: {
  appId: string
  kind: string
  schema: DataSchema
  me: string | null
  /** An OWNER's team members for the Author column (null = no Author pick). */
  members: readonly User[] | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [text, setText] = useState(``)
  const [mapping, setMapping] = useState<Record<string, number | null>>({})
  const [authorColumn, setAuthorColumn] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)

  const fields = useMemo(() => dataColumns(schema), [schema])
  const table = useMemo(() => csvParse(text), [text])
  const headerKey = table.header.join(`\u0000`)

  // A new header = a new first pick.
  useEffect(() => {
    setMapping(csvAutoMap(fields, table.header))
    setAuthorColumn(members ? csvAuthorColumn(table.header) : null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [headerKey, fields, members])

  const records = useMemo<CsvRecord[]>(() => {
    const author = members && me ? { column: authorColumn, importer: me, members: members.map((u) => ({ id: u.id, name: u.name, email: u.email })) } : undefined
    return csvRecords(schema, table.header, table.rows, mapping, author).map((record) => {
      if (record.issues.length > 0) return record
      const issues = validateRecord(schema as unknown as RecordSchema, record.data).map((issue) =>
        APP_CSV_COPY.issue.replace(`{row}`, String(record.row)).replace(`{field}`, issue.path.replace(/^\//, ``)).replace(`{message}`, issue.message)
      )
      return issues.length > 0 ? { ...record, issues } : record
    })
  }, [schema, table, mapping, authorColumn, members, me])

  const ready = records.filter((r) => r.issues.length === 0)
  const tooMany = records.length > APP_CSV_IMPORT.maxRows

  const reset = () => {
    setText(``)
    setError(null)
  }

  const runImport = async () => {
    if (submitting || tooMany || ready.length === 0) return
    setSubmitting(true)
    setError(null)
    try {
      const writes = ready.map((r) => ({ op: `upsert` as const, kind, data: r.data, ...(r.author ? { author: r.author } : {}) }))
      const result = await trpc.apps.records.batch.mutate({ id: appId, writes, via: `import` }, SKIP_ERROR_TOAST)
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      reset()
      onOpenChange(false)
      toastRecordUndo(appId, `batch`, result.eventIds, `editor`, APP_CSV_COPY.imported.replace(`{n}`, String(result.ids.length)))
    } catch (err) {
      if (trpcErrorCode(err) === `BAD_REQUEST`) setError(trpcErrorMessage(err, `the rows are invalid`))
      else failedAction(`import`, `the records`, err)
    } finally {
      setSubmitting(false)
    }
  }

  const columnSelect = (id: string, value: number | null, onChange: (next: number | null) => void, none: string, noneFirst = false) => (
    <Select value={value === null ? SKIP : String(value)} onValueChange={(next) => onChange(next === SKIP ? null : Number(next))}>
      <SelectTrigger id={id} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {noneFirst && <SelectItem value={SKIP}>{none}</SelectItem>}
        {table.header.map((name, i) => (
          <SelectItem key={i} value={String(i)}>
            {name}
          </SelectItem>
        ))}
        {!noneFirst && <SelectItem value={SKIP}>{none}</SelectItem>}
      </SelectContent>
    </Select>
  )

  const required = schema.required ?? []
  const preview = records.slice(0, APP_CSV_IMPORT.previewRows)

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (submitting) return
        if (!next) reset()
        onOpenChange(next)
      }}
    >
      <DialogContent mobile="sheet-full" className="sm:max-h-[85dvh] sm:max-w-lg" data-testid={T.dialog}>
        <DialogHeader>
          <DialogTitle>{APP_CSV_COPY.title.replace(`{kind}`, kind)}</DialogTitle>
        </DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-3">
          <DialogBody className="min-h-0 space-y-3 overflow-y-auto">
            <Textarea
              value={text}
              onChange={(event) => {
                setText(event.target.value)
                setError(null)
              }}
              placeholder={APP_CSV_COPY.placeholder}
              rows={6}
              autoFocus
              spellCheck={false}
              className="font-mono text-xs"
              data-testid={T.text}
            />
            <input
              ref={fileInput}
              type="file"
              accept=".csv,.tsv,.txt,text/csv,text/tab-separated-values,text/plain"
              className="hidden"
              onChange={(event) => void event.target.files?.[0]?.text().then((next) => (setText(next), setError(null)))}
            />
            <Button type="button" variant="outline" size="sm" onClick={() => fileInput.current?.click()} data-testid={T.file}>
              {APP_CSV_COPY.chooseFile}
            </Button>
            {table.header.length > 0 && (
              <div className="space-y-3" data-testid={T.map}>
                {members && (
                  <div className="space-y-1.5" data-testid={T.author}>
                    <Label htmlFor="app-csv-author">{APP_DATA_COPY.author}</Label>
                    {columnSelect(`app-csv-author`, authorColumn, setAuthorColumn, APP_CSV_COPY.me, true)}
                  </div>
                )}
                {fields.map((name) => (
                  <div key={name} className="space-y-1.5">
                    <Label htmlFor={`app-csv-field-${name}`} className="gap-1.5">
                      {name}
                      {!required.includes(name) && <span className="font-normal text-muted-foreground">{APP_DATA_COPY.optional}</span>}
                    </Label>
                    {columnSelect(`app-csv-field-${name}`, mapping[name] ?? null, (next) => setMapping((prev) => ({ ...prev, [name]: next })), APP_CSV_COPY.skip)}
                  </div>
                ))}
              </div>
            )}
            {text.trim() !== `` && records.length === 0 && <p className="text-sm text-muted-foreground">{APP_CSV_COPY.noRows}</p>}
            {records.length > 0 && (
              <div className="space-y-1.5" data-testid={T.preview}>
                <div className={SETTINGS_LIST_CLASS}>
                  {preview.map((record) => {
                    const title = fields.length > 0 ? cellText(record.data[fields[0]!]) : ``
                    const subtitle = fields
                      .slice(1)
                      .map((name) => cellText(record.data[name]))
                      .filter((t) => t !== ``)
                      .join(` · `)
                    return (
                      <ListRow key={record.row} className="block px-3 py-2 text-sm">
                        <div className={title ? `truncate font-medium` : `truncate text-muted-foreground`}>{title || APP_DATA_COPY.emptyTitle}</div>
                        {subtitle && <div className="mt-0.5 truncate text-xs text-muted-foreground">{subtitle}</div>}
                        {record.issues.length > 0 && (
                          <ul className="mt-1 space-y-0.5 font-mono text-xs text-destructive">
                            {record.issues.slice(0, APP_CSV_IMPORT.issuesShown).map((issue, i) => (
                              <li key={i} className="break-words">
                                {issue}
                              </li>
                            ))}
                            {record.issues.length > APP_CSV_IMPORT.issuesShown && (
                              <li>{APP_CSV_COPY.moreIssues.replace(`{n}`, String(record.issues.length - APP_CSV_IMPORT.issuesShown))}</li>
                            )}
                          </ul>
                        )}
                      </ListRow>
                    )
                  })}
                </div>
                <p className="text-xs text-muted-foreground">{csvSummary(records)}</p>
                {tooMany && (
                  <p className="text-sm text-destructive" role="alert">
                    {APP_CSV_COPY.tooMany.replace(`{max}`, String(APP_CSV_IMPORT.maxRows))}
                  </p>
                )}
              </div>
            )}
            {error && (
              <p className="text-sm text-destructive" role="alert" data-testid={T.error}>
                {error}
              </p>
            )}
          </DialogBody>
          <DialogFooter>
            <DialogCancel disabled={submitting}>{APP_DATA_COPY.cancel}</DialogCancel>
            <Button onClick={() => void runImport()} disabled={tooMany || ready.length === 0} pending={submitting} data-testid={T.confirm}>
              {APP_CSV_COPY.importAction.replace(`{n}`, String(ready.length))}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
