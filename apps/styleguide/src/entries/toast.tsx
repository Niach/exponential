import type { ReactNode } from "react"
import { TOAST_CONSTANTS, ToastSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1031 fills this entry (EXP-1029 pre-registered it). A live sonner
// `<Toaster>` renders nothing to static markup, so the island is
// `ToastSpecimen`: plain elements wearing the SAME class constants the live
// toast passes to sonner, the stack placed by the fixture's geometry cases.

const c = TOAST_CONSTANTS

function Captioned({ caption, children }: { caption: string; children: ReactNode }) {
  return (
    <div className="grid content-start gap-2">
      {children}
      <p className="text-xs text-muted-foreground">{caption}</p>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `toast`,
  section: `general`,
  owner: `EXP-1031`,
  title: `Toast`,
  blurb: `The one transient notice, identical on all four clients (packages/domain-contract/fixtures/toast-stack.json, sonner 2.0.7's numbers). One sentence, an optional description, an optional action. The kind (success, error, info, warning) colours the ICON only; the text stays foreground. The card is the opaque glass card fill with the card hairline at radius lg, no shadow, ${c.width} wide on a pointer and full width minus ${c.mobileViewportOffset} on phones; bottom-right on web and the IDE, bottom-centre above the tab bar on phones, ${c.viewportOffset} from the edges. Newest in front, older ones peek ${c.peek} behind and shrink ${c.scaleStep * 100}% per rank, ${c.visible} visible. Hover or tap expands the stack ${c.gap} apart and pauses the ${c.durationMs / 1000} s clock. Dismiss = the close glyph, a swipe past ${c.swipeThreshold}, or the action. On iOS and Android the stack draws in its own see-through window, so it clears sheets and dialogs. Persistent states stay banners, confirmations stay dialogs.`,
  status: {
    web: {
      state: `ok`,
      symbol: `TOAST_CLASS`,
      file: `packages/ui/src/toast.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `toast::show`,
      file: `apps/desktop/crates/ui/src/toast.rs`,
      note: `its own ToastLayer per window; gpui_base::ToastManager for the lifecycle, theme::motion for every transition`,
    },
    ios: {
      state: `ok`,
      symbol: `ToastHost`,
      file: `apps/ios/ExpUI/Sources/Toast.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ToastHost`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/Toast.kt`,
    },
  },
  island: () => (
    <div className="grid gap-8">
      <div className="flex flex-wrap items-start gap-6">
        <Captioned caption={`collapsed · ${c.visible} visible, ${c.peek} peek, −${c.scaleStep * 100}% per rank`}>
          <ToastSpecimen stack="collapsed" />
        </Captioned>
        <Captioned caption={`expanded on hover · ${c.gap} apart, clock paused`}>
          <ToastSpecimen stack="expanded" />
        </Captioned>
      </div>
      <div className="flex flex-wrap items-start gap-6">
        <Captioned caption="success">
          <ToastSpecimen kind="success" title="Issue EXP-1031 created" />
        </Captioned>
        <Captioned caption="error">
          <ToastSpecimen kind="error" title="Could not merge the pull request" description="The branch has conflicts with master." />
        </Captioned>
        <Captioned caption="info">
          <ToastSpecimen kind="info" title="Session started on studio-mac" />
        </Captioned>
        <Captioned caption="warning">
          <ToastSpecimen kind="warning" title="Storage is almost full" description="240 MB of 250 MB used." />
        </Captioned>
        <Captioned caption="with an action">
          <ToastSpecimen kind="error" title="studio-mac is offline" description="Start the run on another device?" action="Choose device" />
        </Captioned>
      </div>
    </div>
  ),
}
