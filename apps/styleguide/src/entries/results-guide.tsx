import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { SessionResultsView, type SessionResultGroup } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1154: the Results face as the GUIDE (Linear's shape) — the REAL
// `SessionResultsView` over static groups: the `Summary` topic leads as a
// plain paragraph, every other topic is the shared group band with a muted
// `01 / 03` caption, its text, the files it touched (hairline rows: the
// dimmed-directory path + `+N −M` off the loaded diff; an unknown path has no
// counts) and its tiles.

const SHOT = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900"><rect width="1440" height="900" fill="#3f3f46"/></svg>`
)}`

const FILES: DiffFile[] = [
  { filename: `feature/topic/impl/src/main/kotlin/TopicScreen.kt`, additions: 58, deletions: 6 },
  { filename: `feature/topic/api/src/main/res/values/strings.xml`, additions: 2, deletions: 0 },
  { filename: `feature/topic/impl/src/androidTest/kotlin/TopicScreenTest.kt`, additions: 22, deletions: 0 },
].map((file) => fromPullFile({ ...file, status: `modified` }))

const shot = (topic: string, label: string, attachmentId: string) => ({
  topic,
  label,
  attachmentId,
  width: 1440,
  height: 900,
  inline: false,
  caption: null,
})

const GROUPS: SessionResultGroup[] = [
  {
    topic: `Summary`,
    text: `The topic screen no longer crashes on a load failure: the error state renders a message and a retry, and a compose test covers it. Fixes #APP-13.`,
    entries: [],
    earlier: [],
    files: [],
  },
  {
    topic: `Error state`,
    text: `TopicScreen renders ErrorState for TopicUiState.Error with the new string and a Retry that re-requests the topic.`,
    entries: [shot(`Error state`, `web`, `g1`)],
    earlier: [],
    files: [
      `feature/topic/impl/src/main/kotlin/TopicScreen.kt`,
      `feature/topic/api/src/main/res/values/strings.xml`,
    ],
  },
  {
    topic: `Compose test`,
    text: `TopicScreenTest asserts the message and the retry button on the error path.`,
    entries: [],
    earlier: [],
    files: [`feature/topic/impl/src/androidTest/kotlin/TopicScreenTest.kt`],
  },
  {
    topic: `News loading`,
    text: `The news section reuses the same ErrorState, so both failures read alike.`,
    entries: [],
    earlier: [],
    // Not in the loaded diff: the row draws without counts.
    files: [`feature/topic/impl/src/main/kotlin/NewsFeed.kt`],
  },
]

export const entry: StyleguideEntry = {
  id: `results-guide`,
  section: `special`,
  owner: `EXP-1154`,
  title: `Results guide`,
  blurb: `The Results face as the Guide, one shape ×4 (fixture \`session-results.json\` \`files\` + \`guide\`). The \`Summary\` topic leads as a plain paragraph at the reading width, no band, no number; every other topic is the group band with a muted tabular \`01 / 04\` caption in its leading slot (never uppercase), its 2 or 3 sentences, then the FILES it touched as flat hairline rows (the dimmed-directory path, \`+N −M\` when the loaded diff has the exact path; a tap opens the Changes face on that file), then its tiles and the \`Earlier\` fold. The same report is the pull request's body on GitHub (text only). An issue with an open PR and no report shows the GitHub PR body as one unnumbered band.`,
  status: {
    web: {
      state: `ok`,
      symbol: `SessionResultsView / GuideSectionHeader / GuideFileList`,
      file: `packages/ui/src/session-results-view.tsx`,
      note: `pure rules guideSections / guideSectionCaption / guideFileRows in session-results.ts`,
    },
    desktop: {
      state: `ok`,
      symbol: `session_results::render`,
      file: `apps/desktop/crates/ui/src/session_results.rs`,
      note: `rules in crates/domain/src/session_results.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `SessionResultsFace`,
      file: `apps/ios/Exponential/UI/Work/SessionResultsFace.swift`,
      note: `rules in ExpCore/Sources/Domain/SessionResults.swift`,
    },
    android: {
      state: `ok`,
      symbol: `ResultsFace`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/ResultsFace.kt`,
      note: `rules in domain/SessionResults.kt`,
    },
  },
  island: () => (
    <SessionResultsView
      groups={GROUPS}
      files={FILES}
      onOpenFile={() => {}}
      attachmentSrc={() => SHOT}
    />
  ),
}
