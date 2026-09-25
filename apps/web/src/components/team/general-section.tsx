import { useEffect, useState } from "react"
import type { Team } from "@/db/schema"
import { contract } from "@exp/domain-contract"
import { GlassGroup, GlassInputRow, GlassSectionHeader, Textarea } from "@exp/ui"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"

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
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setName(team.name)
  }, [team.id, team.name])

  const dirty = name !== team.name

  const handleSave = async () => {
    if (!dirty) return
    setSaving(true)
    setError(null)
    try {
      await trpc.teams.update.mutate({
        teamId: team.id,
        name: name.trim() || team.name,
      })
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to save changes`)
    } finally {
      setSaving(false)
    }
  }

  // EXP-1025: the team prompt is NOT on the synced team row (server-only,
  // like an action's body) — fetched once per team, saved on blur like the
  // name row. `null` = still loading; the field is disabled until then so a
  // blur can never save an empty draft over a prompt that has not arrived.
  const [prompt, setPrompt] = useState<string | null>(null)
  const [savedPrompt, setSavedPrompt] = useState<string>(``)
  const [promptUpdatedAt, setPromptUpdatedAt] = useState<Date | null>(null)
  const [promptSaving, setPromptSaving] = useState(false)
  const [promptError, setPromptError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setPrompt(null)
    setPromptError(null)
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
        setPromptError(trpcErrorMessage(e, `Couldn't load the team prompt`))
      })
    return () => {
      cancelled = true
    }
  }, [team.id])

  const promptBytes = utf8ByteLength(prompt ?? ``)
  const promptOver = promptBytes > TEAM_PROMPT_MAX_BYTES
  const promptDirty = prompt !== null && prompt.trimEnd() !== savedPrompt

  const handleSavePrompt = async () => {
    if (prompt === null || !promptDirty || promptOver || promptSaving) return
    setPromptSaving(true)
    setPromptError(null)
    try {
      await trpc.teams.update.mutate({ teamId: team.id, agentPrompt: prompt })
      const saved = prompt.trimEnd()
      setSavedPrompt(saved)
      setPromptUpdatedAt(new Date())
    } catch (e) {
      setPromptError(trpcErrorMessage(e, `Couldn't save the team prompt`))
    } finally {
      setPromptSaving(false)
    }
  }

  const promptCaption = promptSaving
    ? `Saving…`
    : promptDirty
      ? `Unsaved`
      : promptUpdatedAt
        ? `Edited ${relativeTime(promptUpdatedAt)}`
        : null

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
            trailing={
              saving ? (
                <span className="shrink-0 text-xs text-muted-foreground">Saving…</span>
              ) : dirty ? (
                <span className="shrink-0 text-xs text-muted-foreground">Unsaved</span>
              ) : undefined
            }
          />
        </GlassGroup>
      </div>

      {error && <p className="text-sm text-destructive">{error}</p>}

      {/* EXP-1025: the team prompt — a RAW monospace field, never the
          markdown WYSIWYG: a prompt is read by a model, so the bytes shown
          must be the bytes it gets. Counter in bytes (the server's unit),
          token estimate beside it, saves on blur. */}
      <div>
        <GlassSectionHeader
          label={TEAM_PROMPT_TITLE}
          trailing={
            promptCaption ? (
              <span className="text-xs text-muted-foreground">{promptCaption}</span>
            ) : undefined
          }
        />
        <GlassGroup>
          <Textarea
            id="team-agent-prompt"
            data-testid="team-agent-prompt"
            value={prompt ?? ``}
            onChange={(e) => setPrompt(e.target.value)}
            onBlur={() => void handleSavePrompt()}
            placeholder={prompt === null ? `Loading…` : TEAM_PROMPT_PLACEHOLDER}
            disabled={prompt === null}
            aria-invalid={promptOver || undefined}
            rows={10}
            spellCheck={false}
            className="min-h-48 rounded-none border-0 bg-transparent px-4 py-3 font-mono text-xs shadow-none focus-visible:border-0"
          />
          <div className="flex items-start justify-end gap-4 border-t border-glass-stroke-card px-4 py-2 text-xs leading-snug text-muted-foreground">
            <span
              className={`shrink-0 tabular-nums ${promptOver ? `text-destructive` : ``}`}
              data-testid="team-agent-prompt-counter"
            >
              {`${formatByteCount(promptBytes)} / ${formatByteCount(TEAM_PROMPT_MAX_BYTES)} bytes · ${approxTokens(promptBytes)}`}
            </span>
          </div>
        </GlassGroup>
        {promptError && (
          <p className="mt-2 text-sm text-destructive">{promptError}</p>
        )}
      </div>
    </div>
  )
}
