import type { ReactNode } from "react"

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
  caption,
}: {
  trigger: ReactNode
  surface: ReactNode
  caption?: string
}) {
  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-center gap-2">{trigger}</div>
      <div className="w-[18rem] overflow-hidden rounded-lg border border-glass-stroke bg-popover">
        {surface}
      </div>
      {caption !== undefined && (
        <p className="text-xs text-muted-foreground">{caption}</p>
      )}
    </div>
  )
}
