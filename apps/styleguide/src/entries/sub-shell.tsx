import { GlassGroup, GlassRow, GlassToggleRow, SubShell, SubShellHost, conceptIcon } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1020 fills this entry (EXP-1029 pre-registered it). The demo is the
// REAL `@exp/ui` component, so it cannot disagree with the product.

const noop = () => {}
const WorkflowIcon = conceptIcon(`nav-workflows`)

export const entry: StyleguideEntry = {
  id: `sub-shell`,
  section: `general`,
  owner: `EXP-1020`,
  title: `Sub-shell navigation`,
  blurb: `A settings row that slides a child page in place of the whole card, back button on top. Not a nested card and not a dialog on top of a dialog: the card's groups are replaced, the page is the SAME shell (its own groups of rows), and a sub-shell inside a page slides one level deeper. Back — the button, Escape, or a swipe right on a phone — returns exactly one level. The card behind stays mounted, so an open page keeps seeing live values.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SubShell / SubShellHost`,
      file: `packages/ui/src/sub-shell.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `sub_shell::SubShellHost / sub_shell_row`,
      file: `apps/desktop/crates/ui/src/sub_shell.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SubShell / SubShellHost`,
      file: `apps/ios/ExpUI/Sources/SubShell.swift`,
    },
    android: {
      state: `ok`,
      symbol: `SubShell / SubShellHost`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/SubShell.kt`,
    },
  },
  island: () => (
    <SubShellHost className="w-[22rem]">
      <GlassGroup>
        <GlassToggleRow
          id="demo-sub-shell-default"
          label="Default device"
          checked
          onCheckedChange={noop}
        />
        <SubShell label="Workflow settings" icon={WorkflowIcon} value="opus · fable">
          <GlassGroup>
            <GlassRow>Model</GlassRow>
            <GlassRow>Strong model</GlassRow>
          </GlassGroup>
        </SubShell>
      </GlassGroup>
    </SubShellHost>
  ),
}
