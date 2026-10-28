import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import {
  conceptIcon,
  GlassSectionHeader,
  Pill,
  SessionResultsView,
  StackRail,
  type SessionResultGroup,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1154 + EXP-1251: the GUIDE — the REAL `SessionResultsView` over static
// groups and a loaded diff: the `Summary` topic leads as a plain paragraph,
// every other topic is the shared group band with a muted `01 / 03` caption,
// its text, ONE `Changes · N files · +A −D ›` row (the files it names, by
// path or rename source) and its tiles; the file no topic names lands in the
// unnumbered `Other changes` band and `Show complete diff` closes the page.
// EXP-1306: a thread's Guide speaks for ONE pull request (`guidePullRequest`,
// session-thread.json ×4): in an open stack the `Stack` band + THE StackRail
// lead it (that PR = the current wash), and THE white bottom Merge capsule
// closes it (`Merge stack` on an open-stack member). No PR rows.

const PrStackIcon = conceptIcon(`pr-stack`)
const PrMergedIcon = conceptIcon(`pr-merged`)

const STACK = [
  { key: `app-14`, identifier: `APP-14`, title: `Group board issues by assignee` },
  { key: `app-13`, identifier: `APP-13`, title: `Topic screen error state`, current: true },
]

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
  blurb: `The Guide face (Changes + Results merged, EXP-1251), one shape ×4 (fixture \`session-results.json\` \`files\`, \`guide\` + \`coverage\`). A thread's Guide speaks for ONE pull request (\`guidePullRequest\`: the newest open, else the newest merged; fixture \`session-thread.json\` ×4): when it sits in a linear open stack the \`Stack\` band + THE StackRail lead the Guide (top first, that PR = the current wash, down to its base branch; a row opens that PR's thread, a hovered row offers Merge through here), and THE white bottom Merge capsule closes the tab while it is open (\`Merge stack\` on an open-stack member). No pull-request rows: Close PR lives in the thread header's \`…\` (\`guideMenuEntries\`, EXP-1307; no GitHub entry, only the agent opens PRs, EXP-1338). Its files (\`pullRequests.files\`) feed the Changes rows. The \`Summary\` topic leads the report as a plain paragraph at the reading width, no band, no number; every other topic is the group band with a muted tabular \`01 / 04\` caption in its leading slot (never uppercase), its 2 or 3 sentences, then ONE \`Changes\` row (the \`guide-changes\` glyph, \`N files\` muted, \`+A −D\`, a chevron) over the diff files the topic names by path or rename source; a tap opens that section's diff page. Every diff file no topic names lands in a trailing unnumbered \`Other changes\` band (no report at all: one \`Changes\` band holds the whole diff), and a final hairline \`Show complete diff\` row opens everything. The same Guide is the pull request's body on GitHub (text only). The Guide is the ROOT's (\`sessions.guide\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadGuide: Stack card + GuideBody over SessionResultsView, ThreadGuideMerge, ThreadGuideMenu`,
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
      state: `n/a`,
      note: `S-3 retired the Guide face (results are stories now)`,
    },
    android: {
      state: `n/a`,
      note: `S-3 retired the Guide face (results are stories now)`,
    },
  },
  island: () => (
    <div className="flex flex-col">
      <div className="flex flex-col px-7 pt-5 md:px-5">
        <GlassSectionHeader
          label="Stack"
          leading={<PrStackIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />}
        />
        <StackRail members={STACK} baseBranch="main" onOpen={() => {}} onMergeThrough={() => {}} />
      </div>
      <SessionResultsView
        groups={GROUPS}
        files={FILES}
        onOpenChanges={() => {}}
        attachmentSrc={() => SHOT}
      />
      <div className="flex justify-center pt-2 pb-4">
        <Pill size="md" mode="action" primary className="shrink-0">
          <PrMergedIcon />
          Merge stack
        </Pill>
      </div>
    </div>
  ),
}
