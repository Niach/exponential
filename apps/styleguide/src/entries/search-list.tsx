import type { ReactNode } from "react"
import searchList from "@exp/domain-contract/fixtures/search-list.json"
import {
  Button,
  SearchList,
  conceptIcon,
  getActionIcon,
  issueSearchItems,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// SLOP-28 (EXP-1259 D4/D5, `search-list.json`): THE search list, the real
// `@exp/ui` `SearchList` three times: the `#` typeahead's popover (its
// `popoverWidth`, prefilled with the token after the `#`, more than eight
// hits so the list scrolls under the bottom fade), the issue picker (multi:
// the picks pinned first with their check, the Implement button under the
// rows) and the phone's bottom sheet (the 44px rows, the input focused). The
// live popover hangs off the caret through `SearchListPopover`; a portal, so
// the specimens draw the surface in place.

const noop = (): void => {}

const BACKLOG = { icon: conceptIcon(`status-backlog`), color: `text-muted-foreground` }
const STARTED = { icon: conceptIcon(`status-in-progress`), color: `text-yellow-500` }
const REVIEW = { icon: conceptIcon(`status-in-review`), color: `text-blue-500` }

const ISSUES = [
  { id: `i1`, identifier: `EXP-1259`, title: `UX batch 2: home, tabs, search, getting started`, ...STARTED },
  { id: `i2`, identifier: `EXP-1255`, title: `Tab close on hover, a New session tab`, ...REVIEW },
  { id: `i3`, identifier: `EXP-1253`, title: `The Agent page is home`, ...STARTED },
  { id: `i4`, identifier: `EXP-1249`, title: `One "+" menu in the composer`, ...BACKLOG },
  { id: `i5`, identifier: `EXP-1243`, title: `Motion pass over every component`, ...BACKLOG },
  { id: `i6`, identifier: `EXP-1241`, title: `A comment call renders the real comment`, ...REVIEW },
  { id: `i7`, identifier: `EXP-1240`, title: `One Work card for workflows and task lists`, ...STARTED },
  { id: `i8`, identifier: `SLOP-28`, title: `Side panel search for # references`, ...BACKLOG },
  { id: `i9`, identifier: `EXP-1270`, title: `Getting started as one row of cards`, ...BACKLOG },
  { id: `i10`, identifier: `EXP-1271`, title: `Remote starts never take the window`, ...BACKLOG },
]

const AGENTS = [
  { id: `g1`, name: `Release manager`, icon: `rocket` },
  { id: `g2`, name: `Docs writer`, icon: `megaphone` },
  { id: `g3`, name: `Triage`, icon: `wrench` },
  { id: `g4`, name: `Merge conflicts`, icon: `git-branch` },
]

/** An agent row = its icon and its name. */
const AGENT_ITEMS = AGENTS.map((agent) => ({
  value: agent.id,
  label: agent.name,
  icon: getActionIcon(agent),
}))

function Surface({
  width,
  caption,
  children,
}: {
  width: number
  caption: string
  children: ReactNode
}) {
  return (
    <div className="grid content-start gap-2">
      <div
        className="flex flex-col overflow-hidden rounded-lg border border-glass-stroke bg-popover"
        style={{ width, maxWidth: `100%` }}
      >
        {children}
      </div>
      <p className="text-xs text-muted-foreground">{caption}</p>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `search-list`,
  section: `general`,
  owner: `SLOP-28`,
  title: `Search list`,
  blurb: `ONE component for every place that searches a list and picks from it (fixture \`search-list.json\` ×4): the \`#\` issue typeahead in every editor and composer, the composer "+" menu's Implement issue › and Message an agent › submenus, the issue pickers and the global issue search. An input row (${searchList.input.height}px, the search glyph, the field, the muted count once a query is typed) over the Picker rows at ONE height (${searchList.row.height}px, ${searchList.row.heightPhone} on phones), at most ${searchList.maxVisibleRows} visible, the rest scrolling under a ${searchList.fadeHeight}px bottom fade. The top row is preselected; ↑/↓ move, Enter or Tab pick, a hover selects. Results come from the shared engine at its default limit (no 6/8 cap). Typing \`#\` opens it at the caret (a ${searchList.popoverWidth}px popover; phones: a bottom sheet) with ITS input focused and prefilled with the token after the \`#\`, spaces allowed; a pick writes \`#IDENT \`, Escape returns to the editor leaving the text, Backspace in the empty input deletes the \`#\`.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SearchList / SearchListPopover / typeaheadPrefill`,
      file: `packages/ui/src/search-list.tsx`,
      note: `the # host = apps/web issue-ref-search.tsx; pickers + Cmd+K = issue-picker-host.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `SearchList`,
      file: `apps/desktop/crates/ui/src/picker/search_list.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SearchList`,
      file: `apps/ios/ExpUI/Sources/Picker/SearchList.swift`,
      note: `the # sheet = UI/Search/IssueRefSearchSheet.swift`,
    },
    android: {
      state: `ok`,
      symbol: `SearchList`,
      file: `apps/android/app/src/main/java/com/exponential/app/domain/SearchList.kt`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <Surface
        width={searchList.popoverWidth}
        caption="The # typeahead: prefilled with the token after the #, more than eight hits scroll under the fade."
      >
        <SearchList
          mode="single"
          density="pointer"
          items={issueSearchItems(ISSUES)}
          value={null}
          onChange={noop}
          query="sid"
          shouldFilter={false}
          placeholder="Search issues"
        />
      </Surface>
      <Surface
        width={416}
        caption="Implement issue ›: the picks pinned first with their check, the button under the rows."
      >
        <SearchList
          mode="multi"
          density="pointer"
          items={issueSearchItems(ISSUES.slice(0, 5))}
          value={[`i1`, `i2`]}
          onChange={noop}
          placeholder="Search issues"
          footer={
            <div className="flex justify-end p-2">
              <Button type="button" size="sm">
                Implement 2 issues
              </Button>
            </div>
          }
        />
      </Surface>
      <Surface width={416} caption="Message an agent ›: the agent's icon and its name, nothing else.">
        <SearchList
          mode="single"
          density="pointer"
          items={AGENT_ITEMS}
          value={null}
          onChange={noop}
          placeholder="Search agents"
        />
      </Surface>
      <Surface width={390} caption="A phone: the same list in a bottom sheet, 44px rows.">
        <SearchList
          mode="single"
          density="phone"
          items={issueSearchItems(ISSUES.slice(0, 5))}
          value={null}
          onChange={noop}
          placeholder="Search issues"
        />
      </Surface>
    </div>
  ),
}
