import { useState } from "react"
import { REPO_PAGE_COPY as COPY } from "@exp/domain-contract/repo-page"
import {
  Button,
  DialogFooter,
  GlassInputRow,
  GlassToggleRow,
  ListRow,
  SETTINGS_LIST_CLASS,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"

const BRANCHES = [`main`, `master`, `develop`] as const
const NAME = /^[A-Za-z0-9._-]+$/

/** #EXP-1616 (repo-page.json `copy.add`): the Exponential side of Add
 *  repository: name, default branch, the optional GitHub mirror, then
 *  Cancel / Create (`repositories.create`, owners). */
export function AddEcodeRepositoryForm({
  teamId,
  onCancel,
  onCreated,
}: {
  teamId: string
  onCancel: () => void
  onCreated: () => void | Promise<void>
}) {
  const [name, setName] = useState(``)
  const [branch, setBranch] = useState<string>(`main`)
  const [mirror, setMirror] = useState(false)
  const [mirrorUrl, setMirrorUrl] = useState(``)
  const [pending, setPending] = useState(false)
  const valid = NAME.test(name) && !name.startsWith(`.`) && !name.endsWith(`.git`) && (!mirror || mirrorUrl.startsWith(`https://`))

  const create = async () => {
    if (!valid || pending) return
    setPending(true)
    try {
      await trpc.repositories.create.mutate(
        { teamId, name, defaultBranch: branch, mirrorUrl: mirror ? mirrorUrl.trim() : null },
        SKIP_ERROR_TOAST
      )
      await onCreated()
    } catch (err) {
      failedAction(`create`, `the repository`, err, { retry: () => void create() })
    } finally {
      setPending(false)
    }
  }

  return (
    <form
      data-testid="add-ecode-repository"
      onSubmit={(e) => {
        e.preventDefault()
        void create()
      }}
      className="space-y-3"
    >
      <div className={SETTINGS_LIST_CLASS}>
        <GlassInputRow
          id="ecode-repo-name"
          label={COPY.add.name}
          value={name}
          autoFocus
          placeholder="app"
          aria-invalid={name.length > 0 && !NAME.test(name)}
          onChange={(e) => setName(e.target.value.trim())}
        />
        <ListRow className="gap-3 px-4 py-3">
          <span className="flex-1 text-sm">{COPY.add.defaultBranch}</span>
          <Select value={branch} onValueChange={setBranch}>
            <SelectTrigger size="sm" className="w-auto font-mono">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {BRANCHES.map((b) => (
                <SelectItem key={b} value={b} className="font-mono">
                  {b}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </ListRow>
        <GlassToggleRow id="ecode-repo-mirror" label={COPY.add.mirror} checked={mirror} onCheckedChange={setMirror} />
        {mirror && (
          <GlassInputRow
            id="ecode-repo-mirror-url"
            label={COPY.add.mirrorUrl}
            value={mirrorUrl}
            placeholder="https://github.com/owner/name"
            onChange={(e) => setMirrorUrl(e.target.value)}
          />
        )}
      </div>
      <DialogFooter>
        <Button type="button" variant="ghost" onClick={onCancel}>
          {COPY.add.cancel}
        </Button>
        <Button type="submit" disabled={!valid} pending={pending}>
          {COPY.add.create}
        </Button>
      </DialogFooter>
    </form>
  )
}
