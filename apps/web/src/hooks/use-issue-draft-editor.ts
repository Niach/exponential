import { useEffect, useRef, useState } from "react"
import { toast } from "@exp/ui"
import type { Board, IssueDraft, User } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { toIssueDescription, type IssuePriority } from "@/lib/domain"
import { ISSUE_DRAFT_AUTOSAVE_MS, canCreateDraft } from "@/lib/issue-draft-page"
import {
  hasDraftContent,
  toDialogSeed,
  toUpsertInput,
  type DraftSnapshot,
} from "@/lib/issue-drafts"
import {
  isFallbackStatusOption,
  statusUpdatePayload,
  type StatusRowOption,
} from "@/lib/team-statuses"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { trpc } from "@/lib/trpc-client"
import type { FilesSectionFile } from "@/components/issue-files-section"

// EXP-1170: the New issue page's controller — the issue detail in DRAFT mode.
// Everything the page holds lands in ONE `issue_drafts` row through ONE
// coalesced, serialised write:
//
//   * 800 ms (`ISSUE_DRAFT_AUTOSAVE_MS`) after the last title/description
//     edit; at once on a property pick; at once on a title/description blur,
//     on leaving (Back, any navigation, unmount, `pagehide`) and before
//     Create. Every write is the FULL row (`toUpsertInput`), read from the
//     latest render when it RUNS, so a snapshot that changed mid-request is
//     written after it and an identical one is skipped.
//   * A row is only ever created by content (`hasDraftContent`): chips alone
//     never write. An existing row emptied out is deleted on leave.
//   * The first eager upload needs the row first (`ensureDraft`, cached per
//     page: five pasted images insert one row).
//   * Create flushes, files the issue with `draftId` (the server consumes the
//     row in the same transaction) and finalises the page so the leave write
//     never resurrects it. Discard deletes and finalises.

export interface IssueDraftEditorOptions {
  draftId: string
  teamId: string
  /** The board a fresh draft files onto (`?board=`, else the default). */
  initialBoardId: string
  /** The group status a "+" seeded (`?status=`). */
  initialStatusId?: string | null
  /** The synced row present when the page opened — seeds it ONCE. */
  draft?: IssueDraft
  boards: readonly Board[]
  labels: readonly { id: string }[]
  users: readonly User[]
}

export interface CreatedIssue {
  identifier: string
  boardSlug: string | null
}

type WriteMode = `edit` | `leave`

export function useIssueDraftEditor({
  draftId,
  teamId,
  initialBoardId,
  initialStatusId = null,
  draft,
  boards,
  labels,
  users,
}: IssueDraftEditorOptions) {
  const { resolve: resolveStatus } = useTeamStatusesContext()

  // Seeded ONCE from the row present on open; later shape echoes of our own
  // writes must never overwrite what is being typed.
  const [seed] = useState(() =>
    draft ? toDialogSeed(draft, { boards, labels, users }) : null
  )
  const [title, setTitleState] = useState(seed?.title ?? ``)
  const [description, setDescriptionState] = useState(seed?.description ?? ``)
  const [boardId, setBoardIdState] = useState(seed?.boardId ?? initialBoardId)
  const [statusId, setStatusId] = useState<string | null>(
    seed ? seed.statusId : initialStatusId
  )
  const [priority, setPriorityState] = useState<IssuePriority>(
    seed?.priority ?? `none`
  )
  // In a solo team (exactly one human member) the assignee control is hidden
  // and new issues default to the sole member — the server's own default.
  // Applied in the INITIAL state too, so the reopened row's "already
  // written" key below includes it and merely looking at a draft never
  // rewrites it. `users` length 0 = still loading, never a genuine empty.
  const soleMemberId = users.length === 1 ? users[0].id : null
  const [assigneeId, setAssigneeIdState] = useState<string | null>(
    soleMemberId ?? seed?.assigneeId ?? null
  )
  const [labelIds, setLabelIds] = useState<string[]>(seed?.labelIds ?? [])
  const [dueDate, setDueDateState] = useState<string | null>(
    seed?.dueDate ?? null
  )
  const [files, setFiles] = useState<FilesSectionFile[]>([])
  const [creating, setCreating] = useState(false)
  // Eager uploads in flight (the page reports them): Create waits for them,
  // or the issue would be filed without the file the person just dropped.
  const [uploadCount, setUploadCount] = useState(0)
  // Bumped by every property pick; the effect below flushes once the pick
  // has rendered into the snapshot.
  const [chipVersion, setChipVersion] = useState(0)

  // `null` = no pick: the team's Backlog builtin, resolved live so a late
  // `issue_statuses` snapshot upgrades the constructed fallback in place.
  const status: StatusRowOption = resolveStatus({
    status: `backlog`,
    statusId,
  })

  const snapshotRef = useRef<DraftSnapshot>(null as unknown as DraftSnapshot)
  snapshotRef.current = {
    id: draftId,
    teamId,
    boardId,
    title,
    description,
    // A constructed fallback row has no real id — the draft then carries NULL
    // and resolves to the team's Backlog, which is what it means anyway.
    statusId: isFallbackStatusOption(status) ? null : status.id,
    priority,
    assigneeId,
    labelIds,
    dueDate,
    attachmentCount: files.length,
  }
  const statusRef = useRef(status)
  statusRef.current = status
  const boardsRef = useRef(boards)
  boardsRef.current = boards

  const rowExistsRef = useRef(draft != null)
  // What the row holds as far as this page knows: the reopened row's own
  // payload, so leaving an untouched draft writes nothing.
  const lastWrittenRef = useRef<string | null>(null)
  const initializedRef = useRef(false)
  if (!initializedRef.current) {
    initializedRef.current = true
    if (draft) lastWrittenRef.current = JSON.stringify(toUpsertInput(snapshotRef.current))
  }
  // A reopened draft's attachments are a fetch: until it SUCCEEDED the page
  // cannot know the row holds no file, so an emptied title/description must
  // not delete it (and its files) on leave. A fresh page has none to load.
  const filesLoadedRef = useRef(draft == null)
  const uploadCountRef = useRef(0)
  const failedWritesRef = useRef(0)
  const ensureDraftRef = useRef<Promise<string> | null>(null)
  const writeChainRef = useRef<Promise<void>>(Promise.resolve())
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const finalizedRef = useRef(false)
  const creatingRef = useRef(false)
  // EXP-1212 (R2): the WHOLE Create, from its first flush to the server's
  // answer. `creatingRef` above only covers the request itself (it mutes
  // the autosave, which the pre-create flush still needs); this one gates
  // the leave prompt and Discard.
  const createInFlightRef = useRef(false)

  const clearTimer = () => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current)
      timerRef.current = null
    }
  }

  /** Serialise a write behind every earlier one; the chain never rejects. */
  const enqueue = (task: () => Promise<void>): Promise<void> => {
    const next = writeChainRef.current.then(task, task)
    writeChainRef.current = next.catch(() => undefined)
    return next
  }

  const writeTask = (mode: WriteMode) => async () => {
    if (finalizedRef.current || creatingRef.current) return
    const snapshot = snapshotRef.current
    if (hasDraftContent(snapshot)) {
      const input = toUpsertInput(snapshot)
      const key = JSON.stringify(input)
      if (rowExistsRef.current && key === lastWrittenRef.current) return
      await trpc.issueDrafts.upsert.mutate(input)
      rowExistsRef.current = true
      lastWrittenRef.current = key
      return
    }
    // Emptied out: the row goes, but only on the way out — mid-edit an empty
    // field is just a moment between two thoughts — and only once the
    // attachments are known to be none.
    if (mode === `leave` && rowExistsRef.current && filesLoadedRef.current) {
      await trpc.issueDrafts.delete.mutate({ id: snapshot.id })
      rowExistsRef.current = false
      lastWrittenRef.current = null
      ensureDraftRef.current = null
    }
  }

  /**
   * Write NOW (cancelling a pending debounce). Never rejects: resolves
   * `true` when the write landed (or had nothing to do), `false` when it
   * failed. Debounced writes fail quietly (the next one retries with the
   * full row), but a failed LEAVE write — the last chance — and a second
   * failure in a row say so once.
   */
  const flush = (mode: WriteMode = `edit`): Promise<boolean> => {
    clearTimer()
    return enqueue(writeTask(mode)).then(
      () => {
        failedWritesRef.current = 0
        return true
      },
      () => {
        failedWritesRef.current += 1
        if (mode === `leave` || failedWritesRef.current === 2) {
          toast.error(`Could not save the draft`)
        }
        return false
      }
    )
  }

  const schedule = () => {
    clearTimer()
    timerRef.current = setTimeout(() => {
      timerRef.current = null
      void flush(`edit`)
    }, ISSUE_DRAFT_AUTOSAVE_MS)
  }

  /** The leave write. EXP-1212 (R3): `false` = it failed (and said so), so
   *  "Keep as draft" stays on the page. */
  const leave = () => flush(`leave`)

  // Property picks write at once — after the pick has rendered.
  useEffect(() => {
    if (chipVersion > 0) void flush(`edit`)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chipVersion])

  useEffect(() => {
    if (soleMemberId) setAssigneeIdState(soleMemberId)
  }, [soleMemberId])

  // Draft attachments are server-only (the attachments shape drops them), so
  // a reopened draft's Files section is a fetch, not a live query.
  useEffect(() => {
    if (!draft) return
    let cancelled = false
    void trpc.issueDrafts.listAttachments
      .query({ id: draftId })
      .then((rows) => {
        if (cancelled) return
        filesLoadedRef.current = true
        setFiles(
          rows.map((row) => ({
            id: row.id,
            filename: row.filename,
            contentType: row.contentType,
            sizeBytes: row.sizeBytes,
            url: row.url,
          }))
        )
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
    // The seed row decides it, once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Leaving is a write: unmount (any navigation) and the tab going away.
  // Refs only, so the mount-time closures stay correct for the page's life.
  useEffect(() => {
    const onPageHide = () => void flush(`leave`)
    window.addEventListener(`pagehide`, onPageHide)
    return () => {
      window.removeEventListener(`pagehide`, onPageHide)
      void flush(`leave`)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const bumpChips = () => setChipVersion((version) => version + 1)

  /**
   * Make sure the row exists before anything uploads against it (the upload
   * route needs it). Rides the write chain; cached, so concurrent uploads
   * insert one row.
   */
  const ensureDraft = (): Promise<string> => {
    // A discarded (or filed) page never brings its row back.
    if (finalizedRef.current) return Promise.reject(new Error(`Draft discarded`))
    if (rowExistsRef.current) return Promise.resolve(draftId)
    if (!ensureDraftRef.current) {
      ensureDraftRef.current = enqueue(async () => {
        if (finalizedRef.current) throw new Error(`Draft discarded`)
        if (rowExistsRef.current) return
        const input = toUpsertInput(snapshotRef.current)
        await trpc.issueDrafts.upsert.mutate(input)
        rowExistsRef.current = true
        lastWrittenRef.current = JSON.stringify(input)
      })
        .then(() => draftId)
        .catch((error: unknown) => {
          // A failed ensure must not poison the page: the next upload retries.
          ensureDraftRef.current = null
          throw error
        })
    }
    return ensureDraftRef.current
  }

  const create = async (): Promise<CreatedIssue | null> => {
    if (creatingRef.current || finalizedRef.current) return null
    if (uploadCountRef.current > 0) return null
    if (!snapshotRef.current.title.trim()) return null
    createInFlightRef.current = true
    setCreating(true)
    await flush(`edit`)
    creatingRef.current = true
    const snapshot = snapshotRef.current
    const statusOption = statusRef.current
    let created: { identifier: string }
    let txId: number
    try {
      const result = await trpc.issues.create.mutate({
        boardId: snapshot.boardId,
        title: snapshot.title.trim(),
        // EXP-314: real rows write `statusId`; a constructed fallback row
        // writes the anchor enum instead.
        ...statusUpdatePayload(statusOption),
        priority: snapshot.priority,
        assigneeId: snapshot.assigneeId ?? undefined,
        // Already final `/api/attachments/{id}` URLs (eager uploads).
        description: toIssueDescription(snapshot.description) ?? undefined,
        dueDate: snapshot.dueDate ?? undefined,
        labelIds: snapshot.labelIds.length > 0 ? snapshot.labelIds : undefined,
        draftId: rowExistsRef.current ? draftId : undefined,
      })
      created = result.issue
      txId = result.txId
    } catch (error) {
      creatingRef.current = false
      createInFlightRef.current = false
      setCreating(false)
      toast.error(
        error instanceof Error ? error.message : `Failed to create issue`
      )
      return null
    }
    // The create consumed the draft row in its own transaction.
    finalizedRef.current = true
    createInFlightRef.current = false
    clearTimer()
    try {
      // Land on a row that is already synced, never a "not found" flash.
      await issueCollection.utils.awaitTxId(txId)
    } catch {
      // The issue exists; the detail route catches up on its own.
    }
    const board = boardsRef.current.find((row) => row.id === snapshot.boardId)
    return { identifier: created.identifier, boardSlug: board?.slug ?? null }
  }

  /** Throw the draft away: delete the row if there is one, write nothing on
   *  the way out. The caller navigates. EXP-1212 (R2): never while a Create
   *  is in flight — resolves `false` then, and the caller stays. */
  const discard = async (): Promise<boolean> => {
    if (createInFlightRef.current) return false
    if (finalizedRef.current) return true
    finalizedRef.current = true
    clearTimer()
    // An eager upload may still be creating the row: queue behind it.
    await writeChainRef.current
    if (!rowExistsRef.current) return true
    try {
      await trpc.issueDrafts.delete.mutate({ id: draftId })
      rowExistsRef.current = false
    } catch {
      toast.error(`Could not discard the draft`)
    }
    return true
  }

  return {
    title,
    description,
    boardId,
    status,
    priority,
    assigneeId,
    labelIds,
    dueDate,
    files,
    creating,
    uploading: uploadCount > 0,
    canCreate: canCreateDraft({
      title,
      creating,
      uploading: uploadCount > 0,
    }),
    // EXP-1212: what makes leaving ask. A reopened draft whose files are not
    // known yet counts as content, as it does for the leave write.
    hasContent:
      hasDraftContent(snapshotRef.current) || !filesLoadedRef.current,
    setTitle: (value: string) => {
      setTitleState(value)
      schedule()
    },
    setDescription: (value: string) => {
      setDescriptionState(value)
      schedule()
    },
    setBoardId: (value: string) => {
      setBoardIdState(value)
      bumpChips()
    },
    setStatus: (option: StatusRowOption) => {
      setStatusId(isFallbackStatusOption(option) ? null : option.id)
      bumpChips()
    },
    setPriority: (value: IssuePriority) => {
      setPriorityState(value)
      bumpChips()
    },
    setAssigneeId: (value: string | null) => {
      setAssigneeIdState(value)
      bumpChips()
    },
    toggleLabel: (labelId: string) => {
      setLabelIds((previous) =>
        previous.includes(labelId)
          ? previous.filter((id) => id !== labelId)
          : [...previous, labelId]
      )
      bumpChips()
    },
    setDueDate: (value: string | null) => {
      setDueDateState(value)
      bumpChips()
    },
    onTitleBlur: () => void flush(`edit`),
    onDescriptionBlur: () => void flush(`edit`),
    ensureDraft,
    /** The page reports each eager upload's start and end. */
    beginUpload: () => {
      uploadCountRef.current += 1
      setUploadCount(uploadCountRef.current)
    },
    endUpload: () => {
      uploadCountRef.current = Math.max(0, uploadCountRef.current - 1)
      setUploadCount(uploadCountRef.current)
    },
    /** An attachment already uploaded against the draft. */
    addFile: (file: FilesSectionFile) => {
      setFiles((previous) => [...previous, file])
      bumpChips()
    },
    /** Rejects with the message the Files section shows. */
    removeFile: async (file: FilesSectionFile) => {
      await trpc.attachments.delete.mutate({ id: file.id })
      setFiles((previous) => previous.filter((row) => row.id !== file.id))
    },
    /** EXP-1212 (R2): a Create is in flight (read live, not from a render). */
    isCreating: () => createInFlightRef.current,
    flush,
    leave,
    create,
    discard,
  }
}

export type IssueDraftEditor = ReturnType<typeof useIssueDraftEditor>
