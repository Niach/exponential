import { useEffect, useState } from "react"
import { Check, Github } from "lucide-react"
import type { BoardIcon } from "@exp/db-schema/domain"
import type { Team } from "@/db/schema"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
  BoardIconColorFields,
  BoardNameField,
  BoardPrefixField,
} from "@/components/board-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
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
  const [showRepo, setShowRepo] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [upgradeOpen, setUpgradeOpen] = useState(false)
  const [productIds, setProductIds] = useState<{
    pro: string | null
    business: string | null
    businessYearly: string | null
  }>({ pro: null, business: null, businessYearly: null })

  const [selection, setSelection] = useState<RepoSelection | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        pro: config.creemProProductId,
        business: config.creemBusinessProductId,
        businessYearly: config.creemBusinessYearlyProductId,
      })
    })
  }, [])

  const resetAll = () => {
    setName(``)
    setPrefix(``)
    setColor(`#6366f1`)
    setIcon(`code`)
    setShowRepo(false)
    setSelection(null)
    setError(null)
  }

  const handleNameChange = (value: string) => {
    setName(value)
    setPrefix(derivePrefix(value))
  }

  const handlePickerSelect = (repo: PickerRepo) => {
    setSelection({ kind: `inline`, repo })
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

  const selectedInlineName =
    selection?.kind === `inline` ? selection.repo.fullName : null

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next) resetAll()
          onOpenChange(next)
        }}
      >
        <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-[26rem]">
          <DialogHeader>
            <DialogTitle>Create board</DialogTitle>
          </DialogHeader>

          <form onSubmit={handleSubmit} className="space-y-4">
            <BoardNameField
              value={name}
              onChange={handleNameChange}
              autoFocus
            />
            <BoardPrefixField value={prefix} onChange={setPrefix} />
            <BoardIconColorFields
              icon={icon}
              onIconChange={setIcon}
              color={color}
              onColorChange={setColor}
            />

            <div className="space-y-2">
              <Label>Repository (optional)</Label>
              {showRepo ? (
                <ConnectedRepoPicker
                  teamId={teamId}
                  value={
                    selection?.kind === `registry`
                      ? selection.repositoryId
                      : null
                  }
                  onSelectRegistry={(repo) =>
                    setSelection({
                      kind: `registry`,
                      repositoryId: repo.id,
                      fullName: repo.fullName,
                    })
                  }
                  onConnectNew={handlePickerSelect}
                  appendedRow={
                    selectedInlineName ? (
                      <div className="flex w-full items-center gap-2 px-3 py-2 text-sm">
                        <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
                        <span className="truncate">{selectedInlineName}</span>
                        <Check className="ml-auto h-4 w-4 shrink-0 text-primary" />
                      </div>
                    ) : undefined
                  }
                />
              ) : (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-full justify-start text-muted-foreground"
                  onClick={() => setShowRepo(true)}
                >
                  <Github className="mr-2 h-4 w-4" />
                  Connect a GitHub repository
                </Button>
              )}
            </div>

            {error && (
              <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            )}

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
        proProductId={productIds.pro}
        businessProductId={productIds.business}
        businessYearlyProductId={productIds.businessYearly}
        teamId={teamId}
      />
    </>
  )
}
