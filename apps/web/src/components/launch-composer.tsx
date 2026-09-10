import { useEffect, useRef, useState } from "react"
import { MAX_START_PROMPT } from "@exp/db-schema/domain"
import type { User } from "@/db/schema"
import { Composer, ComposerSubmit, ComposerTool } from "@/components/composer"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { ActionInputFields } from "@/components/launch-dialog/action-input-fields"
import { ActionPicker } from "@/components/launch-dialog/action-picker"
import { IssuePicker } from "@/components/launch-dialog/issue-picker"
import { LaunchOptionsLine } from "@/components/launch-dialog/launch-options-line"
import { SubjectChips } from "@/components/launch-dialog/subject-chips"
import { Pill } from "@/components/ui/pill"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import { BUILTIN_CREATE_ACTION_ID } from "@/lib/builtin-actions"
import { pickChatSuggestions } from "@/lib/chat-suggestions"
import { conceptIcon } from "@/lib/icons.generated"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { cn } from "@/lib/utils"

// EXP-825: the ONE launcher — the Agent page's composer card. The subject
// chips lead, an action's typed input fields follow, then the mention field
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
const ActionRunIcon = conceptIcon(`action-run`)
const EditorImageIcon = conceptIcon(`editor-image`)
const UiSubmitIcon = conceptIcon(`ui-submit`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiCloseIcon = conceptIcon(`ui-close`)

/** What the field asks for, per subject. */
export function composerPlaceholder(model: LaunchComposerModel): string {
  const { subject } = model
  if (subject === null) return `Ask the agent…`
  if (subject.kind === `action` && subject.id === BUILTIN_CREATE_ACTION_ID) {
    return `Describe the action — what it should do, and its name if you have one…`
  }
  return `Additional instructions (optional)…`
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

  const send = () => {
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
      {/* EXP-790: the chips only while there is nothing typed and no subject
          — once the field has text they would just be in the way. */}
      {showSuggestions && (
        <div className="flex flex-wrap items-center gap-1.5 px-1">
          {suggestions.map((suggestion) => (
            <Pill
              key={suggestion}
              size="sm"
              mode="action"
              onClick={() => fieldRef.current?.insertText(suggestion)}
            >
              {suggestion}
            </Pill>
          ))}
        </div>
      )}
      <Composer
        data-testid="agent-composer"
        leading={
          hasSubject ? (
            <SubjectChips
              issues={model.checkedIssues}
              pendingIssueCount={checkedCount - model.checkedIssues.length}
              action={actionSubject ? model.selectedAction : null}
              actionPending={actionSubject && model.selectedAction === null}
              onRemoveIssue={model.toggleIssue}
              onClearAction={model.clearAction}
              disabled={busy}
            />
          ) : undefined
        }
        strip={
          <>
            {actionSubject && subject && inputDefs.length > 0 && (
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
            )}
            {images.length > 0 && (
              <div className="flex flex-wrap gap-2 px-3 pt-3">
                {images.map((image) => (
                  <div key={image.url} className="relative">
                    <img
                      src={image.url}
                      alt=""
                      className="size-16 rounded-md border border-glass-stroke-card object-cover"
                    />
                    <button
                      type="button"
                      aria-label="Remove image"
                      disabled={busy}
                      onClick={() => model.removeImage(image.url)}
                      className="absolute -right-1.5 -top-1.5 rounded-full border border-glass-stroke-card bg-popover p-0.5 text-muted-foreground hover:text-foreground"
                    >
                      <UiCloseIcon className="size-3" />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </>
        }
        tools={
          <>
            <IssuePicker
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
                <ActionRunIcon />
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
              <EditorImageIcon />
            </ComposerTool>
          </>
        }
        submit={
          /* The round send glyph, LABELLED: the label is the contract's
             per-subject submit text, so the card reads what it will do. */
          <ComposerSubmit
            aria-label={submitLabel}
            title={submitLabel}
            data-testid="agent-composer-submit"
            disabled={blocked}
            onClick={send}
            className="w-auto gap-1 px-2 text-xs font-medium"
          >
            {busy ? (
              <UiLoadingIcon className="!size-5 animate-spin" />
            ) : (
              <UiSubmitIcon className="!size-5" />
            )}
            <span>{submitLabel}</span>
          </ComposerSubmit>
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
    </div>
  )
}
