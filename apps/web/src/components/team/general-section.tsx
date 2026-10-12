import { useEffect, useRef, useState } from "react"
import type { Team } from "@/db/schema"
import { contract } from "@exp/domain-contract"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"
import {
  FieldError,
  GlassGroup,
  GlassInputRow,
  GlassSectionHeader,
  GlassToggleRow,
  InlineStatus,
  SkeletonRows,
  Textarea,
} from "@exp/ui"
import { relativeTime } from "@/components/comment-rows/format"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"

/** EXP-1025: the team prompt's cap in UTF-8 bytes (the contract's; the
 *  server refuses more). Shown as the counter's denominator. */
export const TEAM_PROMPT_MAX_BYTES = contract.team.agentPromptMaxBytes

/** EXP-1025: the editor's copy, byte-identical on the IDE
 *  (`settings/team_general.rs`). EXP-1054: one short placeholder, no helper
 *  paragraph under the field — the counter is the footer. */
export const TEAM_PROMPT_TITLE = `Team prompt`
export const TEAM_PROMPT_PLACEHOLDER = `Rules every coding run of this team follows, as markdown.`

/** `12.3k` / `840` — the counter's short form. */
export function formatByteCount(bytes: number): string {
  if (bytes < 1000) return `${bytes}`
  return `${(bytes / 1024).toFixed(1)}k`
}

/** UTF-8 bytes — what the context pays for, and what the server counts. */
export function utf8ByteLength(text: string): number {
  return new TextEncoder().encode(text).byteLength
}

/** EXP-1025: the rough token readout beside the byte counter — the same
 *  chars÷4 estimate the context layout will draw (follow-up), labelled ≈ so
 *  nobody reads it as a measurement. */
export function approxTokens(bytes: number): string {
  const tokens = Math.round(bytes / 4)
  return tokens < 1000 ? `≈${tokens} tokens` : `≈${(tokens / 1000).toFixed(1)}k tokens`
}

// Team visibility is deliberately NOT configurable: every team is
// member-only (EXP-180), so this section is just the name.
export function TeamGeneralSection({ team }: { team: Team }) {
  const [name, setName] = useState(team.name)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    setName(team.name)
  }, [team.id, team.name])

  const dirty = name !== team.name

  const handleSave = async () => {
    if (!dirty) return
    setSaving(true)
    try {
      await trpc.teams.update.mutate(
        { teamId: team.id, name: name.trim() || team.name },
        SKIP_ERROR_TOAST
      )
    } catch (e) {
      failedAction(`save`, `the team name`, e, { retry: () => void handleSave() })
    } finally {
      setSaving(false)
    }
  }

  // EXP-1105: yolo mode — a synced team flag, owner-flipped like the rest of
  // this page. Every PR an agent opens merges at once and Reviews leaves the
  // nav; it comes back only while a PR is left open (a merge that failed).
  const [yoloBusy, setYoloBusy] = useState(false)
  const toggleYolo = async (yoloMode: boolean) => {
    setYoloBusy(true)
    try {
      await trpc.teams.update.mutate({ teamId: team.id, yoloMode }, SKIP_ERROR_TOAST)
    } catch (e) {
      failedAction(`update`, `yolo mode`, e, { retry: () => void toggleYolo(yoloMode) })
    } finally {
      setYoloBusy(false)
    }
  }

  // EXP-1025: the team prompt is NOT on the synced team row (server-only,
  // like an action's body), fetched once per team. EXP-1275 (EXP-1259 D9):
  // it AUTOSAVES `autosave.debounceMs` after the last keystroke and on blur;
  // the caption reads Saving → Saved (`savedHoldMs`) → Edited {ago}, never
  // Unsaved. `null` = still loading; the field is disabled until then so a
  // blur can never save an empty draft over a prompt that has not arrived.
  // FEED-79: a failed load = the empty (disabled) field + a toast with Retry.
  const [prompt, setPrompt] = useState<string | null>(null)
  const [savedPrompt, setSavedPrompt] = useState<string>(``)
  const [promptUpdatedAt, setPromptUpdatedAt] = useState<Date | null>(null)
  const [promptSaving, setPromptSaving] = useState(false)
  const [promptJustSaved, setPromptJustSaved] = useState(false)
  const [promptLoadFailed, setPromptLoadFailed] = useState(false)
  const [promptLoadAttempt, setPromptLoadAttempt] = useState(0)

  useEffect(() => {
    let cancelled = false
    setPrompt(null)
    setPromptLoadFailed(false)
    trpc.teams.getAgentPrompt
      .query({ teamId: team.id })
      .then((res) => {
        if (cancelled) return
        setPrompt(res.agentPrompt)
        setSavedPrompt(res.agentPrompt)
        setPromptUpdatedAt(
          res.agentPromptUpdatedAt ? new Date(res.agentPromptUpdatedAt) : null
        )
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setPromptLoadFailed(true)
        failedAction(`load`, `the team prompt`, e, {
          retry: () => setPromptLoadAttempt((attempt) => attempt + 1),
        })
      })
    return () => {
      cancelled = true
    }
  }, [team.id, promptLoadAttempt])

  const promptBytes = utf8ByteLength(prompt ?? ``)
  const promptOver = promptBytes > TEAM_PROMPT_MAX_BYTES
  const promptDirty = prompt !== null && prompt.trimEnd() !== savedPrompt

  // The draft whose save just failed: the timer never retries it on its own
  // (that would toast every debounce); a new keystroke, blur or Retry does.
  const failedPromptRef = useRef<string | null>(null)

  const handleSavePrompt = async () => {
    if (prompt === null || !promptDirty || promptOver || promptSaving) return
    setPromptSaving(true)
    setPromptJustSaved(false)
    try {
      await trpc.teams.update.mutate({ teamId: team.id, agentPrompt: prompt }, SKIP_ERROR_TOAST)
      const saved = prompt.trimEnd()
      setSavedPrompt(saved)
      setPromptUpdatedAt(new Date())
      setPromptJustSaved(true)
      failedPromptRef.current = null
    } catch (e) {
      failedPromptRef.current = prompt
      failedAction(`save`, `the team prompt`, e, { retry: () => void savePromptRef.current() })
    } finally {
      setPromptSaving(false)
    }
  }
  // The debounce timer and Retry run the LATEST save (the newest draft).
  const savePromptRef = useRef(handleSavePrompt)
  savePromptRef.current = handleSavePrompt

  // Re-armed when a save settles too: keystrokes typed while it was in
  // flight (whose timer found it busy) still save.
  useEffect(() => {
    if (!promptDirty || promptOver || promptSaving) return
    if (prompt === failedPromptRef.current) return
    const timer = setTimeout(
      () => void savePromptRef.current(),
      FEEDBACK_STATES.autosave.debounceMs
    )
    return () => clearTimeout(timer)
  }, [prompt, promptDirty, promptOver, promptSaving])

  useEffect(() => {
    if (!promptJustSaved) return
    const timer = setTimeout(
      () => setPromptJustSaved(false),
      FEEDBACK_STATES.autosave.savedHoldMs
    )
    return () => clearTimeout(timer)
  }, [promptJustSaved])

  const promptStatus = promptSaving ? `saving` : promptJustSaved ? `saved` : `idle`
  const promptEditedLabel = promptUpdatedAt
    ? FEEDBACK_STATES.autosave.captions.edited.replace(`{ago}`, relativeTime(promptUpdatedAt))
    : undefined

  return (
    <div className="space-y-5">
      <div>
        <GlassSectionHeader label="General" />
        {/* EXP-818: the Linear settings row — label left, value right, and
            it SAVES ITSELF on blur and on Enter (the device editor's Name row,
            the IDE's team_general.rs twin). No Save button. */}
        <GlassGroup>
          <GlassInputRow
            id="team-name"
            label="Name"
            value={name}
            maxLength={255}
            onChange={(e) => setName(e.target.value)}
            onBlur={() => void handleSave()}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                e.currentTarget.blur()
              }
            }}
            trailing={saving ? <InlineStatus state="saving" className="shrink-0" /> : undefined}
          />
        </GlassGroup>
      </div>

      <div>
        <GlassGroup>
          <GlassToggleRow
            id="team-yolo-mode"
            label="Yolo mode"
            checked={team.yoloMode}
            disabled={yoloBusy}
            onCheckedChange={(next) => void toggleYolo(next)}
          />
        </GlassGroup>
      </div>

      {/* EXP-1025: the team prompt — a RAW monospace field, never the
          markdown WYSIWYG: a prompt is read by a model, so the bytes shown
          must be the bytes it gets. Counter in bytes (the server's unit),
          token estimate beside it, autosaves (D9). */}
      <div>
        <GlassSectionHeader
          label={TEAM_PROMPT_TITLE}
          trailing={<InlineStatus state={promptStatus} savedLabel={promptEditedLabel} />}
        />
        <GlassGroup>
          {prompt === null && !promptLoadFailed ? (
            <SkeletonRows rows={2} dot={false} />
          ) : (
            <Textarea
              id="team-agent-prompt"
              data-testid="team-agent-prompt"
              value={prompt ?? ``}
              onChange={(e) => setPrompt(e.target.value)}
              onBlur={() => void handleSavePrompt()}
              placeholder={prompt === null ? undefined : TEAM_PROMPT_PLACEHOLDER}
              disabled={prompt === null}
              aria-invalid={promptOver || undefined}
              aria-describedby={promptOver ? `team-agent-prompt-error` : undefined}
              rows={10}
              spellCheck={false}
              className="min-h-48 rounded-none border-0 bg-transparent px-4 py-3 font-mono text-xs shadow-none focus-visible:border-0"
            />
          )}
          <div className="flex items-start justify-end gap-4 border-t border-glass-stroke-card px-4 py-2 text-xs leading-snug text-muted-foreground">
            <span
              className={`shrink-0 tabular-nums ${promptOver ? `text-destructive` : ``}`}
              data-testid="team-agent-prompt-counter"
            >
              {`${formatByteCount(promptBytes)} / ${formatByteCount(TEAM_PROMPT_MAX_BYTES)} bytes · ${approxTokens(promptBytes)}`}
            </span>
          </div>
        </GlassGroup>
        {promptOver && (
          <FieldError id="team-agent-prompt-error" className="mt-2">
            {`Too long: ${promptBytes} of ${TEAM_PROMPT_MAX_BYTES} bytes`}
          </FieldError>
        )}
      </div>
    </div>
  )
}
