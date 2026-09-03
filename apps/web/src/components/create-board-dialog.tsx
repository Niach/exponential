import { useEffect, useState } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { Team } from "@/db/schema"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import {
  BoardColorField,
  BoardNameField,
  BoardPrefixField,
} from "@/components/board-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { BoardRepoField } from "@/components/board-repo-field"
import { UpgradeDialog } from "@/components/upgrade-dialog"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { derivePrefix } from "@/lib/board"
import { useCreateBoard } from "@/hooks/use-create-board"

// The chosen backing repo: either an existing registry repo (by id) or a
// brand-new one picked through the GithubRepoPicker (connected inline by
// boards.create in the same transaction).
type RepoSelection =
  | { kind: `registry`; repositoryId: string; fullName: string }
  | { kind: `inline`; repo: PickerRepo }

export function CreateBoardDialog({
  open,
  onOpenChange,
  team,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  team: Team
}) {
  const teamId = team.id
  const { createBoard } = useCreateBoard()
  const [name, setName] = useState(``)
  const [prefix, setPrefix] = useState(``)
  const [color, setColor] = useState(`#6366f1`)
  const [icon, setIcon] = useState<BoardIcon>(`code`)
  const [submitting, setSubmitting] = useState(false)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    team: string | null
    teamYearly: string | null
  }>({ team: null, teamYearly: null })

  const [selection, setSelection] = useState<RepoSelection | null>(null)
  // EXP-712: the board's own branch; null = the repo's default. Reset with
  // the repo — a branch belongs to the repo it was picked in.
  const [branch, setBranch] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        team: config.creemTeamProductId,
        teamYearly: config.creemTeamYearlyProductId,
      })
    })
  }, [])

  const resetAll = () => {
    setName(``)
    setPrefix(``)
    setColor(`#6366f1`)
    setIcon(`code`)
    setSelection(null)
    setBranch(null)
    setError(null)
  }

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const changeSelection = (next: RepoSelection | null) => {
    setSelection(next)
    setBranch(null)
  }

  const canSubmit = Boolean(name.trim()) && Boolean(prefix.trim())

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim() || !prefix.trim()) return

    setSubmitting(true)
    setError(null)
    const repository = !selection
      ? undefined
      : selection.kind === `registry`
        ? { repositoryId: selection.repositoryId }
        : {
            fullName: selection.repo.fullName,
            defaultBranch: selection.repo.defaultBranch,
            private: selection.repo.private,
          }
    const result = await createBoard({
      teamId,
      name,
      prefix,
      color,
      icon,
      repository,
      defaultBranch: repository ? (branch ?? undefined) : undefined,
    })
    setSubmitting(false)
    if (result.ok) {
      resetAll()
      onOpenChange(false)
      return
    }
    if (result.error.kind === `planLimit`) {
      onOpenChange(false)
      setUpgradeOpen(true)
    } else {
      setError(result.error.message)
    }
  }

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next) resetAll()
          onOpenChange(next)
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Create board</DialogTitle>
          </DialogHeader>

          {/* The form is the flex middle of the panel: its fields scroll in
              the DialogBody while the submit button rides the pinned footer —
              still inside the form, so Enter and the click both submit. */}
          <form
            onSubmit={handleSubmit}
            className="flex min-h-0 flex-1 flex-col gap-4"
          >
            <DialogBody className="space-y-4">
              <BoardNameField
                value={name}
                onChange={handleNameChange}
                autoFocus
                icon={icon}
                onIconChange={setIcon}
                color={color}
              />
              <BoardPrefixField value={prefix} onChange={setPrefix} />
              <BoardColorField color={color} onColorChange={setColor} />

              <BoardRepoField
                teamId={teamId}
                repositoryId={
                  selection?.kind === `registry` ? selection.repositoryId : null
                }
                inlineRepo={selection?.kind === `inline` ? selection.repo : null}
                onSelectRegistry={(repo) =>
                  changeSelection(
                    repo
                      ? {
                          kind: `registry`,
                          repositoryId: repo.id,
                          fullName: repo.fullName,
                        }
                      : null
                  )
                }
                onConnectNew={(repo: PickerRepo) =>
                  changeSelection({ kind: `inline`, repo })
                }
                branch={branch}
                onBranchChange={setBranch}
              />

              {error && (
                <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                  {error}
                </div>
              )}
            </DialogBody>

            <DialogFooter>
              <Button type="submit" disabled={!canSubmit || submitting}>
                {submitting ? `Creating...` : `Create board`}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <UpgradeDialog
        open={upgradeOpen}
        onOpenChange={setUpgradeOpen}
        title="Board limit reached"
        description="You've reached the maximum number of boards for your plan. Upgrade to create more."
        teamProductId={productIds.team}
        teamYearlyProductId={productIds.teamYearly}
        teamId={teamId}
      />
    </>
  )
}
