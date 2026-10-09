import codingReadiness from "@exp/domain-contract/fixtures/coding-readiness.json"
import { Button, RepositoryPickerList, conceptIcon, type RepositoryPickerRow } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// SLOP-7: the repository picker's BODY (`RepositoryPickerList`), the list the
// readiness checklist's "Choose repository" and the Add-repository dialog
// share. The rows are the contract fixture's (`coding-readiness.json`
// `picker`, byte-locked ×4): its first case's ranked rows, minus one so the
// three tag kinds read at a glance, with one row marked private for the lock.

const { copy, picker } = codingReadiness

interface PickerCase {
  expected: { id: string; fullName: string; matches: boolean; tag: string | null }[]
}

/** The ranked rows of the fixture's first picker case. */
const RANKED = (picker as unknown as PickerCase[])[0]?.expected ?? []
if (RANKED.length === 0) throw new Error(`coding-readiness.json has no picker rows`)

/** Kept: the board match, a repo another board backs, an untagged one. */
const KEPT = new Set([`r-app`, `r-web`, `r-own`])
const PRIVATE = new Set([`r-web`])

const ROWS: RepositoryPickerRow[] = RANKED.filter((row) => KEPT.has(row.id)).map((row) => ({
  id: row.id,
  fullName: row.fullName,
  private: PRIVATE.has(row.id),
  tag: row.tag,
  emphasis: row.matches,
}))

const AddIcon = conceptIcon(`ui-add`)
const noop = () => {}

export const entry: StyleguideEntry = {
  id: `picker-repository`,
  section: `general`,
  owner: `SLOP-7`,
  title: `Repository picker`,
  blurb: `The repositories a board can code against, as owner/name rows with the GitHub glyph (SLOP-7). A private repo carries a lock; a trailing tag says why a row sits where it does — "matches board" in green ranks first, "used by Website" (muted) names the first OTHER board it already backs. One search field on top; the footer row leaves for GitHub to add another. Ranking and words are the fixture-locked coding-readiness.json picker cases ×4.`,
  status: {
    web: {
      state: `ok`,
      symbol: `RepositoryPicker / RepositoryPickerList`,
      file: `packages/ui/src/picker/repository-picker.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `AddRepositoryDialogView`,
      file: `apps/desktop/crates/ui/src/settings/add_repository_dialog.rs`,
      note: `The readiness popover's inline picker ranks with coding_readiness::picker_rows.`,
    },
    ios: {
      state: `ok`,
      symbol: `GithubRepoPicker`,
      file: `apps/ios/Exponential/UI/Settings/GithubRepoPicker.swift`,
    },
    android: {
      state: `ok`,
      symbol: `GithubRepoPickerSheet`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/onboarding/GithubRepoPickerSheet.kt`,
    },
  },
  island: () => (
    <div className="w-[20rem]">
      <RepositoryPickerList
        rows={ROWS}
        onPick={noop}
        searchPlaceholder={copy.pickerSearch}
        footer={
          <Button
            variant="ghost"
            size="sm"
            className="w-full justify-start rounded-none text-muted-foreground"
            onClick={noop}
          >
            <AddIcon />
            {copy.pickerAddFromGithub}
          </Button>
        }
      />
    </div>
  ),
}
