import { useCallback, useMemo, useRef, useState } from "react"
import { CommandCandidateRow } from "@/components/autocomplete-rows"
import {
  filterSteerCommands,
  matchSlashDraft,
  steerCommandDraft,
  type SteerCommand,
} from "@/lib/steer-commands"

// EXP-724 — the steering composer's `/` menu. Deliberately NOT a
// MentionTextarea feature: that popup completes a token at the CARET and lets
// Enter fall through to its host, while this one owns the WHOLE draft (a
// command is the entire message) and must swallow the Enter that would
// otherwise send the half-typed command. The keyboard contract below mirrors
// mention-textarea.tsx rule for rule so the two popups feel identical, and the
// three native viewers mirror it again.

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
  const [active, setActive] = useState(0)

  const query = matchSlashDraft(text)
  const candidates = useMemo(
    () => (query === null ? [] : filterSteerCommands(commands, query)),
    [query, commands]
  )
  const open = candidates.length > 0 && dismissed !== text

  // A changed query starts at the top again — kept in a ref so the reset costs
  // no render pass of its own.
  const lastQuery = useRef<string | null>(query)
  if (lastQuery.current !== query) {
    lastQuery.current = query
    if (active !== 0) setActive(0)
  }
  const activeIndex = Math.min(active, Math.max(0, candidates.length - 1))

  const accept = useCallback(
    (command: SteerCommand) => {
      const next = steerCommandDraft(command)
      // A no-argument command leaves a draft that still matches the trigger
      // (`/clear`), so the acceptance itself closes the menu — otherwise the
      // next Enter would re-accept instead of sending.
      setDismissed(next)
      setActive(0)
      onAccept(next)
      return next
    },
    [onAccept]
  )

  const dismiss = useCallback(() => setDismissed(text), [text])

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLElement>): boolean => {
      if (!open) return false
      const count = candidates.length
      if (event.key === `ArrowDown`) {
        event.preventDefault()
        setActive((a) => (a + 1) % count)
        return true
      }
      if (event.key === `ArrowUp`) {
        event.preventDefault()
        setActive((a) => (a - 1 + count) % count)
        return true
      }
      // Plain Enter/Tab accept; a MODIFIED Enter (Cmd/Ctrl+Enter) falls
      // through to the host, exactly as in the comment composer.
      if ((event.key === `Enter` || event.key === `Tab`) && !event.metaKey && !event.ctrlKey) {
        event.preventDefault()
        accept(candidates[activeIndex])
        return true
      }
      if (event.key === `Escape`) {
        event.preventDefault()
        dismiss()
        return true
      }
      return false
    },
    [open, candidates, activeIndex, accept, dismiss]
  )

  return {
    open,
    candidates,
    active: activeIndex,
    setActive,
    handleKeyDown,
    accept,
    dismiss,
  }
}

/** The popup itself — the mention menu's glass panel, anchored above the
 *  composer by a `relative` wrapper the host provides. */
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
    <div className="absolute bottom-full left-0 z-20 mb-1 w-72 overflow-hidden rounded-xl glass-panel">
      {commands.map((command, index) => (
        <CommandCandidateRow
          key={command.name}
          command={command}
          active={index === active}
          onSelect={() => onSelect(command)}
          onHover={() => onHover(index)}
        />
      ))}
    </div>
  )
}
