import { useMemo } from "react"
import { MAX_ACTION_INPUT_TEXT } from "@exp/db-schema/domain"
import { builtinChatAction } from "@/lib/builtin-actions"
import type { ActionRepoOption } from "@/components/action-editor-dialog"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
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
      <div className="flex min-h-0 flex-col gap-2 sm:flex-1">
        <Label htmlFor="chat-prompt">Prompt</Label>
        <Textarea
          id="chat-prompt"
          autoFocus
          value={prompt}
          onChange={(e) => onPromptChange(e.target.value)}
          placeholder={promptDef?.placeholder}
          className="min-h-28 sm:h-full sm:min-h-0 sm:flex-1"
          // Client parity with the server's per-value cap, so a long paste is
          // refused at the field instead of at submit.
          maxLength={MAX_ACTION_INPUT_TEXT}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="chat-repo">Repository</Label>
        {repos.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Connect a repository to this team to chat.
          </p>
        ) : (
          <Select value={repoId || undefined} onValueChange={onRepoChange}>
            <SelectTrigger id="chat-repo" className="w-full">
              <SelectValue placeholder="Select a repository" />
            </SelectTrigger>
            <SelectContent>
              {repos.map((repo) => (
                <SelectItem key={repo.id} value={repo.id}>
                  {repo.fullName}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>
    </div>
  )
}
