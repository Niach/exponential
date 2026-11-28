import { useState, type ReactNode } from "react"
import { REPO_PAGE_COPY as COPY } from "@exp/domain-contract/repo-page"
import {
  Button,
  DialogFooter,
  GlassGroup,
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
 *  Cancel / Create (`repositories.create`, owners). #EXP-2171 `page`: New ›
 *  Repository's hosted tile (`copy.new`): + Description and Run CI (on). */
export function AddEcodeRepositoryForm({
  teamId,
  onCancel,
  onCreated,
  page = false,
}: {
  teamId: string
  onCancel: () => void
  onCreated: (repository: { id: string; slug: string }) => void | Promise<void>
  page?: boolean
}) {
  const [description, setDescription] = useState(``)
  const [runCi, setRunCi] = useState(true)
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
      const { repository } = await trpc.repositories.create.mutate(
        {
          teamId,
          name,
          defaultBranch: branch,
          mirrorUrl: mirror ? mirrorUrl.trim() : null,
          ...(page && description.trim() ? { description: description.trim() } : {}),
        },
        SKIP_ERROR_TOAST
      )
      // Run CI: our CI on (repositories configure_ci); a failure here leaves
      // the repository created, its Settings tab turns it on later.
      if (page && runCi) {
        await trpc.ci.setRepoConfig
          .mutate({ repositoryId: repository.id, enabled: true }, SKIP_ERROR_TOAST)
          .catch((err: unknown) => failedAction(`turn on`, `CI`, err))
      }
      await onCreated(repository)
    } catch (err) {
      failedAction(`create`, `the repository`, err, { retry: () => void create() })
    } finally {
      setPending(false)
    }
  }

  const Footer = page ? PageFooter : DialogFooter
  // A page's fields are THE form group (New board's); the dialog is the surface.
  const Fields = page ? GlassGroup : ListBody
  return (
    <form
      data-testid="add-ecode-repository"
      onSubmit={(e) => {
        e.preventDefault()
        void create()
      }}
      className="space-y-3"
    >
      <Fields>
        <GlassInputRow
          id="ecode-repo-name"
          label={COPY.add.name}
          value={name}
          autoFocus
          placeholder="app"
          aria-invalid={name.length > 0 && !NAME.test(name)}
          onChange={(e) => setName(e.target.value.trim())}
        />
        {page && (
          <GlassInputRow
            id="ecode-repo-description"
            label={COPY.new.description}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        )}
        <ListRow className="gap-3 rounded-none px-4 py-3">
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
        {page && <GlassToggleRow id="ecode-repo-ci" label={COPY.new.runCi} checked={runCi} onCheckedChange={setRunCi} />}
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
      </Fields>
      <Footer>
        <Button type="button" variant="ghost" onClick={onCancel}>
          {COPY.add.cancel}
        </Button>
        <Button type="submit" disabled={!valid} pending={pending}>
          {page ? COPY.new.create : COPY.add.create}
        </Button>
      </Footer>
    </form>
  )
}

/** THE FormActions row on a page (end-aligned, Cancel then the submit). */
function PageFooter({ children }: { children: ReactNode }) {
  return <div className="flex justify-end gap-2">{children}</div>
}

/** The dialog's field list: the dialog is the surface. */
function ListBody({ children }: { children: ReactNode }) {
  return <div className={SETTINGS_LIST_CLASS}>{children}</div>
}
