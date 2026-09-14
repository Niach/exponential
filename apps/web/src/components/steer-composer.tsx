import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react"
import { toast } from "sonner"
import { X } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import type { User } from "@/db/schema"
import {
  planModeChipLabel,
  sessionModel,
  type SessionConfigState,
} from "@/lib/agent-feed"
import { agentModelValues } from "@/lib/coding-launch-prefs"
import { InlinePicker } from "@/components/launch-dialog/launch-options-line"
import { modelLabel } from "@/components/launch-dialog/launch-options-pane"
import {
  mergeAgentCommands,
  parseSteerCommand,
  steerAgentId,
  steerCommandConfirmCopy,
  steerCommandsFor,
  type SteerCommand,
} from "@/lib/steer-commands"
import {
  SlashCommandMenu,
  useSlashCommandMenu,
} from "@/components/steer-command-menu"
import type { SteerSessionStore } from "@/lib/steer-session-store"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { uploadSessionImageFile } from "@/lib/storage/issue-image-upload"
import {
  buildSteerImageMessage,
  insertImageMarker,
  MAX_STEER_IMAGES,
  renumberImageMarkers,
} from "@/lib/steer-image-message"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  Composer,
  ComposerSubmit,
  ComposerTool,
} from "@/components/composer"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

// EXP-698/EXP-724/EXP-790: the STEERING COMPOSER — the field a live run is
// driven from, lifted out of `agent-session.tsx` by EXP-877 so the session
// view and the transcript rows stop sharing one file. Behaviour and wire
// format are unchanged; what EXP-877 added is the FOOTER under the card:
// the plan-mode word, the attach `+` (out of the card's tool row), the model
// picker and the caller's usage slot — the composer itself stays a field and
// a send glyph.

const UiAddIcon = conceptIcon(`ui-add`)

/** EXP-877: one placeholder on every run — the `/` menu is always there. */
const COMPOSER_PLACEHOLDER = `Type / for commands`
/** The footer's plan-mode word, shown only while plan mode is in force. */
const PLAN_MODE_LABEL = `Plan mode`

export interface SteerComposerProps {
  store: SteerSessionStore
  /** Sending is possible — the composer itself stays mounted regardless
   *  (EXP-621), so a connection flap never eats the draft. */
  live: boolean
  onSend: (text: string) => boolean
  /** EXP-790: the agent is busy — with nothing typed, the send glyph is Stop
   *  and interrupts the turn. */
  working: boolean
  /** Every steer image uploads to the session's own server-only store
   *  (EXP-702) — issue runs included, so steering screenshots never clutter
   *  the issue's Files section. */
  sessionId: string
  /** EXP-724: the session's coding agent (synced row), which decides which
   *  slash commands the `/` menu offers. Null = a claude run. */
  agent: string | null
  /** EXP-746: the agent's live configuration — the chips in the tool row and
   *  the agent's own half of the `/` catalog. Null on a PTY run (and until
   *  the first `config_state` lands), which draws no chips at all. */
  config: SessionConfigState | null
  /** EXP-698: the run's team, for the field's `@` autocomplete — `#` issue
   *  refs and `:` emoji work without it. */
  users: User[]
  /** EXP-877: the footer's trailing slot — the caller mounts the context ring
   *  inside its own usage popover here (`components/context-ring.tsx`). */
  usageSlot?: ReactNode
}

export function SteerComposer({
  store,
  live,
  onSend,
  working,
  sessionId,
  agent,
  config,
  users,
  usageSlot,
}: SteerComposerProps) {
  // EXP-621: the draft lives in the per-session store, so it survives
  // reconnects, dock collapse/reopen and navigation. Blob URLs are the
  // store's to revoke — no unmount cleanup here.
  const { text, images: pendingImages } = useSyncExternalStore(
    store.subscribe,
    store.getDraftSnapshot
  )
  const [sending, setSending] = useState(false)
  /** EXP-724: a context-discarding command waiting on its confirmation. */
  const [confirming, setConfirming] = useState<SteerCommand | null>(null)
  // EXP-746: the contract catalog for this agent, then the agent's OWN
  // advertised commands (an ACP run publishes them in `config_state`). An
  // agent-less run that published one is EXTERNAL: no contract rows at all,
  // only what it advertised itself (`steerAgentId`).
  const catalogAgent = steerAgentId(agent, config !== null)
  const commands = useMemo(
    () =>
      mergeAgentCommands(
        steerCommandsFor(catalogAgent),
        config?.commands ?? [],
        catalogAgent
      ),
    [catalogAgent, config?.commands]
  )
  // EXP-877: the footer's three reads — is the run parked in plan mode, what
  // model is in force, and what it can be switched to. A run that publishes
  // no model option draws no picker at all.
  const planMode = planModeChipLabel(config) !== null
  const codex = (agent ?? ``).trim().toLowerCase() === `codex`
  const model = sessionModel(config)
  const modelOptions = useMemo(() => {
    const values = agentModelValues(`claude`)
    const options = values.map((value) => ({
      value,
      label: modelLabel(value),
    }))
    // A model this build has no contract value for still reads as itself
    // rather than collapsing the picker to its label.
    return model !== null && !values.includes(model)
      ? [{ value: model, label: modelLabel(model) }, ...options]
      : options
  }, [model])
  const menu = useSlashCommandMenu({
    text,
    commands,
    onAccept: (next) => store.setDraftText(next),
  })
  /** EXP-790: Stop shows while the agent is working and the field is empty
   *  (no text, no image) — the moment something is typed, it is Send again. */
  const stop = working && !text.trim() && pendingImages.length === 0
  const fileInputRef = useRef<HTMLInputElement>(null)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  // A file chooser steals focus without moving it anywhere in the document
  // AND can stall the tab long enough for the relay to evict the viewer —
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
    // EXP-698: an attached image also drops its POSITIONAL reference at the
    // caret, so "crop [Image #2]" names one of several embeds. The strip
    // length before the add IS the numbering base.
    const base = pendingImages.length
    const { rejected, overflow, added } = store.addDraftImages(files)
    if (added > 0) {
      let next = text
      let caret = fieldRef.current?.caret() ?? text.length
      for (let i = 0; i < added; i++) {
        const inserted = insertImageMarker(next, caret, base + i + 1)
        next = inserted.text
        caret = inserted.caret
      }
      store.setDraftText(next)
      fieldRef.current?.setCaret(caret)
    }
    if (rejected > 0) {
      toast.error(`Only images up to 10 MB can be attached`)
    }
    if (overflow > 0) {
      toast.error(`Up to ${MAX_STEER_IMAGES} images per message`)
    }
  }

  /** Dropping a pending image takes its markers with it and slides the
   *  higher ones down, so the numbers keep matching the strip. */
  const removeImage = (url: string) => {
    const index = pendingImages.findIndex((image) => image.url === url)
    if (index >= 0) store.setDraftText(renumberImageMarkers(text, index + 1))
    store.removeDraftImage(url)
  }

  const send = async (confirmed = false) => {
    if (sending || !live) return
    if (!text.trim() && pendingImages.length === 0) return
    // EXP-724: a slash command is the WHOLE message. It rides the ordinary
    // input frames (the desktop recognizes it by its first token), so the only
    // client-side rules are: no image payload to wrap it in, and a
    // context-discarding command asks first.
    const command = parseSteerCommand(text, commands)
    if (command) {
      if (pendingImages.length > 0) {
        toast.error(`Remove the images to send a command`)
        return
      }
      if (command.command.confirm && !confirmed) {
        setConfirming(command.command)
        return
      }
    }
    if (pendingImages.length === 0) {
      if (onSend(text)) store.clearDraftAfterSend()
      return
    }
    setSending(true)
    try {
      // Upload sequentially, persisting each id as it lands — a mid-batch
      // failure keeps the composer intact and a retry only uploads the rest.
      const ids: string[] = []
      for (const image of pendingImages) {
        let uploadedId = image.uploadedId
        if (!uploadedId) {
          const uploaded = await uploadSessionImageFile(sessionId, image.file)
          uploadedId = uploaded.id
          store.setDraftImageUploaded(image.url, uploadedId)
        }
        ids.push(uploadedId)
      }
      if (!onSend(buildSteerImageMessage(text, ids))) {
        toast.error(`The session is no longer connected`)
        return
      }
      store.clearDraftAfterSend()
    } catch (error) {
      toast.error(`Couldn't upload image`, {
        description: error instanceof Error ? error.message : undefined,
      })
    } finally {
      setSending(false)
    }
  }

  // EXP-696/EXP-698: ONE rounded card laid out as a COLUMN — the pending
  // strip, a borderless full-width field, then the `[+]`·spacer·send row (the
  // natives' composerCard), now the shared `Composer`. Behavior and wire
  // format are unchanged.
  return (
    <>
      <Composer
        inline
        strip={
          pendingImages.length > 0 && (
            <div className="flex flex-wrap gap-2 px-3 pt-3">
              {pendingImages.map((image) => (
                <div key={image.url} className="relative">
                  <img
                    src={image.url}
                    alt=""
                    className="size-16 rounded-md border border-glass-stroke-card object-cover"
                  />
                  <button
                    type="button"
                    aria-label="Remove image"
                    disabled={sending}
                    onClick={() => removeImage(image.url)}
                    className="absolute -right-1.5 -top-1.5 rounded-full border border-glass-stroke-card bg-popover p-0.5 text-muted-foreground hover:text-foreground"
                  >
                    <X className="size-3" />
                  </button>
                </div>
              ))}
            </div>
          )
        }
        submit={
          // EXP-790: nothing typed while the agent works = Stop; otherwise
          // the send glyph (`ui-submit`), dimmed until there is something to
          // send. The plan-mode pill left this row (EXP-790): the mode is set
          // at launch and the plan card itself is where a plan is answered.
          stop ? (
            <ComposerSubmit
              stop
              disabled={!live}
              onClick={() => {
                if (!store.interrupt()) {
                  toast.error(`The session is no longer connected`)
                }
              }}
            />
          ) : (
            <ComposerSubmit
              disabled={
                sending || !live || (!text.trim() && pendingImages.length === 0)
              }
              onClick={() => void send()}
            />
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
        {/* EXP-698: the steer field is the mention field — `@` members, `#`
            issue refs and `:` emoji all work while steering. EXP-724: the `/`
            menu floats above it, so the field gets a positioned wrapper of its
            own (the mention popup anchors inside the field's own). */}
        <div className="relative">
          <MentionTextarea
            ref={fieldRef}
            value={text}
            onValueChange={(next) => store.setDraftText(next)}
            users={users}
            onKeyDown={(e) => {
              // The menu gets first refusal: with it open, Enter/Tab accept a
              // command and must NEVER send the half-typed draft.
              if (menu.handleKeyDown(e)) return
              if (e.key === `Enter` && !e.shiftKey) {
                e.preventDefault()
                void send()
              }
            }}
            onPaste={(e) => {
              if (e.clipboardData.files.length === 0) return
              e.preventDefault()
              addFiles(Array.from(e.clipboardData.files))
            }}
            placeholder={COMPOSER_PLACEHOLDER}
            rows={1}
            className={cn(
              `max-h-32 min-h-9 w-full border-none px-3 py-2 shadow-none focus-visible:border-transparent`,
              // The card IS the field chrome, so the field drops the stock
              // Textarea's glass fill (EXP-616).
              `bg-transparent`
            )}
          />
          {menu.open && (
            <SlashCommandMenu
              commands={menu.candidates}
              active={menu.active}
              onSelect={menu.accept}
              onHover={menu.setActive}
            />
          )}
        </div>
      </Composer>
      {/* EXP-877: the footer UNDER the card — plan mode, the attach `+`, then
          the model picker and the caller's usage slot pushed right. The card
          itself keeps nothing but the field and the round send glyph. */}
      <div
        data-testid="steer-composer-footer"
        className="flex items-center gap-3 px-2 pt-1.5 text-xs text-muted-foreground"
      >
        {planMode && <span className="text-sky-400">{PLAN_MODE_LABEL}</span>}
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
          disabled={sending}
          onClick={() => {
            filePickerOpenRef.current = true
            fileInputRef.current?.click()
          }}
        >
          {/* EXP-850 §13: the STEER composers attach with the `ui-add`
              plus (×4); comment and description editors keep
              `editor-image`. */}
          <UiAddIcon />
        </ComposerTool>
        <div className="ml-auto flex items-center gap-3">
          {model !== null &&
            (codex ? (
              /* Codex takes no mid-run model switch: the value is a word. */
              <span>{modelLabel(model)}</span>
            ) : (
              /* EXP-877: picking a model SENDS `/model <alias>` down the
                 ordinary message path — queued mid-turn like anything else,
                 and the republished `config_state` is the confirmation. No
                 optimistic write. */
              <InlinePicker
                label="Model"
                value={model}
                options={modelOptions}
                onChange={(value) => {
                  // The picker is CONTROLLED by the republished
                  // `config_state`, so a send that never left has to say so —
                  // otherwise the value silently snaps back.
                  if (!live || value === model) return
                  if (!onSend(`/model ${value}`)) {
                    toast.error(`The session is no longer connected`)
                  }
                }}
              />
            ))}
          {usageSlot}
        </div>
      </div>
      {/* EXP-724: `/clear` throws the conversation away, and the
          publisher runs whatever it receives — so every viewer confirms
          first, with the same copy. */}
      <Dialog
        open={confirming !== null}
        onOpenChange={(open) => {
          if (!open) setConfirming(null)
        }}
      >
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>
              {steerCommandConfirmCopy(confirming?.name ?? ``).title}
            </DialogTitle>
            <DialogDescription>
              {steerCommandConfirmCopy(confirming?.name ?? ``).body}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel onClick={() => setConfirming(null)} />
            <Button
              variant="destructive"
              onClick={() => {
                setConfirming(null)
                void send(true)
              }}
            >
              {steerCommandConfirmCopy(confirming?.name ?? ``).confirm}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}