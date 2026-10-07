import { useState } from "react"
import { ScopePicker, type ScopePickerTeam, type ScopeSelection } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// FEED-76: the team/board scope picker the MCP consent screen (EXP-792) and
// the Create-API-key dialog share. The island is the real control over a
// demo tree with the "Everything" switch already off, so the tree shows.

const TREE: ScopePickerTeam[] = [
  {
    id: `t-acme`,
    name: `Acme`,
    boards: [
      { id: `b-web`, name: `Web`, prefix: `WEB` },
      { id: `b-mob`, name: `Mobile`, prefix: `MOB` },
    ],
  },
  { id: `t-lab`, name: `Lab`, boards: [{ id: `b-rnd`, name: `Research`, prefix: `RND` }] },
]

function ScopePickerDemo() {
  const [value, setValue] = useState<ScopeSelection>({
    allTeams: false,
    teamIds: [`t-acme`],
    boardIds: [`b-rnd`],
  })
  return <ScopePicker tree={TREE} value={value} onChange={setValue} idPrefix="demo-scope" />
}

export const entry: StyleguideEntry = {
  id: `scope-picker`,
  section: `general`,
  owner: `FEED-76`,
  title: `Scope picker`,
  blurb: `What a credential may touch: an "Everything" switch (all teams and boards, including ones created later) over the member's teams, each a whole-team checkbox with its boards indented beneath — a board under a ticked team reads checked and disabled because the team covers it. The MCP OAuth consent screen and the Create-API-key dialog show the same control; a scoped API key is MCP-only and the key row wears the pick as "Scoped to Acme, Web (WEB)".`,
  status: {
    web: {
      state: `ok`,
      symbol: `ScopePicker`,
      file: `packages/ui/src/scope-picker.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `ScopePicker`,
      file: `apps/desktop/crates/ui/src/scope_picker.rs`,
      note: `The Create-key alert's Access block; the key row carries the scope caption (users.rs scope_caption).`,
    },
    ios: {
      state: `n/a`,
      note: `No key or consent UI on iOS; the consent page is web-only mid-authorize.`,
    },
    android: {
      state: `n/a`,
      note: `No key or consent UI on Android; the consent page is web-only mid-authorize.`,
    },
  },
  island: () => (
    <div className="w-[24rem]">
      <ScopePickerDemo />
    </div>
  ),
}
