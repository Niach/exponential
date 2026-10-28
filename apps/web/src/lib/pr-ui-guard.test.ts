import { describe, expect, it } from "vitest"
import {
  missingPicturesMessage,
  missingPlatformPictures,
  uiPlatformsOfPaths,
} from "./pr-ui-guard"

// EXP-1315: pr_open's UI picture guard, the pure half.
describe(`uiPlatformsOfPaths`, () => {
  it(`maps every UI root onto its platform, in platform order`, () => {
    expect(
      uiPlatformsOfPaths([
        `apps/desktop/crates/exponential-ui-gpui/src/lib.rs`,
        `apps/android/app/src/main/kotlin/A.kt`,
        `packages/exponential-ui-react/src/button.tsx`,
        `apps/ios/Sources/A.swift`,
      ])
    ).toEqual([`web`, `ios`, `android`, `desktop`])
    expect(uiPlatformsOfPaths([`packages/ui/src/menu.tsx`])).toEqual([`web`])
    expect(uiPlatformsOfPaths([`apps/styleguide/src/entries.tsx`])).toEqual([`web`])
    expect(uiPlatformsOfPaths([`apps/desktop/crates/ui/src/sidebar.rs`])).toEqual([`desktop`])
  })

  it(`server, contract and engine paths touch no UI`, () => {
    expect(
      uiPlatformsOfPaths([
        `apps/web/server-bun.ts`,
        `apps/desktop/crates/engine/src/session.rs`,
        `packages/domain-contract/fixtures/session-thread.json`,
        `apps/desktop/crates/ui-extra/src/a.rs`,
        `CLAUDE.md`,
      ])
    ).toEqual([])
  })
})

describe(`missingPlatformPictures`, () => {
  const picture = (label: string) => ({ label, attachmentId: `a-${label}` })

  it(`a picture labelled with the platform word covers it; web-mobile counts for web`, () => {
    expect(
      missingPlatformPictures([`web`, `ios`], [picture(`web-mobile thread`), picture(`iOS thread`)], null)
    ).toEqual([])
  })

  it(`a text entry, an unlabelled one or a mere prefix never counts`, () => {
    expect(
      missingPlatformPictures(
        [`web`, `ios`, `desktop`],
        [{ label: null, attachmentId: null }, picture(`website hero`), picture(`the ios thread`)],
        null
      )
    ).toEqual([`web`, `ios`, `desktop`])
  })

  it(`notRun naming a platform covers it`, () => {
    expect(
      missingPlatformPictures([`android`, `desktop`], null, `Android: the share sheet; desktop: settings`)
    ).toEqual([])
    expect(missingPlatformPictures([`android`], null, `androidx bump`)).toEqual([`android`])
  })

  it(`the Guide's Not run topic counts like the argument`, () => {
    const notRun = (text: string) => ({ topic: `Not run`, label: null, attachmentId: null, text })
    expect(missingPlatformPictures([`ios`, `desktop`], [notRun(`ios: the share sheet`)], null)).toEqual([
      `desktop`,
    ])
    expect(
      missingPlatformPictures([`ios`], [{ topic: `nav`, label: null, attachmentId: null, text: `ios` }], null)
    ).toEqual([`ios`])
  })

  it(`the refusal names the platforms and the next step`, () => {
    const message = missingPicturesMessage([`ios`, `android`])
    expect(message).toMatch(/ios, android/)
    expect(message).toMatch(/exponential_show \{surface, pictures: \[\{label: '<platform> …'\}\]\}/)
    expect(message).toMatch(/notRun/)
  })
})
