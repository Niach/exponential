import { useCallback, useEffect, useRef, useState } from "react"
import {
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Files,
  Link2,
} from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { Link, useNavigate } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Issue, User, Board } from "@/db/schema"
import { BoardGlyph } from "@/components/board-glyph"
import { issueCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  formatDateForMutation,
  getIssueDescriptionText,
  normalizeIssueDescriptionText,
} from "@/lib/domain"
import {
  uploadIssueFile,
  uploadIssueImageFile,
} from "@/lib/storage/issue-image-upload"
import { useSession } from "@/hooks/use-session"
import { parseLocalDate } from "@/lib/utils"
import { useIsMobile } from "@/hooks/use-mobile"
import { Button } from "@/components/ui/button"
import { IconTooltip } from "@/components/icon-tooltip"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Textarea } from "@/components/ui/textarea"
import { Separator } from "@/components/ui/separator"
import type { IssueFilterSearch } from "@/lib/filters"
import { useDuplicateInterception } from "@/hooks/use-duplicate-interception"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { statusUpdatePayload } from "@/lib/team-statuses"
import {
  MarkdownEditor,
  type MarkdownEditorRef,
} from "@/components/issue-editor/markdown-editor"
import { IssuePropertiesPanel } from "@/components/issue-properties-panel"
import { IssueTimeline } from "@/components/issue-timeline"
import { IssueCodingControl, IssuePrRow } from "@/components/issue-coding-rows"
import { IssueDetailMobileBar } from "@/components/issue-detail-mobile-bar"
import { IssueFilesSection } from "@/components/issue-files-section"
import { SubscribeToggle } from "@/components/subscribe-toggle"
import { IssueDetailMobileMenu } from "@/components/issue-detail-mobile-menu"
import { WidgetSubmissionCard } from "@/components/widget-submission-card"

const UiMoreIcon = conceptIcon(`ui-more`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const UiUndoIcon = conceptIcon(`ui-undo`)

// Where the current issue sits in the board's filtered+sorted sequence — feeds
// the header's "N / total" prev/next switcher. Null (or omitted) hides the
// switcher, e.g. when the issue is filtered out of the carried board view.
export interface IssueSwitcherPosition {
  index: number
  total: number
  prevIdentifier: string | null
  nextIdentifier: string | null
}

interface IssueDetailViewProps {
  issue: Issue
  issueLabelIds: string[]
  users: User[]
  board: Board
  teamSlug: string
  teamId: string
  readOnly?: boolean
  // Board filter params carried from the list view — preserved on prev/next
  // navigation and on the breadcrumb's back-to-board link.
  filterSearch?: IssueFilterSearch
  position?: IssueSwitcherPosition | null
}

// Canonical-issue banner shown on a duplicate's detail view: "Duplicate of
// #IDENT — {title}", clickable through to the canonical issue, with an Unmark
// action (clears the link; the server restores status atomically).
function DuplicateOfBanner({
  duplicateOfId,
  onUnmark,
  readOnly,
}: {
  duplicateOfId: string
  onUnmark: () => void
  readOnly: boolean
}) {
  const issueRefs = useIssueRefs()
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.id, duplicateOfId)),
    [duplicateOfId]
  )
  const canonical = (data?.[0] ?? null) as Issue | null
  if (!canonical) return null

  return (
    <div className="flex items-center gap-2 border-b border-border bg-accent/30 px-4 py-2 text-sm min-w-0">
      <Files className="size-4 shrink-0 text-muted-foreground" />
      <span className="shrink-0 text-muted-foreground">Duplicate of</span>
      <Button
        variant="outline"
        size="xs"
        className="h-5 shrink-0 rounded-full px-2 font-mono text-xs"
        onClick={() => issueRefs?.open(canonical.identifier)}
      >
        #{canonical.identifier}
      </Button>
      <span className="truncate text-muted-foreground">{canonical.title}</span>
      {!readOnly && (
        <Button
          variant="ghost"
          size="xs"
          className="ml-auto shrink-0 text-muted-foreground"
          onClick={onUnmark}
        >
          <UiUndoIcon className="size-3.5" />
          Unmark
        </Button>
      )}
    </div>
  )
}

export function IssueDetailView({
  issue,
  issueLabelIds,
  users,
  board,
  teamSlug,
  teamId,
  readOnly = false,
  filterSearch,
  position = null,
}: IssueDetailViewProps) {
  const { data: session } = useSession()
  const currentUserId = session?.user?.id ?? null
  const isMobile = useIsMobile()
  const navigate = useNavigate()

  // In-place hop to a sibling issue in the board sequence, preserving the
  // carried filter params. Safe without unmount: the issue.id-keyed reset
  // effect below re-seeds all local editor state, and IssueTimeline is keyed
  // on issue.id so its composer draft resets too (REV-47).
  const navigateToIssue = (identifier: string | null) => {
    if (!identifier) return
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug: board.slug,
        issueIdentifier: identifier,
      },
      search: {
        status: filterSearch?.status,
        priority: filterSearch?.priority,
        labels: filterSearch?.labels,
      },
    })
  }

  const prevIdentifier = position?.prevIdentifier ?? null
  const nextIdentifier = position?.nextIdentifier ?? null

  // J/K prev-next shortcuts (Linear parity), scoped to this view's lifetime.
  // Ignored while typing (inputs / the TipTap contenteditable), while any
  // dialog is open, or while a popper overlay (dropdown/popover/select) is up.
  useEffect(() => {
    if (!position) return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return
      const key = event.key.toLowerCase()
      if (key !== `j` && key !== `k`) return
      const target = event.target as HTMLElement | null
      if (
        target &&
        (target.tagName === `INPUT` ||
          target.tagName === `TEXTAREA` ||
          target.isContentEditable ||
          target.closest(`[contenteditable="true"]`))
      ) {
        return
      }
      if (
        document.querySelector(
          `[role="dialog"][data-state="open"], [data-radix-popper-content-wrapper]`
        )
      ) {
        return
      }
      const identifier = key === `j` ? nextIdentifier : prevIdentifier
      if (!identifier) return
      event.preventDefault()
      navigateToIssue(identifier)
    }
    window.addEventListener(`keydown`, handleKeyDown)
    return () => window.removeEventListener(`keydown`, handleKeyDown)
  }, [
    Boolean(position),
    prevIdentifier,
    nextIdentifier,
    board.slug,
    teamSlug,
    filterSearch?.status,
    filterSearch?.priority,
    filterSearch?.labels,
  ])

  const editorRef = useRef<MarkdownEditorRef>(null)
  const descriptionRef = useRef(getIssueDescriptionText(issue.description))
  // Two baselines in two coordinate systems, both always normalized. The
  // editor re-serializes whatever it parses, and markdown authored on other
  // clients (native apps, MCP, the widget) need not round-trip
  // byte-identically through TipTap — mixing the spaces made one applied
  // non-canonical description look like unsaved local edits forever,
  // deferring every later remote update and letting a mere focus+blur save
  // stale re-serialized text over newer remote saves.
  // - lastSavedDescriptionRef: EDITOR-serialized text at the last
  //   apply/save/settle — compared against the editor's local text to detect
  //   unsaved edits.
  // - syncedDescriptionRef: RAW synced text this view has accounted for —
  //   compared against the incoming value to detect new remote content.
  const lastSavedDescriptionRef = useRef(
    normalizeIssueDescriptionText(getIssueDescriptionText(issue.description))
  )
  const syncedDescriptionRef = useRef(
    normalizeIssueDescriptionText(getIssueDescriptionText(issue.description))
  )
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve())
  const uploadQueueRef = useRef<Promise<void>>(Promise.resolve())

  const [title, setTitle] = useState(issue.title)
  const [description, setDescription] = useState(
    getIssueDescriptionText(issue.description)
  )
  const [attachmentStatus, setAttachmentStatus] = useState<string | null>(null)
  const [activeUploadCount, setActiveUploadCount] = useState(0)
  const [linkCopied, setLinkCopied] = useState(false)
  // EXP-568: the floating phone bar steps aside while the description is being
  // written — the keyboard formatting rail owns the bottom edge then.
  const [descriptionFocused, setDescriptionFocused] = useState(false)
  // The band's measured height feeds the editor's scroll-into-view insets so
  // the caret never hides under it; a ResizeObserver tracks it live (the
  // title wraps, the toolbar row wraps — the height is dynamic).
  const [stickyBandHeight, setStickyBandHeight] = useState(0)
  // Memoized so React attaches it exactly once. An inline callback is a new
  // identity every render, which makes React detach and re-attach the ref on
  // every commit — i.e. tear down and rebuild the observer on every keystroke
  // in the title or the description. The returned cleanup is React 19's ref
  // cleanup, so there is no `null` detach call to handle either.
  const setStickyBand = useCallback((node: HTMLDivElement) => {
    setStickyBandHeight(node.offsetHeight)
    const observer = new ResizeObserver(() => {
      setStickyBandHeight(node.offsetHeight)
    })
    observer.observe(node)
    return () => {
      observer.disconnect()
      setStickyBandHeight(0)
    }
  }, [])

  const { resolve: resolveStatus } = useTeamStatusesContext()
  const statusOption = resolveStatus(issue)

  const { handleStatusChange, duplicatePicker } = useDuplicateInterception({
    issueId: issue.id,
    onStatusChange: async (next) => {
      if (readOnly) return
      await trpc.issues.update.mutate({
        id: issue.id,
        ...statusUpdatePayload(next),
      })
    },
  })

  const incomingDescription = getIssueDescriptionText(issue.description)
  const normalizedIncoming = normalizeIssueDescriptionText(incomingDescription)

  // Destructive replace of the local editor content with a synced value —
  // setMarkdown resets the caret, so callers must ensure there are no unsaved
  // local edits worth keeping.
  const applyIncomingDescription = (nextDescription: string) => {
    setDescription(nextDescription)
    descriptionRef.current = nextDescription
    syncedDescriptionRef.current =
      normalizeIssueDescriptionText(nextDescription)
    editorRef.current?.setMarkdown(nextDescription)
    // Settle the local text and the unsaved-edits baseline from the editor's
    // OWN serialization of what it just parsed (setMarkdown also re-enters
    // onChange with it), never from the raw incoming string — the two need
    // not match byte-for-byte. While the editor instance does not exist yet
    // the raw value stands in for both, and the editor is then created from
    // that same value, so the pair stays consistent either way.
    const editorMarkdown = editorRef.current?.getMarkdown()
    if (editorMarkdown != null) {
      setDescription(editorMarkdown)
      descriptionRef.current = editorMarkdown
    }
    lastSavedDescriptionRef.current = normalizeIssueDescriptionText(
      descriptionRef.current
    )
  }

  // Full reset when navigating to a different issue.
  useEffect(() => {
    setTitle(issue.title)
    applyIncomingDescription(incomingDescription)
    setAttachmentStatus(null)
  }, [issue.id])

  // Opening an issue clears its inbox notifications (EXP-92) — the safety net
  // for push taps and email deep links that never pass through the inbox (whose
  // row click keeps its own eager markRead). Fire-and-forget: failure just
  // leaves the row unread.
  useEffect(() => {
    trpc.notifications.markReadByIssue
      .mutate({ issueId: issue.id })
      .catch(() => {})
  }, [issue.id])

  // Sync title from Electric when another client changes it,
  // but skip if the local value matches what we'd save (user is editing).
  useEffect(() => {
    if (issue.title !== title && issue.title !== title.trim()) {
      setTitle(issue.title)
    }
  }, [issue.title])

  // Sync description from Electric when another client changes it — without
  // clobbering typing in progress. An incoming value the editor already shows
  // (the Electric echo of a save can beat the tRPC response) only settles the
  // bookkeeping; with unsaved local edits the replace is deferred to the next
  // blur instead of wiping the user's text and resetting the caret.
  useEffect(() => {
    if (normalizedIncoming === syncedDescriptionRef.current) return
    const normalizedLocal = normalizeIssueDescriptionText(
      descriptionRef.current
    )
    if (normalizedIncoming === normalizedLocal) {
      syncedDescriptionRef.current = normalizedIncoming
      lastSavedDescriptionRef.current = normalizedLocal
      return
    }
    if (normalizedLocal !== lastSavedDescriptionRef.current) return
    applyIncomingDescription(incomingDescription)
  }, [normalizedIncoming])

  const handleTitleBlur = async () => {
    if (readOnly) return
    const trimmed = title.trim()
    if (trimmed && trimmed !== issue.title) {
      await trpc.issues.update.mutate({ id: issue.id, title: trimmed })
    }
  }

  const queueDescriptionSave = async (nextDescription: string) => {
    if (readOnly) return
    const normalizedDescription = normalizeIssueDescriptionText(nextDescription)
    if (normalizedDescription === lastSavedDescriptionRef.current) {
      await saveQueueRef.current
      return
    }
    const saveTask = async () => {
      const baselineAtSaveStart = lastSavedDescriptionRef.current
      await trpc.issues.update.mutate({
        id: issue.id,
        description: normalizedDescription ? normalizedDescription : null,
      })
      // A remote apply, an echo settle, or an issue switch may have moved the
      // baselines while the mutate was in flight — rewinding them to this
      // save would mark the newer editor content as unsaved local edits.
      if (lastSavedDescriptionRef.current === baselineAtSaveStart) {
        lastSavedDescriptionRef.current = normalizedDescription
        syncedDescriptionRef.current = normalizedDescription
      }
    }
    const queuedSave = saveQueueRef.current.then(saveTask, saveTask)
    saveQueueRef.current = queuedSave.catch(() => undefined)
    try {
      await queuedSave
      setAttachmentStatus(null)
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to save description`
      )
      throw error
    }
  }

  const handleDescriptionBlur = async () => {
    const hadLocalEdits =
      normalizeIssueDescriptionText(descriptionRef.current) !==
      lastSavedDescriptionRef.current
    try {
      await queueDescriptionSave(descriptionRef.current)
    } catch {
      return
    }
    // A remote change that arrived mid-edit was deferred by the sync effect;
    // when this blur had nothing of ours to write over it, show it now. After
    // a real save the Electric echo of our own write reconciles instead.
    if (!hadLocalEdits && normalizedIncoming !== syncedDescriptionRef.current) {
      applyIncomingDescription(incomingDescription)
    }
  }

  const setDescriptionValue = (nextDescription: string) => {
    descriptionRef.current = nextDescription
    setDescription(nextDescription)
  }

  const enqueueUploadTask = async (task: () => Promise<void>) => {
    setActiveUploadCount((c) => c + 1)
    const queuedTask = uploadQueueRef.current.then(task, task)
    uploadQueueRef.current = queuedTask.catch(() => undefined)
    try {
      await queuedTask
    } finally {
      setActiveUploadCount((c) => c - 1)
    }
  }

  const handleImageFiles = async (files: File[]) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          const { url } = await uploadIssueImageFile(issue.id, file)
          editorRef.current?.insertImage({ alt: file.name, src: url })
          const nextDescription =
            editorRef.current?.getMarkdown() ?? descriptionRef.current
          setDescriptionValue(nextDescription)
          await queueDescriptionSave(nextDescription)
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload image`
      )
    }
  }

  // Images picked via the Files section's attach button (EXP-316): they belong
  // in the description, appended at the bottom rather than at the caret.
  const handleAppendImageFiles = async (files: File[]) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          const { url } = await uploadIssueImageFile(issue.id, file)
          editorRef.current?.appendImage({ alt: file.name, src: url })
          const nextDescription =
            editorRef.current?.getMarkdown() ?? descriptionRef.current
          setDescriptionValue(nextDescription)
          await queueDescriptionSave(nextDescription)
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload image`
      )
    }
  }

  // Pasted/dropped files that are NOT inline-embeddable images (EXP-297): they
  // upload to the Files section instead of entering the markdown.
  const handleOtherFiles = async (files: File[]) => {
    setAttachmentStatus(null)
    try {
      await enqueueUploadTask(async () => {
        for (const file of files) {
          await uploadIssueFile(issue.id, file)
        }
      })
    } catch (error) {
      setAttachmentStatus(
        error instanceof Error ? error.message : `Failed to upload file`
      )
    }
  }

  // EXP-57: the server renumbers the issue in the target board, so both the
  // board slug AND the identifier change — await the issues txId, then hop to
  // the issue's new canonical URL. Shared by the properties picker and the
  // phone `…` menu (EXP-687).
  const handleBoardChange = async (boardId: string) => {
    if (readOnly) return
    const {
      txId,
      issue: moved,
      boardSlug,
    } = await trpc.issues.move.mutate({ id: issue.id, boardId })
    await issueCollection.utils.awaitTxId(txId)
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug,
        issueIdentifier: moved.identifier,
      },
    })
  }

  // Delete is a hard delete (issues.delete cleans up attachments server-side);
  // once it commits, land back on the board with the carried filters.
  const handleDeleteIssue = async () => {
    await trpc.issues.delete.mutate({ id: issue.id })
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug`,
      params: { teamSlug, boardSlug: board.slug },
      search: {
        status: filterSearch?.status,
        priority: filterSearch?.priority,
        labels: filterSearch?.labels,
      },
    })
  }

  const dueDate = issue.dueDate ? parseLocalDate(issue.dueDate) : undefined

  // Coding "coding now" row (EXP-184): a main-column row on desktop, one
  // circle in the floating bar on phones (EXP-568). The component owns the
  // repo/membership/relay gating and focuses the global dock rather than
  // mounting the live viewer inline. Since EXP-616 the IDLE start affordance
  // moved out of this row and into the properties card below.
  const codingControl =
    currentUserId && !isMobile ? (
      <IssueCodingControl
        issue={issue}
        board={board}
        teamId={teamId}
        currentUserId={currentUserId}
        users={users}
        variant="row"
      />
    ) : null

  const codingFab =
    currentUserId && isMobile ? (
      <IssueCodingControl
        issue={issue}
        board={board}
        teamId={teamId}
        currentUserId={currentUserId}
        users={users}
        variant="fab"
      />
    ) : null

  // EXP-616: "Start coding" is a capsule at the trailing end of the properties
  // card (desktop parity with the IDE) — it no longer gets a row of its own
  // below the description. Desktop only: on phones the floating bar's circle
  // still owns the start, and the properties card there is the same node.
  const codingStartButton =
    currentUserId && !isMobile ? (
      <IssueCodingControl
        issue={issue}
        board={board}
        teamId={teamId}
        currentUserId={currentUserId}
        users={users}
        variant="start"
      />
    ) : null

  const propsPanel = (
    <IssuePropertiesPanel
      status={statusOption}
      onStatusChange={handleStatusChange}
      priority={issue.priority}
      onPriorityChange={async (priority) => {
        if (readOnly) return
        await trpc.issues.update.mutate({ id: issue.id, priority })
      }}
      assigneeId={issue.assigneeId}
      onAssigneeChange={async (assigneeId) => {
        if (readOnly) return
        await trpc.issues.update.mutate({ id: issue.id, assigneeId })
      }}
      users={users}
      teamId={teamId}
      selectedLabelIds={issueLabelIds}
      onToggleLabel={async (labelId) => {
        if (readOnly) return
        if (issueLabelIds.includes(labelId)) {
          await trpc.issueLabels.remove.mutate({ issueId: issue.id, labelId })
          return
        }
        await trpc.issueLabels.add.mutate({ issueId: issue.id, labelId })
      }}
      dueDate={dueDate}
      onDueDateSelect={async (date) => {
        if (readOnly) return
        await trpc.issues.update.mutate({
          id: issue.id,
          dueDate: formatDateForMutation(date),
        })
      }}
      source={issue.source}
      boardColor={board.color}
      boardPrefix={board.prefix}
      boardIcon={board.icon}
      boardRepositoryId={board.repositoryId}
      boardId={issue.boardId}
      issueIdentifier={issue.identifier}
      onBoardChange={handleBoardChange}
      disabled={readOnly}
    />
  )

  // EXP-568: properties live at the TOP of the reading column on every
  // viewport, inside their own glass card — no sidebar, no border-to-border
  // band welded to the header.
  const propsBand = (
    <div className="mx-auto w-full max-w-3xl px-4 pt-3">
      <div className="flex items-center gap-1.5 rounded-xl border border-glass-stroke-card bg-popover/40">
        <div className="min-w-0 flex-1">{propsPanel}</div>
        {/* min-w-0, not shrink-0: the "waiting for the desktop" caption beside
            the capsule truncates rather than squeezing the property pills. */}
        {codingStartButton && (
          <div className="min-w-0 pr-3">{codingStartButton}</div>
        )}
      </div>
    </div>
  )

  // Header actions shared by the desktop breadcrumb and the compact phone
  // header below — one definition each, two arrangements.
  const switcherButtons = position ? (
    <>
      <IconTooltip label="Previous issue" shortcut="K">
        <Button
          variant="glass"
          size="icon-sm"
          aria-label="Previous issue (K)"
          disabled={!position.prevIdentifier}
          onClick={() => navigateToIssue(position.prevIdentifier)}
        >
          <ChevronUp className="size-4" />
        </Button>
      </IconTooltip>
      <IconTooltip label="Next issue" shortcut="J">
        <Button
          variant="glass"
          size="icon-sm"
          aria-label="Next issue (J)"
          disabled={!position.nextIdentifier}
          onClick={() => navigateToIssue(position.nextIdentifier)}
        >
          <ChevronDown className="size-4" />
        </Button>
      </IconTooltip>
    </>
  ) : null

  // The label follows the icon into its copied state, so the tooltip confirms
  // the copy rather than repeating the invitation to click.
  const copyLinkButton = (
    <IconTooltip label={linkCopied ? `Link copied` : `Copy link to issue`}>
      <Button
        variant="glass"
        size="icon-sm"
        aria-label="Copy link to issue"
        onClick={() => {
          if (typeof navigator === `undefined` || !navigator.clipboard) {
            return
          }
          const url = `${window.location.origin}/t/${teamSlug}/boards/${board.slug}/issues/${issue.identifier}`
          navigator.clipboard.writeText(url).then(
            () => {
              setLinkCopied(true)
              setTimeout(() => setLinkCopied(false), 1500)
            },
            () => {
              // Clipboard denied (permissions/insecure context) — no success state.
            }
          )
        }}
      >
        {linkCopied ? (
          <Check className="size-4 text-primary" />
        ) : (
          <Link2 className="size-4" />
        )}
      </Button>
    </IconTooltip>
  )

  // EXP-426: the only remaining "…" item is the conditional duplicate unmark
  // — delete moved out to its own always-visible icon.
  const unmarkDuplicateMenu =
    !readOnly && issue.duplicateOfId ? (
      <DropdownMenu>
        <IconTooltip label="More actions">
          <DropdownMenuTrigger asChild>
            <Button
              variant="glass"
              size="icon-sm"
              aria-label="Issue actions"
            >
              <UiMoreIcon />
            </Button>
          </DropdownMenuTrigger>
        </IconTooltip>
        <DropdownMenuContent align="end" className="w-[13rem]">
          <DropdownMenuItem
            onSelect={() => {
              void trpc.issues.update.mutate({
                id: issue.id,
                duplicateOfId: null,
              })
            }}
          >
            <UiUndoIcon className="size-4" />
            Unmark duplicate
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    ) : null

  // Destructive → confirm on a second click, matching the issue-row context
  // menu's delete pattern (EXP-59).
  const deleteMenu = !readOnly ? (
    <DropdownMenu>
      <IconTooltip label="Delete issue">
        <DropdownMenuTrigger asChild>
          <Button
            variant="glass"
            size="icon-sm"
            aria-label="Delete issue"
          >
            <UiDeleteIcon />
          </Button>
        </DropdownMenuTrigger>
      </IconTooltip>
      <DropdownMenuContent align="end" className="w-[14rem]">
        <DropdownMenuItem
          variant="destructive"
          onSelect={() => {
            void handleDeleteIssue()
          }}
        >
          <UiDeleteIcon className="size-4" />
          Confirm delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  ) : null

  const subscribeToggle = currentUserId ? (
    <SubscribeToggle issueId={issue.id} currentUserId={currentUserId} />
  ) : null

  const issueUrl = `${typeof window === `undefined` ? `` : window.location.origin}/t/${teamSlug}/boards/${board.slug}/issues/${issue.identifier}`

  // The phone header collapses copy-link / subscribe / unmark / delete into
  // ONE `…` (EXP-687), the way the iOS and Android toolbars already do.
  const mobileMenu = (
    <IssueDetailMobileMenu
      issueId={issue.id}
      issueTitle={title}
      issueUrl={issueUrl}
      teamId={teamId}
      boardId={issue.boardId}
      issueIdentifier={issue.identifier}
      duplicateOfId={issue.duplicateOfId ?? null}
      currentUserId={currentUserId}
      readOnly={readOnly}
      onDelete={handleDeleteIssue}
      onMoveBoard={handleBoardChange}
      onUnmarkDuplicate={() => {
        void trpc.issues.update.mutate({ id: issue.id, duplicateOfId: null })
      }}
    />
  )

  const breadcrumb = (
    <div className="flex items-center gap-1.5 text-xs text-muted-foreground px-4 py-2 border-b border-border min-w-0">
      <Link
        to="/t/$teamSlug/boards/$boardSlug"
        params={{ teamSlug, boardSlug: board.slug }}
        // Link back to the board WITH the carried filters, so the round trip
        // lands on the exact view the user navigated from.
        search={{
          status: filterSearch?.status,
          priority: filterSearch?.priority,
          labels: filterSearch?.labels,
        }}
        className="inline-flex min-w-0 shrink items-center gap-1.5 hover:text-foreground"
      >
        <BoardGlyph board={board} className="size-3.5" />
        <span className="truncate">{board.name}</span>
      </Link>
      <ChevronRight className="size-3 shrink-0 text-muted-foreground/50" />
      <span className="shrink-0 font-mono">{issue.identifier}</span>
      <ChevronRight className="size-3 shrink-0 text-muted-foreground/50" />
      <span className="truncate text-foreground">{title}</span>
      <div className="ml-auto flex items-center gap-1 shrink-0">
        {position && (
          <>
            <span className="hidden px-0.5 font-mono tabular-nums whitespace-nowrap sm:inline">
              {position.index} / {position.total}
            </span>
            {switcherButtons}
            <Separator orientation="vertical" className="mx-1 !h-3.5" />
          </>
        )}
        {copyLinkButton}
        {subscribeToggle}
        {unmarkDuplicateMenu}
        {deleteMenu}
      </div>
    </div>
  )

  // EXP-568 phone header: one line, no room for a board NAME or an "N / total"
  // counter — the board glyph stands in for the crumb, and the identifier +
  // title carry the rest.
  const mobileHeader = (
    <div className="flex items-center gap-1.5 border-b border-border px-3 py-2 text-xs text-muted-foreground min-w-0">
      <Link
        to="/t/$teamSlug/boards/$boardSlug"
        params={{ teamSlug, boardSlug: board.slug }}
        search={{
          status: filterSearch?.status,
          priority: filterSearch?.priority,
          labels: filterSearch?.labels,
        }}
        aria-label={board.name}
        className="inline-flex shrink-0 items-center hover:text-foreground"
      >
        <BoardGlyph board={board} className="size-4" />
      </Link>
      <ChevronRight className="size-3 shrink-0 text-muted-foreground/50" />
      <span className="shrink-0 font-mono">{issue.identifier}</span>
      <span className="truncate text-foreground">{title}</span>
      <div className="ml-auto flex shrink-0 items-center">
        {switcherButtons}
        {position && (
          <Separator orientation="vertical" className="mx-1 !h-3.5" />
        )}
        {mobileMenu}
      </div>
    </div>
  )

  const duplicateBanner = issue.duplicateOfId ? (
    <DuplicateOfBanner
      duplicateOfId={issue.duplicateOfId}
      readOnly={readOnly}
      onUnmark={() => {
        void trpc.issues.update.mutate({ id: issue.id, duplicateOfId: null })
      }}
    />
  ) : null

  // A wrapping textarea (field-sizing-content), not an Input — long titles
  // must wrap on narrow viewports instead of clipping (EXP-189). Enter
  // commits via blur; titles stay single-logical-line.
  const titleField = (
    <Textarea
      value={title}
      rows={1}
      onBlur={() => void handleTitleBlur()}
      onChange={(e) => setTitle(e.target.value.replace(/\n/g, ``))}
      onKeyDown={(e) => {
        if (e.key === `Enter`) {
          e.preventDefault()
          e.currentTarget.blur()
        }
      }}
      placeholder="Issue title"
      disabled={readOnly}
      // EXP-424: ProseMirror's image drag carries the image URL as
      // `text/plain`, which a textarea happily accepts — an image dragged
      // within the description would otherwise land as a URL in the title and
      // save on blur. Refuse every drop here; the editor keeps its own.
      onDragOver={(e) => {
        e.preventDefault()
        e.dataTransfer.dropEffect = `none`
      }}
      onDrop={(e) => e.preventDefault()}
      className="min-h-0 resize-none bg-transparent dark:bg-transparent border-none shadow-none !text-2xl font-semibold px-5 pt-4 pb-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
    />
  )

  const editor = (
    <div className="px-1">
      <MarkdownEditor
        ref={editorRef}
        markdown={description}
        editable={!readOnly}
        onChange={setDescriptionValue}
        onBlur={() => void handleDescriptionBlur()}
        placeholder="Add description..."
        onFocusChange={setDescriptionFocused}
        topScrollInset={isMobile ? undefined : stickyBandHeight}
        imageUpload={{
          enabled: !readOnly,
          uploading: activeUploadCount > 0,
          onFiles: handleImageFiles,
          onOtherFiles: handleOtherFiles,
        }}
      />
    </div>
  )

  // The attachments strip is gone (EXP-256) — the editor's rail/paste/drop and
  // the image node menu cover add/remove; only upload errors still need a
  // surface.
  const attachmentError = attachmentStatus ? (
    <p className="px-5 py-2 text-xs text-destructive">{attachmentStatus}</p>
  ) : null

  // EXP-297 Files rail: non-inline-image attachments straight from the synced
  // shape, plus the "Attach file" affordance for members.
  const filesSection = (
    <IssueFilesSection
      issueId={issue.id}
      readOnly={readOnly}
      onImageFiles={handleAppendImageFiles}
    />
  )

  // PR / pushed-branch link to the review-detail route (EXP-106) — stays in
  // the main column on every layout.
  const prRow = currentUserId ? (
    <IssuePrRow
      issue={issue}
      board={board}
      teamId={teamId}
      teamSlug={teamSlug}
      currentUserId={currentUserId}
    />
  ) : null

  // Keyed on issue.id: prev/next navigation swaps issues in place, and the
  // composer draft (and comment edit state) must not carry over — an unsent
  // reply typed on one issue would otherwise post to the next (REV-47).
  const timeline = currentUserId ? (
    <IssueTimeline
      key={issue.id}
      issue={issue}
      currentUserId={currentUserId}
      users={users}
      hideComposer={isMobile}
    />
  ) : null

  // Same mutation the timeline's own composer runs (EXP-568: on phones the
  // composer moved into the floating bar, which sits outside the timeline).
  const handleCommentSubmit = async (
    body: string,
    attachmentIds: string[]
  ) => {
    await trpc.comments.create.mutate({
      issueId: issue.id,
      body,
      attachmentIds,
    })
  }

  // EXP-42b: reporter/page/env metadata of widget-filed issues, members-only
  // (the server gates it; anonymous viewers never even fetch).
  const widgetCard = currentUserId ? (
    <WidgetSubmissionCard issueId={issue.id} source={issue.source} />
  ) : null

  if (isMobile) {
    return (
      <div className="flex flex-col h-full min-h-0">
        {mobileHeader}
        {duplicateBanner}
        {/* pb-24 clears the floating bar so the last comment stays readable. */}
        <div className="flex-1 overflow-y-auto pb-24">
          {propsBand}
          {titleField}
          {editor}
          {attachmentError}
          {filesSection}
          {prRow}
          {widgetCard}
          {timeline}
        </div>
        {currentUserId && (
          <IssueDetailMobileBar
            issueId={issue.id}
            users={users}
            propertiesNode={propsPanel}
            codingNode={codingFab}
            onSubmitComment={handleCommentSubmit}
            hidden={descriptionFocused}
          />
        )}
        {duplicatePicker}
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {breadcrumb}
      {duplicateBanner}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <div className="flex flex-1 min-w-0 flex-col overflow-hidden">
          <div className="flex-1 min-h-0 overflow-y-auto">
            <div
              ref={setStickyBand}
              className="sticky top-0 z-10 glass-chrome-top"
            >
              <div className="mx-auto max-w-3xl">{titleField}</div>
            </div>
            <div className="mx-auto max-w-3xl">
              {propsBand}
              {editor}
              {attachmentError}
              {filesSection}
              {codingControl}
              {prRow}
              {widgetCard}
              {timeline}
            </div>
          </div>
        </div>
      </div>
      {duplicatePicker}
    </div>
  )
}
