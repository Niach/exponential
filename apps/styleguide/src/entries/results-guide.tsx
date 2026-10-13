import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import {
  conceptIcon,
  GlassSectionHeader,
  Pill,
  PrRow,
  SessionResultsView,
  type SessionResultGroup,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1154 + EXP-1251: the GUIDE — the REAL `SessionResultsView` over static
// groups and a loaded diff: the `Summary` topic leads as a plain paragraph,
// every other topic is the shared group band with a muted `01 / 03` caption,
// its text, ONE `Changes · N files · +A −D ›` row (the files it names, by
// path or rename source) and its tiles; the file no topic names lands in the
// unnumbered `Other changes` band and `Show complete diff` closes the page.
// EXP-1293: a thread's Guide tab leads with its pull requests
// (`pull_requests` rows of the root): one PR row each (`#number`, the title,
// the state word), the open one carrying THE merge pill (`Merge stack` while
// it sits in an open stack); the picked PR's files feed the Changes rows.

const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)

const PRS = [
  { key: `p45`, number: 45, title: `Topic screen error state`, state: `open` },
  { key: `p41`, number: 41, title: `Topic repository retries`, state: `merged` },
] as const

const SHOT = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900"><rect width="1440" height="900" fill="#3f3f46"/></svg>`
)}`

const FILES: DiffFile[] = [
  { filename: `feature/topic/impl/src/main/kotlin/TopicScreen.kt`, additions: 58, deletions: 6 },
  { filename: `feature/topic/api/src/main/res/values/strings.xml`, additions: 2, deletions: 0 },
  { filename: `feature/topic/impl/src/androidTest/kotlin/TopicScreenTest.kt`, additions: 22, deletions: 0 },
  // Named by no topic: the Other changes band.
  { filename: `gradle/libs.versions.toml`, additions: 1, deletions: 1 },
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
    // Not in the loaded diff: the section draws no Changes row.
    files: [`feature/topic/impl/src/main/kotlin/NewsFeed.kt`],
  },
]

export const entry: StyleguideEntry = {
  id: `results-guide`,
  section: `special`,
  owner: `EXP-1154`,
  title: `Guide`,
  blurb: `The Guide face (Changes + Results merged, EXP-1251), one shape ×4 (fixture \`session-results.json\` \`files\`, \`guide\` + \`coverage\`). A thread's Guide tab leads with its pull requests (the root's \`pull_requests\` rows, open first: \`#number\`, the title, the state word, the GitHub link, THE merge pill on an open one reading \`Merge stack\` while it sits in an open stack, Close in its menu); the picked PR's files (\`pullRequests.files\`) feed the Changes rows. The \`Summary\` topic leads the report as a plain paragraph at the reading width, no band, no number; every other topic is the group band with a muted tabular \`01 / 04\` caption in its leading slot (never uppercase), its 2 or 3 sentences, then ONE \`Changes\` row (the \`guide-changes\` glyph, \`N files\` muted, \`+A −D\`, a chevron) over the diff files the topic names by path or rename source; a tap opens that section's diff page. Every diff file no topic names lands in a trailing unnumbered \`Other changes\` band (no report at all: one \`Changes\` band holds the whole diff), and a final hairline \`Show complete diff\` row opens everything. The same Guide is the pull request's body on GitHub (text only). The Guide is the ROOT's (\`sessions.guide\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadGuide: PR rows + GuideBody over SessionResultsView`,
      file: `apps/web/src/components/thread/thread-guide.tsx`,
      note: `section page + guideSectionPage: guide-section-diff.tsx; merge: session-merge-button.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `session_results::render`,
      file: `apps/desktop/crates/ui/src/session_results.rs`,
      note: `rules in crates/domain/src/session_results.rs; section page = GuidePageView over the shared diff pane`,
    },
    ios: {
      state: `ok`,
      symbol: `GuideFace / GuideSectionDiffView`,
      file: `apps/ios/Exponential/UI/Work/GuideFace.swift`,
      note: `rules in ExpCore/Sources/Domain/SessionResults.swift; tiles in SessionResultTiles.swift`,
    },
    android: {
      state: `ok`,
      symbol: `GuideFace`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/GuideFace.kt`,
      note: `rules in domain/SessionResults.kt; section page = GuideSectionDiff.kt`,
    },
  },
  island: () => (
    <div className="flex flex-col">
      <div className="flex flex-col px-7 pt-5 md:px-5">
        <GlassSectionHeader
          label="Pull requests"
          leading={<PrOpenIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />}
        />
        {PRS.map((pr, index) => (
          <PrRow
            key={pr.key}
            node={pr.state === `open` ? `open` : `base`}
            identifier={`#${pr.number}`}
            title={pr.title}
            word={pr.state === `open` ? null : pr.state}
            active={index === 0}
            onClick={() => {}}
            trailing={
              pr.state === `open` ? (
                <Pill size="md" mode="action" primary className="shrink-0">
                  <PrMergedIcon />
                  Merge PR
                </Pill>
              ) : null
            }
          />
        ))}
      </div>
      <SessionResultsView
        groups={GROUPS}
        files={FILES}
        onOpenChanges={() => {}}
        attachmentSrc={() => SHOT}
      />
    </div>
  ),
}
