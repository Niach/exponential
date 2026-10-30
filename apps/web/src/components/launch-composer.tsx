import { useRef, useState } from "react"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import type { User } from "@/db/schema"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import {
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import { ComposerPlusMenu } from "@/components/launch-dialog/composer-plus-menu"
import { LaunchHeadline } from "@/components/launch-dialog/launch-headline"
import { LaunchNotes } from "@/components/launch-dialog/launch-notes"
import { ComposerAgentRow } from "@exp/ui"
import {
  ComposerInlineTools,
  type ComposerInlineToolsHandle,
} from "@/components/composer-inline-tools"
import {
  AttachmentFileTile,
  AttachmentThumb,
  conceptIcon,
  Composer,
  ComposerFrame,
  ComposerSubmit,
  FEEDBACK_WORDS,
  Spinner,
  useDelayedFlag,
} from "@exp/ui"
import { contract } from "@exp/domain-contract"
import { HOME_PLACEHOLDER } from "@exp/domain-contract/home-greeting"
import type { LaunchComposerModel } from "@/hooks/use-launch-composer"
import type { SteerCommand } from "@/lib/steer-commands"
import { cn } from "@/lib/utils"
import { useSession } from "@/hooks/use-session"
import { useIssuesCodingReadiness } from "@/hooks/use-coding-readiness"
import {
  CodingReadinessOverlay,
  ReadinessCaption,
} from "@/components/coding-readiness-checklist"

// EXP-825: the ONE launcher — the composer card, rendered inline on the
// Agent page and inside the start-coding dialog (EXP-1019, `launch-dialog.tsx`)
// without a second implementation of any of it. The HEADLINE leads (a picked
// prompt's chip / "Implement <issues>" — the chips live there now, not in the
// card), then the mention field (`@` members, `#` issue refs, `:` emoji;
// Enter sends, Shift+Enter breaks the line) with its `/` menu (NEW-2: the
// team's PROMPTS only — no builtins before a session exists; a pick makes the
// prompt the subject), the pending-image strip, and a tool row whose ONE tool
// is the "+" menu (EXP-1249: Implement issue ›, Add file or image, the run's
// options and tools — `launch-dialog/composer-plus-menu.tsx`), the submit
// glyph carrying the per-subject label (Start session / Start coding / Start
// batch · N). S-3: over the card the AGENT ROW (composer-home.json
// `agentRow`, the home and the dialog: an agent picked = a message into its
// chat, the issue subject riding along);
// the "+" is the card's ONLY tool (no inline image/file/#/emoji, no options
// line: Device/Account/Model/Repository are "+" rows); under the card only
// the launch notes (no desktop, not ready, batch guards). All state is the hook's
// (`use-launch-composer.ts`); this file owns chrome, the caret and the file
// plumbing. Test ids are byte-identical with the native suites
// (`agent-composer*`).
//
// EXP-1293: Start is ONE `sessions.send`; while it uploads and sends, the
// submit goes PENDING (the spinner in its slot, "Starting", only after
// `pending.delayMs`), then the composer opens the new thread.
//
// Icons are CONCEPTS — this is a multi-client surface (`lib/icons.test.ts`).

const UiSubmitIcon = conceptIcon(`ui-submit`)

/** Where the composer sits: `home` = the Agent page column
 *  (composer-home.json: "What do you want to build?"), `dialog` = the
 *  launch dialog, which keeps the contract's "Ask the agent…". */
export type LaunchComposerVariant = `home` | `dialog`

/** What the field asks for, per subject. EXP-1019: the two standing
 *  placeholders are the contract's (`composerUi`, ×4) — with a subject up in
 *  the headline, the field is the SECONDARY half and says so. EXP-1253: the
 *  home column asks composer-home.json's question instead of the chat one. */
export function composerPlaceholder(
  model: LaunchComposerModel,
  variant: LaunchComposerVariant = `dialog`
): string {
  const { subject } = model
  // S-3: a message to the agent row's pick keeps the home's question.
  if (variant === `home` && model.recipient) return HOME_PLACEHOLDER
  if (subject === null)
    return variant === `home` ? HOME_PLACEHOLDER : contract.composerUi.chatPlaceholder
  return contract.composerUi.instructionsPlaceholder
}

export function LaunchComposer({
  model,
  users,
  variant = `dialog`,
  className,
}: {
  model: LaunchComposerModel
  /** The team roster, for the field's `@` autocomplete. */
  users: User[]
  variant?: LaunchComposerVariant
  className?: string
}) {
  const { subject, text, images, busy, blocked } = model
  const pending = busy
  const pendingShown = useDelayedFlag(pending)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  const toolsRef = useRef<ComposerInlineToolsHandle>(null)
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
  // A message to an agent runs on the agent's own device: no readiness.
  const checkedIds = subject?.kind === `issues` && !model.recipient ? subject.ids : []
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

  // NEW-2: the `/` menu = the team's prompts (no builtins: there is no
  // session yet). A pick makes the prompt the subject and clears the
  // `/name` it was typed as.
  const menu = useSlashCommandMenu({
    text,
    commands: NO_COMMANDS,
    // S-3: a prompt pick moves the agent row to Temporary session.
    prompts: model.prompts,
    onAccept: model.setText,
    onPickPrompt: (prompt) => {
      model.pickPrompt(prompt.id)
      model.setText(``)
    },
  })

  const submitLabel = pendingShown ? FEEDBACK_WORDS.pending.start : model.submitLabel

  return (
    <ComposerFrame
      className={cn(`gap-2`, className)}
      // S-3: a message to an agent runs on the agent's own device and
      // login: no launch notes.
      options={model.recipient ? undefined : <LaunchNotes model={model} />}
    >
      {/* EXP-1019: the SUBJECT leads — a prompt / "Implement <issues>" —
          and the field under it is the optional half. The chips are the
          ones the card used to carry in its leading row, ✕ and all. */}
      <LaunchHeadline model={model} />
      {/* S-3: the agent row stands directly over the card (×4: under the
          headline), on the home AND in the Start coding dialog (#EXP-1451). */}
      {model.agentRow && (
        <ComposerAgentRow
          row={model.agentRow}
          onPick={model.pickRecipient}
          disabled={busy}
          className="mb-1"
        />
      )}
      <Composer
        data-testid="agent-composer"
        strip={
          <>
            {images.length > 0 && (
              <div className="flex flex-wrap gap-2 px-3 pt-3">
                {images.map((image) =>
                  image.kind === `image` ? (
                    <AttachmentThumb
                      key={image.url}
                      src={image.url}
                      removeLabel="Remove image"
                      onRemove={() => model.removeImage(image.url)}
                      disabled={busy}
                    />
                  ) : (
                    <AttachmentFileTile
                      key={image.url}
                      name={image.uploadedName ?? image.file.name}
                      removeLabel={`Remove ${image.file.name}`}
                      onRemove={() => model.removeImage(image.url)}
                      disabled={busy}
                    />
                  )
                )}
              </div>
            )}
          </>
        }
        tools={
          <>
            <ComposerPlusMenu
              model={model}
              disabled={busy}
              onAttach={() => toolsRef.current?.attach()}
            />
            <ComposerInlineTools ref={toolsRef} onFiles={addFiles} />
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
              data-pending={pendingShown || undefined}
              aria-busy={pending || undefined}
              disabled={blocked}
              onClick={send}
            >
              {pendingShown ? (
                <Spinner
                  size={FEEDBACK_STATES_PENDING_SPINNER}
                  label={FEEDBACK_WORDS.pending.start}
                  className="text-current"
                />
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
        <div className="relative">
          <MentionTextarea
            ref={fieldRef}
            id="agent-composer-field"
            data-testid="agent-composer-field"
            value={text}
            onValueChange={model.setText}
            users={users}
            onKeyDown={(event) => {
              // An IME's own Enter is never a send.
              if (event.nativeEvent.isComposing) return
              // The `/` menu gets first refusal: Enter/Tab pick a prompt and
              // never send the half-typed `/name`.
              if (menu.handleKeyDown(event)) return
              // Shift+Enter breaks the line.
              if (event.key !== `Enter` || event.shiftKey) return
              event.preventDefault()
              send()
            }}
            onPaste={(e) => {
              if (e.clipboardData.files.length === 0) return
              e.preventDefault()
              addFiles(Array.from(e.clipboardData.files))
            }}
            placeholder={composerPlaceholder(model, variant)}
            className="min-h-20 resize-none border-0 bg-transparent px-3 pt-3 shadow-none focus-visible:ring-0"
            // Client parity with the server's cap, so a long paste is refused
            // at the field instead of at submit.
            maxLength={SESSIONS_SEND_TEXT_MAX}
          />
          {menu.open && (
            <SlashCommandMenu
              candidates={menu.candidates}
              active={menu.active}
              onSelect={menu.accept}
              onHover={menu.setActive}
            />
          )}
        </div>
      </Composer>
    </ComposerFrame>
  )
}

/** The launch composer's `/` menu offers no builtins (no session yet). */
const NO_COMMANDS: readonly SteerCommand[] = []

/** The pending submit's spinner: the round 24px glyph's slot, so a bit
 *  larger than a text button's `pending.spinner`. */
const FEEDBACK_STATES_PENDING_SPINNER = 18
