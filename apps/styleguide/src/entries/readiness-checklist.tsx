import codingReadiness from "@exp/domain-contract/fixtures/coding-readiness.json"
import {
  ReadinessFixPill,
  ReadinessFixes,
  ReadinessProgress,
  ReadinessRow,
  ReadinessRows,
  conceptIcon,
  type ReadinessRowState,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// SLOP-7 (EXP-1121): the "Ready to code?" checklist. The app's popover
// (`apps/web/src/components/coding-readiness-checklist.tsx`) reads Electric
// and tRPC, and no entry imports an app composition, so this is the
// device-settings precedent: the checklist's REAL `@exp/ui` chrome
// (`ReadinessRow`, `ReadinessFixPill`, `ReadinessProgress`) over static rows.
// Every word comes from the contract fixture `coding-readiness.json`
// (byte-locked ×4): one case's `expected` steps, so the specimen cannot drift
// from what the four clients print.

const { copy, cases } = codingReadiness

type StepKey = `github` | `repository` | `device`
type Fix =
  | `connect_github`
  | `choose_repository`
  | `board_settings`
  | `open_devices`
  | `get_desktop_app`
  | `set_up_server`

/** The fields the specimen reads off a case's `expected`. */
interface ReadinessCase {
  name: string
  expected: {
    summary: string
    steps: {
      key: StepKey
      state: ReadinessRowState
      title: string
      body: string | null
      detail: string | null
      fixes: Fix[]
    }[]
  }
}

const CASE_NAME = `repo missing: GitHub met, repository current, device pending with last seen`

/** The fixture's case, or a build that fails naming what moved. */
function readinessCase(): ReadinessCase {
  const found = (cases as unknown as ReadinessCase[]).find((row) => row.name === CASE_NAME)
  if (!found) throw new Error(`coding-readiness.json has no "${CASE_NAME}" case`)
  return found
}

const CASE = readinessCase()

const STEP_ICONS: Record<StepKey, ReturnType<typeof conceptIcon>> = {
  github: conceptIcon(`ui-github`),
  repository: conceptIcon(`ui-branch`),
  device: conceptIcon(`ui-device`),
}

const FIX_ICONS: Partial<Record<Fix, ReturnType<typeof conceptIcon>>> = {
  connect_github: conceptIcon(`ui-github`),
  choose_repository: conceptIcon(`ui-branch`),
  open_devices: conceptIcon(`nav-devices`),
  get_desktop_app: conceptIcon(`ui-download`),
  set_up_server: conceptIcon(`ui-server`),
}

const FIX_LABELS: Record<Fix, string> = {
  connect_github: copy.fixConnectGithub,
  choose_repository: copy.fixChooseRepository,
  board_settings: copy.fixBoardSettings,
  open_devices: copy.fixOpenDevices,
  get_desktop_app: copy.fixGetDesktopApp,
  set_up_server: copy.fixSetUpServer,
}

const ChevronIcon = conceptIcon(`ui-chevron-right`)
const noop = () => {}

export const entry: StyleguideEntry = {
  id: `readiness-checklist`,
  section: `special`,
  owner: `SLOP-7`,
  title: `Ready to code? checklist`,
  blurb: `What Start coding opens while a run cannot start yet (EXP-1121/SLOP-7), one layout ×4. Three steps in a fixed order — GitHub connected, Repository connected, Device online — under a three-slice progress strip. A step is MET (green tick, muted title, its detail on the right), CURRENT (the first unmet one: amber ring, amber wash, its fixes inline, the first one primary) or PENDING (dashed grey ring, no fixes until it is current). The model and every word are the fixture-locked coding-readiness.json; this specimen is its "repo missing" case.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ReadinessRow`,
      file: `packages/ui/src/readiness-checklist.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `coding_readiness::render_popover`,
      file: `apps/desktop/crates/ui/src/coding_readiness.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `CodingReadinessSheet`,
      file: `apps/ios/Exponential/UI/Issue/CodingReadinessSheet.swift`,
    },
    android: {
      state: `ok`,
      symbol: `CodingReadinessSheet`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/issue/CodingReadinessSheet.kt`,
    },
  },
  island: () => (
    <div className="w-[23.5rem] overflow-hidden rounded-lg border border-glass-stroke bg-popover">
      <div className="px-4 pt-4 pb-3">
        <div className="text-sm font-semibold text-foreground">{copy.title}</div>
        <div className="mt-0.5 text-xs text-muted-foreground">{CASE.expected.summary}</div>
        <ReadinessProgress
          className="mt-3"
          states={CASE.expected.steps.map((step) => step.state)}
        />
      </div>
      <ReadinessRows className="border-t border-glass-stroke">
        {CASE.expected.steps.map((step) => (
          <ReadinessRow
            key={step.key}
            stepKey={step.key}
            icon={STEP_ICONS[step.key]}
            state={step.state}
            title={step.title}
            body={step.body}
            detail={step.detail}
          >
            {step.state === `current` && step.fixes.length > 0 && (
              <ReadinessFixes>
                {step.fixes.map((fix, index) => {
                  const Glyph = FIX_ICONS[fix]
                  return (
                    <ReadinessFixPill key={fix} primary={index === 0} onClick={noop}>
                      {Glyph && <Glyph />}
                      {FIX_LABELS[fix]}
                      {fix === `board_settings` && <ChevronIcon />}
                    </ReadinessFixPill>
                  )
                })}
              </ReadinessFixes>
            )}
          </ReadinessRow>
        ))}
      </ReadinessRows>
    </div>
  ),
}
