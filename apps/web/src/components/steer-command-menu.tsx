import { useCallback, useMemo, useState } from "react"
import { TypeaheadMenu, useTypeahead } from "@exp/ui"
import {
  CommandCandidateRow,
  PromptCandidateRow,
} from "@/components/autocomplete-rows"
import {
  filterSlashPrompts,
  filterSteerCommands,
  matchSlashDraft,
  steerCommandDraft,
  type SlashPrompt,
  type SteerCommand,
} from "@/lib/steer-commands"

// EXP-724 — the composers' `/` menu. Deliberately NOT a MentionTextarea
// feature: that popup completes a token at the CARET, while this one owns the
// WHOLE draft (a command is the entire message), so what lives here is the
// draft bookkeeping — which draft the menu was dismissed on, and the draft an
// acceptance leaves behind.
//
// NEW-2: ONE menu = the contract builtins (`steerCommands`: compact, model,
// effort; a live session only) and then the team's PROMPTS (icon + name).
// Accepting a command leaves its draft; accepting a prompt hands it to the
// host (`onPickPrompt`): a live session inserts its body into the draft, the
// launch composer makes it the new session's subject.
//
// EXP-941: the keyboard contract is no longer mirrored from
// mention-textarea.tsx — both popups now CALL the same `useTypeahead`
// (@exp/ui), so arrows/wrap/Enter/Tab/Escape and the "a modified Enter is the
// host's send" rule cannot drift apart. The three native viewers mirror it.

/** One row of the `/` menu. */
export type SlashCandidate =
  | { kind: `command`; command: SteerCommand }
  | { kind: `prompt`; prompt: SlashPrompt }

/** A stable empty list for the closed menu (a fresh `[]` per render would be a
 *  new `items` identity every time). */
const NO_CANDIDATES: SlashCandidate[] = []
const NO_PROMPTS: SlashPrompt[] = []

export interface SlashCommandMenuState {
  /** Whether the popup should render (a matching draft, candidates, not
   *  dismissed). */
  open: boolean
  candidates: SlashCandidate[]
  active: number
  setActive: (index: number) => void
  /** Handle one composer keystroke. Returns true when the menu CONSUMED it —
   *  the host must then do nothing else (in particular: not send). */
  handleKeyDown: (event: React.KeyboardEvent<HTMLElement>) => boolean
  /** Accept a candidate: a command returns the draft it leaves behind (also
   *  handed to `onAccept`); a prompt goes to `onPickPrompt` and returns the
   *  draft unchanged. Never sends. */
  accept: (candidate: SlashCandidate) => string
  /** Close the menu until the draft changes (Escape, blur). */
  dismiss: () => void
}

export function useSlashCommandMenu({
  text,
  commands,
  prompts = NO_PROMPTS,
  onAccept,
  onPickPrompt,
}: {
  /** The composer's whole draft. */
  text: string
  /** The builtins for this session's agent (steerCommandsFor); none before a
   *  session exists. */
  commands: readonly SteerCommand[]
  /** The team's prompts (usePrompts), listed after the builtins. */
  prompts?: readonly SlashPrompt[]
  /** Write the accepted command's draft back into the composer. */
  onAccept: (next: string) => void
  /** A prompt was picked (without it, prompts are not offered). */
  onPickPrompt?: (prompt: SlashPrompt) => void
}): SlashCommandMenuState {
  // Escape closes the menu for the draft it was pressed on; typing anything
  // (a different draft) offers it again. Storing the TEXT rather than a bool
  // is what makes "until the draft changes" free of an effect.
  const [dismissed, setDismissed] = useState<string | null>(null)

  const query = matchSlashDraft(text)
  const offerPrompts = onPickPrompt !== undefined
  const candidates = useMemo<SlashCandidate[]>(() => {
    if (query === null) return []
    return [
      ...filterSteerCommands(commands, query).map(
        (command): SlashCandidate => ({ kind: `command`, command })
      ),
      ...(offerPrompts ? filterSlashPrompts(prompts, query) : []).map(
        (prompt): SlashCandidate => ({ kind: `prompt`, prompt })
      ),
    ]
  }, [query, commands, prompts, offerPrompts])
  const open = candidates.length > 0 && dismissed !== text

  const accept = useCallback(
    (candidate: SlashCandidate) => {
      if (candidate.kind === `prompt`) {
        // The host replaces the draft (or not, on a failed fetch): the menu
        // stays shut for the draft it was picked on either way.
        setDismissed(text)
        onPickPrompt?.(candidate.prompt)
        return text
      }
      const next = steerCommandDraft(candidate.command)
      // A no-argument command leaves a draft that still matches the trigger,
      // so the acceptance itself closes the menu — otherwise the next Enter
      // would re-accept instead of sending. The active row goes back to the
      // top on its own: the accepted draft is a new query.
      setDismissed(next)
      onAccept(next)
      return next
    },
    [onAccept, onPickPrompt, text]
  )

  const dismiss = useCallback(() => setDismissed(text), [text])

  // A dismissed menu hands the hook NO rows, which is exactly how the shared
  // typeahead says "handled nothing" — so a dismissed Enter sends, as before.
  const typeahead = useTypeahead<SlashCandidate>({
    items: open ? candidates : NO_CANDIDATES,
    onAccept: accept,
    onDismiss: dismiss,
    resetKey: query,
  })

  return {
    open,
    candidates,
    active: typeahead.active,
    setActive: typeahead.setActive,
    handleKeyDown: typeahead.handleKeyDown,
    accept,
    dismiss,
  }
}

/** The popup itself — the shared `TypeaheadMenu`, exactly the surface the
 *  mention / issue-ref / emoji panels wear, anchored above the composer by a
 *  `relative` wrapper the host provides. */
export function SlashCommandMenu({
  candidates,
  active,
  onSelect,
  onHover,
}: {
  candidates: readonly SlashCandidate[]
  active: number
  onSelect: (candidate: SlashCandidate) => void
  onHover: (index: number) => void
}) {
  return (
    <TypeaheadMenu placement="above" data-testid="slash-menu">
      {candidates.map((candidate, index) =>
        candidate.kind === `command` ? (
          <CommandCandidateRow
            key={`command:${candidate.command.name}`}
            command={candidate.command}
            active={index === active}
            onSelect={() => onSelect(candidate)}
            onHover={() => onHover(index)}
          />
        ) : (
          <PromptCandidateRow
            key={`prompt:${candidate.prompt.id}`}
            prompt={candidate.prompt}
            active={index === active}
            onSelect={() => onSelect(candidate)}
            onHover={() => onHover(index)}
          />
        )
      )}
    </TypeaheadMenu>
  )
}
