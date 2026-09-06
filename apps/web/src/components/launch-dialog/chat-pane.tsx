import { useMemo } from "react"
import { MAX_ACTION_INPUT_TEXT } from "@exp/db-schema/domain"
import { builtinChatAction } from "@/lib/builtin-actions"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import { Label } from "@/components/ui/label"
import {
  GlassGroup,
  GlassPickerRow,
  type GlassPickerOption,
} from "@/components/ui/glass-rows"
import { Textarea } from "@/components/ui/textarea"

// The Chat tab of the unified launch dialog (EXP-615): a free prompt with an
// OPTIONAL repository (EXP-739) — pick one and the run gets its own
// `exp/chat-<id8>` worktree cut from that repo's default branch, leave it and
// the chat runs in the agent's scratch dir with only the Exponential MCP
// server wired up. It rides the
// hidden "Chat" builtin action, so the field labels and the prompt placeholder
// come from that definition and can never drift from the other three clients.
// All state lives in the dialog shell.

// EXP-758: repo-less is a real choice, so it needs a real option — without one
// a picked repo could never be cleared again. Radix Select forbids an
// empty-string item value, so it rides a sentinel inside the picker only (the
// CLI_DEFAULT_MODEL/EFFORT pattern); `onRepoChange` still speaks the empty
// string the shell and the server understand.
export const NO_REPO = `no-repo`

export function ChatPane({
  prompt,
  onPromptChange,
  repoId,
  onRepoChange,
  repos,
  teamId,
}: {
  prompt: string
  onPromptChange: (value: string) => void
  repoId: string
  onRepoChange: (repoId: string) => void
  /** The team's connected repos (shell-fetched); empty = nothing to chat on. */
  repos: ActionRepoOption[]
  teamId: string
}) {
  const inputDefs = useMemo(() => builtinChatAction(teamId).inputs, [teamId])
  const promptDef = inputDefs.find((def) => def.key === `prompt`)
  const repoOptions: GlassPickerOption[] = [
    { value: NO_REPO, label: `No repository` },
    ...repos.map((repo) => ({ value: repo.id, label: repo.fullName })),
  ]

  return (
    // Shrink only under `sm:` — see the actions pane's note (EXP-313).
    <div className="flex shrink-0 flex-col gap-3 sm:min-h-0 sm:shrink">
      {/* EXP-616: the prompt is its OWN glass card — caption-sized label,
          borderless field. The desktop column's stretch lives on the CARD now,
          the textarea just fills it. */}
      <GlassGroup className="min-h-0 sm:flex-1">
        <div className="flex min-h-0 flex-1 flex-col gap-1 p-3">
          <Label htmlFor="chat-prompt" className="text-xs text-foreground/50">
            Prompt
          </Label>
          <Textarea
            id="chat-prompt"
            autoFocus
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
            placeholder={promptDef?.placeholder}
            className="min-h-28 border-0 bg-transparent px-0 shadow-none focus-visible:ring-0 sm:h-full sm:min-h-0 sm:flex-1"
            // Client parity with the server's per-value cap, so a long paste
            // is refused at the field instead of at submit.
            maxLength={MAX_ACTION_INPUT_TEXT}
          />
        </div>
      </GlassGroup>
      {repos.length === 0 ? (
        <div className="space-y-2">
          <Label>Repository</Label>
          <p className="text-xs text-muted-foreground">
            No repository connected. The chat runs without one.
          </p>
        </div>
      ) : (
        <GlassGroup>
          <GlassPickerRow
            label="Repository"
            value={repoId || NO_REPO}
            onValueChange={(value) =>
              onRepoChange(value === NO_REPO ? `` : value)
            }
            options={repoOptions}
          />
        </GlassGroup>
      )}
    </div>
  )
}
