import chats from "@exp/domain-contract/fixtures/chats.json"
import { PhonePageHeaderSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1384 (`chats.json` `shell.header`): THE phone page header, `@exp/ui`
// `PhonePageHeaderSpecimen` (the inbox dot on and off, a pushed page's back
// control); the app's `?dev=phone-page-header` capture renders the same.

const HEADER = chats.shell.header

export const entry: StyleguideEntry = {
  id: `phone-page-header`,
  section: `special`,
  owner: `EXP-1384`,
  title: `Page header`,
  blurb: `THE phone page header ×3 on every top-level page (${HEADER.pages.join(`, `)}): the large title on the left (the Board page's = the board's name, the Team page's = the team's name) and the round trailing circles, SEARCH (opens the phone search) then the INBOX (a sky dot while any notification of the active team is unread; pushes the inbox-only page). ${HEADER.circle}px FAB-chrome circles on Android and web-mobile; iOS paints the large navigation title with the native toolbar circles. A pushed page (Devices from the Team page) puts the back control before the title.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PageHeader`,
      file: `packages/ui/src/page-header.tsx`,
      note: `mounted by apps/web components/team/mobile-page-header.tsx (md:hidden)`,
    },
    desktop: {
      state: `ok`,
      symbol: `phone_page_header`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/phone_page_header.rs`,
      note: `the IDE has no phone shell; the styleguide twin paints the same header`,
    },
    ios: {
      state: `ok`,
      symbol: `pageHeader`,
      file: `apps/ios/Exponential/UI/Navigation/PageHeader.swift`,
    },
    android: {
      state: `ok`,
      symbol: `PageHeader`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/PageHeader.kt`,
    },
  },
  island: () => <PhonePageHeaderSpecimen />,
}
