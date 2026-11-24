import { readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import { SEARCH_LIST_TRANSLATE } from "@exp/domain-contract/search-list"
import {
  ISSUE_SEARCH_DEFAULT_LIMIT,
  ISSUE_SEARCH_EMPTY_DETAIL,
  ISSUE_SEARCH_EMPTY_HINT,
  ISSUE_SEARCH_NO_RESULTS,
  ISSUE_SEARCH_PLACEHOLDER,
} from "./issue-search"

// EXP-922: "one consistent search across all platforms". The RANKING is locked
// by the shared fixture (issue-search.test.ts and its three mirrors); this
// locks what a fixture cannot reach — the words the four search surfaces
// print, the number of rows they ask for, and the fact that the IDE's palette
// searches issues and nothing else.
//
// Source greps, like board-copy.test.ts: the natives cannot import a TS
// constant, so the constant is the reference and this proves each surface
// still spells it.
const REPO = join(import.meta.dirname, `../../../..`)
const read = (path: string) => readFileSync(join(REPO, path), `utf8`)

const WEB_SHEET = `apps/web/src/components/issue-search-sheet.tsx`
const DESKTOP_PALETTE = `apps/desktop/crates/ui/src/search_sheet.rs`
// EXP-1259 D4: the palette's rows come from THE search list (also the `#`
// typeahead and the "+" submenus), which owns the engine calls.
const DESKTOP_SEARCH_LIST = `apps/desktop/crates/ui/src/picker/search_list.rs`
const IOS_SEARCH = `apps/ios/Exponential/UI/Search/SearchView.swift`
const ANDROID_SEARCH = `apps/android/app/src/main/java/com/exponential/app/ui/search/SearchScreen.kt`

// The web sheet renders the constants by name; the three natives spell them.
const NATIVE_SURFACES = [DESKTOP_PALETTE, IOS_SEARCH, ANDROID_SEARCH]

describe(`the four search surfaces print the same copy`, () => {
  for (const [name, copy] of [
    [`placeholder`, ISSUE_SEARCH_PLACEHOLDER],
    [`empty-query hint`, ISSUE_SEARCH_EMPTY_HINT],
    [`empty-query detail`, ISSUE_SEARCH_EMPTY_DETAIL],
    [`no results`, ISSUE_SEARCH_NO_RESULTS],
  ] as const) {
    it(`every native surface spells the ${name}`, () => {
      for (const surface of NATIVE_SURFACES) {
        expect(read(surface), `${surface} is missing the ${name}`).toContain(
          `"${copy}"`
        )
      }
    })
  }

  it(`the web sheet renders the constants rather than its own strings`, () => {
    const source = read(WEB_SHEET)
    for (const constant of [
      `ISSUE_SEARCH_PLACEHOLDER`,
      `ISSUE_SEARCH_EMPTY_HINT`,
      `ISSUE_SEARCH_EMPTY_DETAIL`,
      `ISSUE_SEARCH_NO_RESULTS`,
    ]) {
      expect(source).toContain(constant)
    }
    // The strings this surface used to own, before the four agreed.
    expect(source).not.toContain(`Type to search issues`)
    expect(source).not.toContain(`Search issues...`)
  })
})

// #NEW-136: `Translate to a search` exists where a model runs on the user's
// own device or login: the desktop app and iOS. Web and Android have none,
// so they have no row (search-list.json TRANSLATE).
describe(`the model fallback row lives on desktop and iOS only`, () => {
  const IOS_VIEW_MODEL = IOS_SEARCH.replace(`SearchView`, `SearchViewModel`)

  it(`the fixture names the two platforms`, () => {
    expect(SEARCH_LIST_TRANSLATE.platforms).toEqual([`desktop`, `ios`])
    expect(SEARCH_LIST_TRANSLATE.label).toBe(`Translate to a search`)
    expect(SEARCH_LIST_TRANSLATE.failed).toBe(`Couldn't translate this search`)
  })

  it(`desktop and iOS spell the row and the toast`, () => {
    for (const surface of [DESKTOP_PALETTE, IOS_SEARCH]) {
      const source = read(surface)
      expect(source, `${surface} is missing the row`).toContain(`"${SEARCH_LIST_TRANSLATE.label}"`)
      expect(source, `${surface} is missing the toast`).toContain(`"${SEARCH_LIST_TRANSLATE.failed}"`)
    }
  })

  it(`web and Android never offer it`, () => {
    for (const surface of [
      WEB_SHEET,
      `apps/web/src/components/issue-picker-host.tsx`,
      `packages/ui/src/search-list.tsx`,
      ANDROID_SEARCH,
      ANDROID_SEARCH.replace(`SearchScreen`, `SearchViewModel`),
      `apps/android/app/src/main/java/com/exponential/app/ui/components/SearchList.kt`,
    ]) {
      const source = read(surface)
      expect(source, `${surface} offers the model fallback`).not.toContain(SEARCH_LIST_TRANSLATE.label)
      expect(source, `${surface} offers the model fallback`).not.toContain(`SEARCH_LIST_TRANSLATE`)
    }
  })

  it(`typing never calls the model: only a pick does`, () => {
    // Desktop: the one call site sits in the pick handler's `translate`.
    const desktop = read(DESKTOP_PALETTE)
    expect(desktop.split(`search_translate::run(`).length - 1).toBe(1)
    expect(desktop).toContain(`fn translate(&mut self`)
    // iOS: the view model's `translate()` is the one caller of the model.
    const ios = read(IOS_VIEW_MODEL)
    expect(ios.split(`IssueSearchTranslator.translate(`).length - 1).toBe(1)
    expect(ios).toContain(`func translate(done:`)
  })
})

describe(`the four search surfaces ask for the same number of rows`, () => {
  it(`nobody hard-codes a result limit of its own`, () => {
    // Web + desktop take the shared constant by name.
    expect(read(WEB_SHEET)).toContain(`limit: ISSUE_SEARCH_DEFAULT_LIMIT`)
    // The IDE's search list ranks locally and asks the server with it.
    const searchList = read(DESKTOP_SEARCH_LIST)
    expect(searchList).toContain(
      `domain::issue_search::rank(&rows, query, domain::issue_search::DEFAULT_LIMIT`
    )
    expect(searchList).toContain(`domain::issue_search::DEFAULT_LIMIT as u32`)
    expect(searchList).not.toMatch(/MAX_RESULTS|const [A-Z_]*LIMIT: usize = \d/)
    // The natives take their own engine's copy of it.
    expect(read(IOS_SEARCH.replace(`SearchView`, `SearchViewModel`))).toContain(
      `resultLimit = IssueSearch.defaultLimit`
    )
    expect(
      read(ANDROID_SEARCH.replace(`SearchScreen`, `SearchViewModel`))
    ).toContain(`MAX_RESULTS = IssueSearch.DEFAULT_LIMIT`)
  })

  it(`the shared limit is the one value`, () => {
    expect(ISSUE_SEARCH_DEFAULT_LIMIT).toBe(30)
    // The mirrors hard-code the same number (they cannot import it).
    expect(read(`apps/desktop/crates/domain/src/issue_search.rs`)).toContain(
      `pub const DEFAULT_LIMIT: usize = 30;`
    )
    expect(read(`apps/ios/ExpCore/Sources/Domain/IssueSearch.swift`)).toContain(
      `defaultLimit = 30`
    )
    expect(
      read(`apps/android/app/src/main/java/com/exponential/app/domain/IssueSearch.kt`)
    ).toContain(`DEFAULT_LIMIT = 30`)
  })
})

describe(`the four search surfaces list one flat set of rows`, () => {
  it(`no surface bands its results under board headers`, () => {
    // EXP-922: the natives used to group by board, which fought the ranking —
    // a board's band appeared wherever its best hit landed and dragged its
    // weaker hits up with it, so the same query read differently per client.
    for (const surface of [IOS_SEARCH, ANDROID_SEARCH]) {
      const source = read(surface)
      expect(source, `${surface} still draws a board header`).not.toContain(
        `boardHeader`
      )
      expect(source, `${surface} still draws a board header`).not.toContain(
        `BoardHeader`
      )
    }
  })
})

// "the ide shouldnt search through files anymore" (EXP-922). The repo file
// finder and its `git grep` content pass came out with EXP-892; this keeps
// them out. The Files tool owns the repo — the palette owns issues.
describe(`the IDE's search palette searches issues and nothing else`, () => {
  const source = read(DESKTOP_PALETTE)

  it(`holds issue hits alone`, () => {
    // One list, one kind of row: a file/symbol pass would need its own.
    expect(source).toContain(`hits: Vec<IssueHit>,`)
    expect(source).toContain(`search_list::local_issue_hits_teams(`)
    expect(source).not.toMatch(/file_hits|path_hits|symbol_hits|Vec<SearchHit>/)
  })

  it(`never walks the working tree or shells out to git`, () => {
    for (const forbidden of [
      `git grep`,
      `std::fs::read_dir`,
      `walkdir`,
      `Command::new`,
      `ripgrep`,
    ]) {
      // `git grep` appears once, in the module doc that records the removal.
      const hits = source.split(forbidden).length - 1
      const allowed = forbidden === `git grep` ? 1 : 0
      expect(hits, `${forbidden} in the search palette`).toBe(allowed)
    }
  })
})
