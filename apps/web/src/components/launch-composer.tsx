import { useEffect, useRef, useState } from "react"
import { MAX_START_PROMPT } from "@exp/db-schema/domain"
import type { User } from "@/db/schema"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { BlockedStartDialog } from "@/components/blocked-start-dialog"
import { ActionInputFields } from "@/components/launch-dialog/action-input-fields"
import { FixConflictsCard } from "@/components/launch-dialog/fix-conflicts-card"
import { ActionPicker } from "@/components/launch-dialog/action-picker"
import { IssuePicker } from "@/components/launch-dialog/issue-picker"
import { LaunchHeadline } from "@/components/launch-dialog/launch-headline"
import { LaunchOptionsLine } from "@/components/launch-dialog/launch-options-line"
import { AttachmentThumb, Pill, conceptIcon, Composer, ComposerSubmit, ComposerTool } from "@exp/ui"
import { contract } from "@exp/domain-contract"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import { pickChatSuggestions } from "@/lib/chat-suggestions"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { cn } from "@/lib/utils"
import { useSession } from "@/hooks/use-session"
import { useIssuesCodingReadiness } from "@/hooks/use-coding-readiness"
import {
  CodingReadinessOverlay,
  ReadinessCaption,
} from "@/components/coding-readiness-checklist"

// EXP-825: the ONE launcher — the composer card, rendered inline on the
// Agent page and inside the start-coding dialog (EXP-1019, `launch-dialog.tsx`)
// without a second implementation of any of it. The HEADLINE leads ("Run
// <action>" / "Implement <issues>" — the chips live there now, not in the
// card), an action's typed input fields follow, then the mention field
// (`@` members, `#` issue refs, `:` emoji; Enter sends, Shift+Enter breaks
// the line), the pending-image strip, and a tool row with `#` (issue
// picker), ▶ (action picker) and the image button, the submit glyph carrying
// the contract label (Start chat / Start coding / Start batch · N / Run
// action). All state is the hook's (`use-launch-composer.ts`); this file owns
// chrome, the caret and the file plumbing. Test ids are byte-identical with
// the native suites (`agent-composer*`).
//
// Icons are CONCEPTS — this is a multi-client surface (`lib/icons.test.ts`).

const IssueRefIcon = conceptIcon(`editor-issue-ref`)
const NavActionsIcon = conceptIcon(`nav-actions`)
// EXP-850 §13: the STEER composers (this one and the session composer)
// attach with the `ui-add` plus ×4; comment and description editors keep
// `editor-image`.
const UiAddIcon = conceptIcon(`ui-add`)
const UiSubmitIcon = conceptIcon(`ui-submit`)
const UiLoadingIcon = conceptIcon(`ui-loading`)

/** EXP-827: a suggestion carrying a `#` placeholder lands the caret right
 *  behind that `#` (wherever it sits), so the issue-ref menu opens at once;
 *  a complete prompt lands it at the end. */
export function suggestionCaretOffset(text: string): number | undefined {
  const hash = text.indexOf(`#`)
  return hash >= 0 ? hash + 1 : undefined
}

function insertSuggestion(
  field: MentionTextareaHandle | null,
  suggestion: string
) {
  field?.insertText(suggestion, suggestionCaretOffset(suggestion))
}

/** What the field asks for, per subject. EXP-1019: the two standing
 *  placeholders are the contract's (`composerUi`, ×4) — with a subject up in
 *  the headline, the field is the SECONDARY half and says so. */
export function composerPlaceholder(model: LaunchComposerModel): string {
  const { subject, selectedAction } = model
  if (subject === null) return contract.composerUi.chatPlaceholder
  // EXP-825: an action can say what the requester should type here (its
  // `promptPlaceholder`, seeded from the retired free-text input); the
  // Create-action builtin carries its own.
  const hint = selectedAction?.promptPlaceholder?.trim()
  if (subject.kind === `action` && hint) return hint
  return contract.composerUi.instructionsPlaceholder
}

export function LaunchComposer({
  model,
  users,
  className,
}: {
  model: LaunchComposerModel
  /** The team roster, for the field's `@` autocomplete. */
  users: User[]
  className?: string
}) {
  const { subject, text, images, busy, blocked } = model
  // EXP-820: a few chips drawn from the pool per mount — the same range the
  // getting-started cards show, not three fixed verbs.
  const [suggestions] = useState(() => pickChatSuggestions())
  const fieldRef = useRef<MentionTextareaHandle>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  // A file chooser steals focus without moving it anywhere in the document —
  // latched on the click that opens one, released when it resolves either
  // way (the comment composer's pattern).
  const filePickerOpenRef = useRef(false)
  useEffect(() => {
    const release = () => {
      filePickerOpenRef.current = false
    }
    window.addEventListener(`focus`, release)
    return () => window.removeEventListener(`focus`, release)
  }, [])

  const addFiles = (files: File[]) => {
    if (files.length === 0) return
    // EXP-698: an attached image also drops its POSITIONAL reference at the
    // caret, so "crop [Image #2]" names one of several embeds.
    const caret = fieldRef.current?.caret() ?? text.length
    const next = model.addFiles(files, caret)
    fieldRef.current?.setCaret(next)
  }

  // EXP-1121: an issue subject that cannot start RIGHT NOW (no device
  // online, the board lost its repository…) says the missing step on the
  // submit instead of failing on send; pressing it opens the "Ready to
  // code?" checklist with the fix.
  const { data: session } = useSession()
  const checkedIds = subject?.kind === `issues` ? subject.ids : []
  const readinessState = useIssuesCodingReadiness({
    teamId: model.teamId,
    issueIds: checkedIds,
    known: model.checkedIssues,
    currentUserId: session?.user?.id ?? ``,
  })
  const { readiness } = readinessState
  const notReady =
    checkedIds.length > 0 &&
    readiness.visible &&
    !readiness.loading &&
    !readiness.ready
  const [readinessOpen, setReadinessOpen] = useState(false)

  const send = () => {
    if (notReady && !busy) {
      setReadinessOpen(true)
      return
    }
    if (blocked) return
    void model.submit()
  }

  const checkedCount = subject?.kind === `issues` ? subject.ids.length : 0
  const actionSubject = subject?.kind === `action`
  const hasSubject = subject !== null
  const inputDefs = model.selectedAction?.inputs ?? []
  const showSuggestions = !hasSubject && text.length === 0
  const submitLabel = model.submitLabel

  return (
    <div className={cn(`flex flex-col gap-2`, className)}>
      {/* EXP-1019: the SUBJECT leads — "Run <action>" / "Implement <issues>"
          — and the field under it is the optional half. The chips are the
          ones the card used to carry in its leading row, ✕ and all. */}
      <LaunchHeadline model={model} />
      {/* EXP-790: the chips only while there is nothing typed and no subject
          — once the field has text they would just be in the way. */}
      {showSuggestions && (
        <div className="flex flex-wrap items-center gap-1.5 px-1">
          {suggestions.map((suggestion) => (
            <Pill
              key={suggestion}
              size="sm"
              mode="action"
              className="max-w-full"
              onClick={() => insertSuggestion(fieldRef.current, suggestion)}
            >
              <span className="truncate">{suggestion}</span>
            </Pill>
          ))}
        </div>
      )}
      <Composer
        data-testid="agent-composer"
        strip={
          <>
            {actionSubject && subject && model.fixConflicts ? (
              /* EXP-1233: the Fix merge conflicts builtin wears its own
                 card — the PR's branch row (the picker) and, after a refused
                 merge, the reason. */
              <div className="px-3 pt-3">
                <FixConflictsCard
                  view={model.fixConflicts}
                  teamId={model.teamId}
                  value={subject.inputs.pr ?? ``}
                  seedIssueId={model.seedPrIssueId}
                  onChange={(issueId) => model.setInput(`pr`, issueId)}
                  disabled={busy}
                />
              </div>
            ) : actionSubject && subject && inputDefs.length > 0 ? (
              /* The action's typed picks (repo / board / pr / icon) — the
                 free-text kinds are gone (EXP-825): what the requester types
                 IS the run's instructions. */
              <div className="px-3 pt-3">
                <ActionInputFields
                  defs={inputDefs}
                  values={subject.inputs}
                  onChange={model.setInput}
                  repos={model.repos ?? []}
                  teamId={model.teamId}
                  seedPrIssueId={model.seedPrIssueId}
                />
              </div>
            ) : null}
            {images.length > 0 && (
              <div className="flex flex-wrap gap-2 px-3 pt-3">
                {images.map((image) => (
                  <AttachmentThumb
                    key={image.url}
                    src={image.url}
                    removeLabel="Remove image"
                    onRemove={() => model.removeImage(image.url)}
                    disabled={busy}
                  />
                ))}
              </div>
            )}
          </>
        }
        tools={
          <>
            <IssuePicker
              teamId={model.teamId}
              eligible={model.eligibleIssues}
              checked={model.checkedIssues}
              onToggle={model.toggleIssue}
              disabled={busy}
            >
              <ComposerTool
                aria-label="Pick issues"
                title="Issues"
                data-testid="agent-composer-issues-button"
                className={checkedCount > 0 ? `text-foreground` : undefined}
              >
                <IssueRefIcon />
              </ComposerTool>
            </IssuePicker>
            <ActionPicker
              actions={model.actions}
              selectedActionId={actionSubject && subject ? subject.id : null}
              onSelect={model.pickAction}
              disabled={busy}
            >
              <ComposerTool
                aria-label="Pick an action"
                title="Actions"
                data-testid="agent-composer-actions-button"
                className={actionSubject ? `text-foreground` : undefined}
              >
                <NavActionsIcon />
              </ComposerTool>
            </ActionPicker>
            <input
              ref={fileInputRef}
              type="file"
              multiple
              accept={acceptedImageContentTypes.join(`,`)}
              className="hidden"
              onChange={(e) => {
                filePickerOpenRef.current = false
                if (e.target.files) addFiles(Array.from(e.target.files))
                e.target.value = ``
              }}
            />
            <ComposerTool
              aria-label="Attach image"
              title="Attach image"
              data-testid="agent-composer-image-button"
              disabled={busy}
              onClick={() => {
                filePickerOpenRef.current = true
                fileInputRef.current?.click()
              }}
            >
              <UiAddIcon />
            </ComposerTool>
          </>
        }
        submit={
          notReady ? (
            /* EXP-1121: the missing step beside a greyed submit; a press
               opens the checklist rather than sending. */
            <CodingReadinessOverlay
              state={readinessState}
              open={readinessOpen}
              onOpenChange={setReadinessOpen}
              // Ready, the checklist closes and the composer sends as usual.
              onStart={() => {
                if (!blocked) void model.submit()
              }}
            >
              {readiness.caption && (
                <ReadinessCaption
                  caption={readiness.caption}
                  onClick={() => setReadinessOpen((value) => !value)}
                  className="mr-1"
                />
              )}
              <ComposerSubmit
                aria-label={readiness.caption ?? submitLabel}
                title={readiness.caption ?? submitLabel}
                data-testid="agent-composer-submit"
                data-readiness="missing"
                // Disabled-LOOKING (still a target): the circled glyph
                // greys out; the amber caption beside it says why.
                className="text-muted-foreground hover:text-foreground"
                onClick={() => setReadinessOpen((value) => !value)}
              >
                <UiSubmitIcon className="!size-6" />
              </ComposerSubmit>
            </CodingReadinessOverlay>
          ) : (
            /* The round send glyph. EXP-827: icon-only — the chips already
               say what the send starts; the contract's per-subject label
               stays the button's name (aria-label + tooltip). */
            <ComposerSubmit
              aria-label={submitLabel}
              title={submitLabel}
              data-testid="agent-composer-submit"
              disabled={blocked}
              onClick={send}
            >
              {busy ? (
                <UiLoadingIcon className="!size-6 animate-spin" />
              ) : (
                <UiSubmitIcon className="!size-6" />
              )}
            </ComposerSubmit>
          )
        }
        onDrop={(event) => {
          if (event.dataTransfer.files.length === 0) return
          event.preventDefault()
          addFiles(Array.from(event.dataTransfer.files))
        }}
        onDragOver={(event) => {
          if (event.dataTransfer.types.includes(`Files`)) event.preventDefault()
        }}
      >
        <MentionTextarea
          ref={fieldRef}
          id="agent-composer-field"
          data-testid="agent-composer-field"
          value={text}
          onValueChange={model.setText}
          users={users}
          onKeyDown={(event) => {
            // Shift+Enter breaks the line; an IME's own Enter is never a send.
            if (event.key !== `Enter` || event.shiftKey) return
            if (event.nativeEvent.isComposing) return
            event.preventDefault()
            send()
          }}
          onPaste={(e) => {
            if (e.clipboardData.files.length === 0) return
            e.preventDefault()
            addFiles(Array.from(e.clipboardData.files))
          }}
          placeholder={composerPlaceholder(model)}
          className="min-h-20 resize-none border-0 bg-transparent px-3 pt-3 shadow-none focus-visible:ring-0"
          // Client parity with the server's cap, so a long paste is refused
          // at the field instead of at submit.
          maxLength={MAX_START_PROMPT}
        />
      </Composer>
      <LaunchOptionsLine model={model} />
      {/* EXP-980: the submit on BLOCKED issues asks first (SLOP-3: Cancel ·
          Start anyway · Stacked PR). */}
      <BlockedStartDialog
        open={model.blockedOpen}
        teamId={model.teamId}
        pickedIds={model.checkedIssues.map((issue) => issue.id)}
        blockers={model.blockedStart}
        busy={busy}
        onOpenChange={(next) => {
          if (!next) model.closeBlockedStart()
        }}
        stackReason={model.blockedStack.reason}
        stackIdent={model.blockedStack.ident}
        stackRun={model.blockedStack.plan?.run ?? []}
        onStartAnyway={() => void model.startAnyway()}
        onStartStacked={() => void model.startStacked()}
      />
    </div>
  )
}
