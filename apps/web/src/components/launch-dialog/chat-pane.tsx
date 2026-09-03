import { useMemo } from "react"
import { MAX_ACTION_INPUT_TEXT } from "@exp/db-schema/domain"
import { builtinChatAction } from "@/lib/builtin-actions"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import { Label } from "@/components/ui/label"
import { GlassGroup, GlassPickerRow } from "@/components/ui/glass-rows"
import { Textarea } from "@/components/ui/textarea"

// The Chat tab of the unified launch dialog (EXP-615): a free prompt on a
// repository's trunk clone — the web twin of opening a terminal tab in the
// desktop IDE. It rides the hidden "Chat" builtin action, so the field labels
// and the prompt placeholder come from that definition and can never drift
// from the other three clients. All state lives in the dialog shell.

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
            Connect a repository to this team to chat.
          </p>
        </div>
      ) : (
        <GlassGroup>
          <GlassPickerRow
            label="Repository"
            value={repoId || undefined}
            onValueChange={onRepoChange}
            placeholder="Select a repository"
            options={repos.map((repo) => ({
              value: repo.id,
              label: repo.fullName,
            }))}
          />
        </GlassGroup>
      )}
    </div>
  )
}
