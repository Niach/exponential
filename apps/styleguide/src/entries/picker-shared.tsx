import type { ReactNode } from "react"

import { ComposerOptionsRow, Label, Picker, PickerList, Switch, type PickerItem } from "@exp/ui"

import type { ComponentPlatform, ComponentStatus } from "../components.tsx"

/**
 * EXP-1021 — what the eleven picker entries share.
 *
 * Not an entry itself (nothing in `sections.json` names it): just the status
 * rows and the one demo shell, so eleven files do not each re-derive where
 * the four picker modules live or how an open surface is framed.
 */

/** The four picker modules, one per platform — every entry's primitive row. */
export const PICKER_FILES = {
  web: `packages/ui/src/picker`,
  desktop: `apps/desktop/crates/ui/src/picker`,
  ios: `apps/ios/ExpUI/Sources/Picker`,
  android: `apps/android/app/src/main/java/com/exponential/app/ui/components/picker`,
} as const

function ok(symbol: string, file: string, note?: string): ComponentStatus {
  return note === undefined ? { state: `ok`, symbol, file } : { state: `ok`, symbol, file, note }
}

/**
 * The status table of ONE typed picker. The ten are named the same on three
 * platforms (EXP-1029's whole point), so one `name` answers for web, iOS and
 * Android; the IDE is Rust, where the symbol IS the module function, derived
 * from its file rather than transcribed.
 */
export function typedPickerStatus(
  name: string,
  files: { web: string; desktop: string; ios: string; android: string },
  note?: string
): Record<ComponentPlatform, ComponentStatus> {
  return {
    web: ok(name, `${PICKER_FILES.web}/${files.web}`, note),
    desktop: ok(
      `picker::${files.desktop.replace(/\.rs$/, ``)}`,
      `${PICKER_FILES.desktop}/${files.desktop}`,
      note
    ),
    ios: ok(name, `${PICKER_FILES.ios}/${files.ios}`, note),
    android: ok(name, `${PICKER_FILES.android}/${files.android}`, note),
  }
}

/**
 * The demo canvas: a picker's resting state is its TRIGGER, and its surface
 * is a portal that renders nothing at rest — so every entry shows the trigger
 * beside the surface's BODY (`PickerList`), framed the way the popover frames
 * it. One caption names which half is which.
 */
export function PickerSpecimen({
  trigger,
  surface,
  inline,
  caption,
}: {
  trigger: ReactNode
  surface: ReactNode
  /** N-6: the INLINE presentation, disclosed — what every picker inside a
   *  dialog (the IDE's dialog windows, the phones' sheets) wears: the same
   *  rows in the host's flow under the trigger. One entry shows it. */
  inline?: ReactNode
  caption?: string
}) {
  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-center gap-2">{trigger}</div>
      <div className="w-[18rem] overflow-hidden rounded-lg border border-glass-stroke bg-popover">
        {surface}
      </div>
      {inline !== undefined && (
        <div
          data-testid="picker-inline-specimen"
          className="w-[18rem] rounded-lg border border-glass-stroke p-2"
        >
          {inline}
        </div>
      )}
      {caption !== undefined && (
        <p className="text-xs text-muted-foreground">{caption}</p>
      )}
    </div>
  )
}

const ROW_DEVICES: PickerItem[] = [
  { value: `mac`, label: `MacBook Pro` },
  { value: `server`, label: `Build server` },
]
const ROW_ACCOUNTS: PickerItem[] = [
  { value: `claude`, label: `danny@example.com` },
  { value: `codex`, label: `ops@example.com` },
]
const ROW_MODELS: PickerItem[] = [
  { value: `opus`, label: `Opus` },
  { value: `sonnet`, label: `Sonnet` },
  { value: `haiku`, label: `Haiku` },
]
const ROW_REPOS: PickerItem[] = [
  { value: `web`, label: `niach/exponential` },
  { value: `site`, label: `niach/marketing` },
]

const noop = (): void => {}

/** One start-session options row (Device · Account · Model · Plan ·
 *  Repository) as a dialog draws it; `openModel` discloses the Model pin. */
function OptionsRow({ openModel }: { openModel: boolean }) {
  const pin = (title: string, items: PickerItem[], value: string, open?: boolean) => (
    <Picker
      mode="single"
      items={items}
      value={value}
      onChange={noop}
      mobileTitle={title}
      triggerVariant="inline"
      presentation="inline"
      {...(open === undefined ? {} : { open, onOpenChange: noop })}
    />
  )
  return (
    <ComposerOptionsRow className="px-0">
      {pin(`Device`, ROW_DEVICES, `mac`)}
      {pin(`Account`, ROW_ACCOUNTS, `claude`)}
      {pin(`Model`, ROW_MODELS, `opus`, openModel ? true : undefined)}
      <Label className="cursor-pointer gap-1.5 font-normal">
        <span>Plan</span>
        <Switch size="sm" checked={false} onCheckedChange={noop} aria-label="Plan mode" />
      </Label>
      {pin(`Repository`, ROW_REPOS, `web`)}
    </ComposerOptionsRow>
  )
}

/**
 * N-14 (picker-presentation.json): THE composer options row in a dialog.
 * Collapsed, every pin is its bare trigger; opened, its rows disclose into
 * the ONE slot under the whole row, as wide as the row, so no pin moves.
 */
export function OptionsRowSpecimen() {
  return (
    <div className="grid gap-3">
      {[false, true].map((openModel) => (
        <div
          key={String(openModel)}
          data-testid={openModel ? `picker-row-specimen-open` : `picker-row-specimen-collapsed`}
          className="w-[30rem] max-w-full rounded-xl border border-glass-stroke bg-popover p-3"
        >
          <div className="mb-2 rounded-lg border border-glass-stroke px-3 py-2 text-sm text-muted-foreground">
            What should the agent do?
          </div>
          <OptionsRow openModel={openModel} />
          {/* A static page renders no portal: the slot's disclosed rows are
              drawn here in the slot's own frame. */}
          {openModel && (
            <div className="mt-1 max-h-72 overflow-hidden border-t border-border/50 pt-1 text-sm text-foreground">
              <PickerList mode="single" items={ROW_MODELS} value="opus" onChange={noop} />
            </div>
          )}
        </div>
      ))}
    </div>
  )
}
