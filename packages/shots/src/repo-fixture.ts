/**
 * EXP-1558: the desktop lane's repository, built from nothing on every run.
 *
 * The repo-backed desktop views (`files`, `source-control`, `terminal`,
 * `start-coding`, `settings-worktrees`) read `<repos_root>/acme/mobile-app`
 * straight off disk. Pointing them at a developer's real clone made every
 * Mac photograph a different tree, history and worktree list (or, with no
 * clone at all, "Repository not cloned yet"), so a run without
 * `--repos-root` now gets this fixture instead: a small Android app whose
 * history names the demo team and the seeded issues, a local bare `origin`
 * (nothing talks to GitHub), the two kept worktrees of the device-doctor
 * fixture's low-disk case (`exp-session-9497089a` with uncommitted changes,
 * `exp-EXP-1375` with an unpushed commit) and two uncommitted edits in the
 * trunk clone (the files the demo session's transcript edits).
 *
 * Commit dates are relative to the run (`now`), so "2 days ago" reads the
 * same in every capture. The base is username-free on purpose: the Worktrees
 * pane and the worktree rows print the paths verbatim.
 */
import { mkdirSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"

/** Where the fixture lives: `<base>/repos` is the repos root, `<base>/origin` its remote. */
export const REPO_FIXTURE_BASE = `/Users/Shared/Exponential/shots`
export const REPO_FIXTURE_OWNER = `acme`
export const REPO_FIXTURE_NAME = `mobile-app`

const HOUR = 60 * 60 * 1000
const DAY = 24 * HOUR

interface Person {
  name: string
  email: string
}

const ALEX: Person = { name: `Alex Carter`, email: `alex@acme.dev` }
const MIRA: Person = { name: `Mira Chen`, email: `mira@acme.dev` }
const JONAS: Person = { name: `Jonas Weber`, email: `jonas@acme.dev` }
const SOFIA: Person = { name: `Sofia Almeida`, email: `sofia@acme.dev` }

const PKG = `app/src/main/java/com/acme/mobile`

/** The tree of the first commit: an Android app's top level, short files. */
const BASE_FILES: Record<string, string> = {
  ".gitignore": `.gradle/\nbuild/\nlocal.properties\n.idea/\n`,
  ".github/workflows/ci.yml": `name: CI\non: [pull_request]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: ./gradlew check\n`,
  "README.md": `# Mobile App\n\nThe Acme issue tracker for Android.\n\n    ./gradlew installDebug\n`,
  "LICENSE": `Copyright 2026 Acme\n`,
  "build.gradle.kts": `plugins {\n    alias(libs.plugins.android.application) apply false\n    alias(libs.plugins.kotlin.android) apply false\n}\n`,
  "settings.gradle.kts": `rootProject.name = "mobile-app"\ninclude(":app", ":core:data", ":core:ui", ":feature:board", ":feature:issue")\n`,
  "gradle.properties": `org.gradle.jvmargs=-Xmx4g\nandroid.useAndroidX=true\n`,
  "gradle/libs.versions.toml": `[versions]\nagp = "8.7.0"\nkotlin = "2.1.0"\n`,
  "gradlew": `#!/bin/sh\nexec gradle "$@"\n`,
  "app/build.gradle.kts": `plugins {\n    id("com.android.application")\n}\n\nandroid {\n    namespace = "com.acme.mobile"\n}\n`,
  "app/src/main/AndroidManifest.xml": `<manifest xmlns:android="http://schemas.android.com/apk/res/android">\n    <application android:label="Mobile App" />\n</manifest>\n`,
  [`${PKG}/MainActivity.kt`]: `package com.acme.mobile\n\nclass MainActivity : ComponentActivity() {\n    override fun onCreate(savedInstanceState: Bundle?) {\n        super.onCreate(savedInstanceState)\n        setContent { App() }\n    }\n}\n`,
  [`${PKG}/ui/board/BoardScreen.kt`]: `package com.acme.mobile.ui.board\n\n@Composable\nfun BoardScreen(state: BoardState) {\n    LazyColumn {\n        state.groups.forEach { group ->\n            item { StatusHeader(group.status, group.issues.size) }\n            items(group.issues) { IssueRow(it) }\n        }\n    }\n}\n`,
  [`${PKG}/data/sync/ShapeSubscriber.kt`]: `package com.acme.mobile.data.sync\n\nclass ShapeSubscriber(private val client: ElectricClient) {\n    fun subscribe(shape: String) = client.stream(shape)\n}\n`,
  "core/data/build.gradle.kts": `plugins {\n    id("com.android.library")\n}\n`,
  "core/ui/build.gradle.kts": `plugins {\n    id("com.android.library")\n}\n`,
  "feature/board/build.gradle.kts": `plugins {\n    id("com.android.library")\n}\n`,
  "feature/issue/build.gradle.kts": `plugins {\n    id("com.android.library")\n}\n`,
  "build-logic/convention/build.gradle.kts": `plugins {\n    \`kotlin-dsl\`\n}\n`,
}

interface Step {
  message: string
  author: Person
  /** How long before the run the commit was made. */
  ago: number
  files: Record<string, string>
}

/** Trunk's history after the first commit, oldest first; `branch` steps land through a merge. */
const HISTORY: Step[] = [
  {
    message: `Board screen with status groups`,
    author: ALEX,
    ago: 8 * DAY,
    files: { [`${PKG}/ui/board/BoardState.kt`]: `package com.acme.mobile.ui.board\n\ndata class BoardState(val groups: List<StatusGroup>)\n` },
  },
  {
    message: `Fix crash when uploading HEIC photos`,
    author: SOFIA,
    ago: 4 * DAY,
    files: { [`${PKG}/data/upload/ImageEncoder.kt`]: `package com.acme.mobile.data.upload\n\nfun encode(bitmap: Bitmap): ByteArray = bitmap.compress(JPEG, 90)\n` },
  },
  {
    message: `Defer shape subscribe until after first frame`,
    author: MIRA,
    ago: 2 * DAY,
    files: { [`${PKG}/data/sync/ShapeSubscriber.kt`]: `package com.acme.mobile.data.sync\n\nclass ShapeSubscriber(private val client: ElectricClient) {\n    fun subscribe(shape: String) = client.stream(shape, after = FirstFrame)\n}\n` },
  },
  {
    message: `Profile app launch end-to-end`,
    author: ALEX,
    ago: 26 * HOUR,
    files: { "benchmarks/build.gradle.kts": `plugins {\n    id("com.android.test")\n}\n` },
  },
]

/** A feature branch merged into trunk between the history's 2nd and 3rd steps. */
const MERGED_BRANCH = {
  name: `offline-queue`,
  pr: 12,
  commits: [
    {
      message: `Offline queue for issue edits`,
      author: JONAS,
      ago: 6 * DAY,
      files: { [`${PKG}/data/sync/EditQueue.kt`]: `package com.acme.mobile.data.sync\n\nclass EditQueue {\n    private val pending = ArrayDeque<Edit>()\n    fun push(edit: Edit) = pending.addLast(edit)\n}\n` },
    },
    {
      message: `Replay queued edits on reconnect`,
      author: JONAS,
      ago: 5 * DAY,
      files: { [`${PKG}/data/sync/EditQueue.kt`]: `package com.acme.mobile.data.sync\n\nclass EditQueue {\n    private val pending = ArrayDeque<Edit>()\n    fun push(edit: Edit) = pending.addLast(edit)\n    fun replay(send: (Edit) -> Unit) = pending.forEach(send)\n}\n` },
    },
  ] as Step[],
  mergedBy: MIRA,
  mergedAgo: 5 * DAY - 2 * HOUR,
}

/** The trunk clone's uncommitted edits: the files the demo session works on. */
const WORKING_TREE: Record<string, string> = {
  [`${PKG}/ui/board/BoardScreen.kt`]: `package com.acme.mobile.ui.board\n\n@Composable\nfun BoardScreen(state: BoardState, cache: BoardSnapshotCache) {\n    val groups = state.groups.ifEmpty { cache.lastGroups() }\n    LazyColumn {\n        groups.forEach { group ->\n            item { StatusHeader(group.status, group.issues.size) }\n            items(group.issues) { IssueRow(it) }\n        }\n    }\n}\n`,
  [`${PKG}/data/sync/BoardSnapshotCache.kt`]: `package com.acme.mobile.data.sync\n\nclass BoardSnapshotCache(private val dir: File) {\n    fun lastGroups(): List<StatusGroup> = read(dir.resolve("board.snapshot"))\n    fun persistOnStop(groups: List<StatusGroup>) = write(dir.resolve("board.snapshot"), groups)\n}\n`,
}

/** The two kept worktrees of `device-doctor.json`'s low-disk case. */
const KEPT_WORKTREES = {
  uncommitted: `exp-session-9497089a`,
  unpushed: `exp-EXP-1375`,
}

function git(cwd: string, args: string[], env: Record<string, string> = {}): string {
  const result = Bun.spawnSync({
    cmd: [`git`, ...args],
    cwd,
    env: {
      ...process.env,
      // A developer's global hooks, signing or templates must not shape the fixture.
      GIT_CONFIG_GLOBAL: `/dev/null`,
      GIT_CONFIG_NOSYSTEM: `1`,
      ...env,
    } as Record<string, string>,
    stdout: `pipe`,
    stderr: `pipe`,
  })
  if (result.exitCode !== 0) {
    throw new Error(`repo fixture: git ${args.join(` `)} failed in ${cwd}: ${result.stderr.toString().trim()}`)
  }
  return result.stdout.toString().trim()
}

function writeFiles(root: string, files: Record<string, string>): void {
  for (const [path, content] of Object.entries(files)) {
    const file = join(root, path)
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, content)
  }
}

/** Author AND committer pinned, so the graph's names and times are the fixture's. */
function authorEnv(person: Person, at: number): Record<string, string> {
  const date = new Date(at).toISOString()
  return {
    GIT_AUTHOR_NAME: person.name,
    GIT_AUTHOR_EMAIL: person.email,
    GIT_AUTHOR_DATE: date,
    GIT_COMMITTER_NAME: person.name,
    GIT_COMMITTER_EMAIL: person.email,
    GIT_COMMITTER_DATE: date,
  }
}

function commit(cwd: string, step: Pick<Step, `message` | `author` | `ago`>, now: number): void {
  git(cwd, [`commit`, `--quiet`, `--no-verify`, `-m`, step.message], authorEnv(step.author, now - step.ago))
}

function applyStep(cwd: string, step: Step, now: number): void {
  writeFiles(cwd, step.files)
  git(cwd, [`add`, `-A`])
  commit(cwd, step, now)
}

/**
 * (Re)build the fixture under `base` and return its repos root. Wipes what a
 * previous run left there first, so every run starts from the same tree.
 */
export function buildRepoFixture(base: string = REPO_FIXTURE_BASE, now: number = Date.now()): string {
  const reposRoot = join(base, `repos`)
  const originRoot = join(base, `origin`)
  rmSync(reposRoot, { recursive: true, force: true })
  rmSync(originRoot, { recursive: true, force: true })

  const origin = join(originRoot, REPO_FIXTURE_OWNER, `${REPO_FIXTURE_NAME}.git`)
  const clone = join(reposRoot, REPO_FIXTURE_OWNER, REPO_FIXTURE_NAME)
  mkdirSync(origin, { recursive: true })
  mkdirSync(clone, { recursive: true })
  git(origin, [`init`, `--quiet`, `--bare`, `--initial-branch=main`])
  git(clone, [`init`, `--quiet`, `--initial-branch=main`])
  git(clone, [`config`, `commit.gpgsign`, `false`])

  writeFiles(clone, BASE_FILES)
  git(clone, [`add`, `-A`])
  commit(clone, { message: `Initial Android project`, author: MIRA, ago: 10 * DAY }, now)

  const [first, ...rest] = HISTORY
  applyStep(clone, first!, now)
  // The merged feature branch: two commits beside trunk, then a merge commit,
  // so the commit graph draws a second lane.
  git(clone, [`switch`, `--quiet`, `-c`, MERGED_BRANCH.name])
  for (const step of MERGED_BRANCH.commits) applyStep(clone, step, now)
  git(clone, [`switch`, `--quiet`, `main`])
  git(clone, [`merge`, `--quiet`, `--no-ff`, `--no-edit`, `--no-verify`, MERGED_BRANCH.name, `-m`,
    `Merge pull request #${MERGED_BRANCH.pr} from ${REPO_FIXTURE_OWNER}/${MERGED_BRANCH.name}`,
  ], authorEnv(MERGED_BRANCH.mergedBy, now - MERGED_BRANCH.mergedAgo))
  for (const step of rest) applyStep(clone, step, now)

  git(clone, [`remote`, `add`, `origin`, origin])
  git(clone, [`push`, `--quiet`, `-u`, `origin`, `main`])
  git(clone, [`branch`, `--quiet`, `-D`, MERGED_BRANCH.name])

  // The kept worktrees, in the app's `<clone>.worktrees/<branch>` layout.
  const worktrees = join(reposRoot, REPO_FIXTURE_OWNER, `${REPO_FIXTURE_NAME}.worktrees`)
  const uncommitted = join(worktrees, KEPT_WORKTREES.uncommitted)
  git(clone, [`worktree`, `add`, `--quiet`, `-b`, KEPT_WORKTREES.uncommitted, uncommitted])
  writeFiles(uncommitted, { [`${PKG}/ui/board/BoardScreen.kt`]: `${BASE_FILES[`${PKG}/ui/board/BoardScreen.kt`]}\n// TODO: skeleton rows while the snapshot loads\n` })
  const unpushed = join(worktrees, KEPT_WORKTREES.unpushed)
  git(clone, [`worktree`, `add`, `--quiet`, `-b`, KEPT_WORKTREES.unpushed, unpushed])
  applyStep(
    unpushed,
    {
      message: `Warn when the disk runs low`,
      author: ALEX,
      ago: 3 * HOUR,
      files: { [`${PKG}/data/storage/DiskWatch.kt`]: `package com.acme.mobile.data.storage\n\nconst val LOW_DISK_BYTES = 10_000_000_000L\n` },
    },
    now
  )

  writeFiles(clone, WORKING_TREE)
  return reposRoot
}
