import { useCallback, useMemo, useState } from "react"
import { TypeaheadMenu, useTypeahead } from "@exp/ui"
import { CommandCandidateRow } from "@/components/autocomplete-rows"
import {
  filterSteerCommands,
  matchSlashDraft,
  steerCommandDraft,
  type SteerCommand,
} from "@/lib/steer-commands"

// EXP-724 — the steering composer's `/` menu. Deliberately NOT a
// MentionTextarea feature: that popup completes a token at the CARET, while
// this one owns the WHOLE draft (a command is the entire message), so what
// lives here is the draft bookkeeping — which draft the menu was dismissed on,
// and the draft an acceptance leaves behind.
//
// EXP-941: the keyboard contract is no longer mirrored from
// mention-textarea.tsx — both popups now CALL the same `useTypeahead`
// (@exp/ui), so arrows/wrap/Enter/Tab/Escape and the "a modified Enter is the
// host's send" rule cannot drift apart. The three native viewers mirror it.

/** A stable empty list for the closed menu (a fresh `[]` per render would be a
 *  new `items` identity every time). */
const NO_COMMANDS: SteerCommand[] = []

export interface SlashCommandMenuState {
  /** Whether the popup should render (a matching draft, candidates, not
   *  dismissed). */
  open: boolean
  candidates: SteerCommand[]
  active: number
  setActive: (index: number) => void
  /** Handle one composer keystroke. Returns true when the menu CONSUMED it —
   *  the host must then do nothing else (in particular: not send). */
  handleKeyDown: (event: React.KeyboardEvent<HTMLElement>) => boolean
  /** Accept a candidate: returns the draft it leaves behind (also handed to
   *  `onAccept`). Never sends. */
  accept: (command: SteerCommand) => string
  /** Close the menu until the draft changes (Escape, blur). */
  dismiss: () => void
}

export function useSlashCommandMenu({
  text,
  commands,
  onAccept,
}: {
  /** The composer's whole draft. */
  text: string
  /** The catalog for this session's agent (steerCommandsFor). */
  commands: readonly SteerCommand[]
  /** Write the accepted draft back into the composer. */
  onAccept: (next: string) => void
}): SlashCommandMenuState {
  // Escape closes the menu for the draft it was pressed on; typing anything
  // (a different draft) offers it again. Storing the TEXT rather than a bool
  // is what makes "until the draft changes" free of an effect.
  const [dismissed, setDismissed] = useState<string | null>(null)

  const query = matchSlashDraft(text)
  const candidates = useMemo(
    () => (query === null ? [] : filterSteerCommands(commands, query)),
    [query, commands]
  )
  const open = candidates.length > 0 && dismissed !== text

  const accept = useCallback(
    (command: SteerCommand) => {
      const next = steerCommandDraft(command)
      // A no-argument command leaves a draft that still matches the trigger
      // (`/clear`), so the acceptance itself closes the menu — otherwise the
      // next Enter would re-accept instead of sending. The active row goes
      // back to the top on its own: the accepted draft is a new query.
      setDismissed(next)
      onAccept(next)
      return next
    },
    [onAccept]
  )

  const dismiss = useCallback(() => setDismissed(text), [text])

  // A dismissed menu hands the hook NO rows, which is exactly how the shared
  // typeahead says "handled nothing" — so a dismissed Enter sends, as before.
  const typeahead = useTypeahead<SteerCommand>({
    items: open ? candidates : NO_COMMANDS,
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
  commands,
  active,
  onSelect,
  onHover,
}: {
  commands: readonly SteerCommand[]
  active: number
  onSelect: (command: SteerCommand) => void
  onHover: (index: number) => void
}) {
  return (
    <TypeaheadMenu placement="above">
      {commands.map((command, index) => (
        <CommandCandidateRow
          key={command.name}
          command={command}
          active={index === active}
          onSelect={() => onSelect(command)}
          onHover={() => onHover(index)}
        />
      ))}
    </TypeaheadMenu>
  )
}
