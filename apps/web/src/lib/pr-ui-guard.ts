import { githubApiHeaders } from "@/lib/integrations/github-app"
import { isNotRunTopic } from "@/lib/pr-body"

// EXP-1315: `exponential_pr_open` refuses a UI diff without pictures. The
// changed paths (GitHub compare base...head) name the UI platforms they
// touch; each touched platform needs ONE story picture (S-3; formerly a
// Guide picture) whose label starts
// with that platform word (`web-mobile` counts for web), else `notRun` must
// name it ("<platform>: <screen you could not run>"). Pure helpers + the one
// compare read; a compare failure never blocks a PR (the caller skips).

export const UI_PLATFORMS = [`web`, `ios`, `android`, `desktop`] as const
export type UiPlatform = (typeof UI_PLATFORMS)[number]

const PLATFORM_ROOTS: readonly [string, UiPlatform][] = [
  [`apps/web/src/`, `web`],
  [`packages/ui/`, `web`],
  [`packages/exponential-ui-react/`, `web`],
  [`apps/styleguide/`, `web`],
  [`apps/ios/`, `ios`],
  [`apps/android/`, `android`],
  [`apps/desktop/crates/ui/`, `desktop`],
  [`apps/desktop/crates/eui-gpui/`, `desktop`],
]

/** The UI platforms a diff touches, in `UI_PLATFORMS` order. */
export function uiPlatformsOfPaths(paths: readonly string[]): UiPlatform[] {
  const hit = new Set<UiPlatform>()
  for (const raw of paths) {
    const path = raw.replace(/^\.?\//, ``)
    for (const [root, platform] of PLATFORM_ROOTS) {
      if (path.startsWith(root)) hit.add(platform)
    }
  }
  return UI_PLATFORMS.filter((platform) => hit.has(platform))
}

const startsWithWord = (text: string, word: string) =>
  new RegExp(`^${word}(?![a-z0-9])`, `i`).test(text.trim())

const namesWord = (text: string, word: string) =>
  new RegExp(`(^|[^a-z0-9])${word}(?![a-z0-9])`, `i`).test(text)

/** The touched platforms with neither a labelled Guide picture nor a
 *  `notRun` mention: the call's string or the Guide's `Not run` topic (a
 *  later pr_open without the argument still passes). */
export function missingPlatformPictures(
  platforms: readonly UiPlatform[],
  guide:
    | readonly {
        topic?: string | null
        label?: string | null
        attachmentId?: string | null
        text?: string | null
      }[]
    | null,
  notRun: string | null | undefined
): UiPlatform[] {
  const entries = guide ?? []
  const labels = entries
    .filter((entry) => entry.attachmentId && entry.label)
    .map((entry) => entry.label as string)
  const notRuns = [
    ...(notRun ? [notRun] : []),
    ...entries
      .filter((entry) => !entry.attachmentId && entry.topic && isNotRunTopic(entry.topic))
      .map((entry) => entry.text ?? ``),
  ]
  return platforms.filter(
    (platform) =>
      !labels.some((label) => startsWithWord(label, platform)) &&
      !notRuns.some((text) => namesWord(text, platform))
  )
}

/** The refusal, naming the exact next step. */
export function missingPicturesMessage(missing: readonly UiPlatform[]): string {
  return `This diff changes UI on ${missing.join(`, `)} but your stories have no picture labelled ${missing
    .map((platform) => `'${platform} …'`)
    .join(` / `)}. Capture the screen, then exponential_show {surface, pictures: [{label: '<platform> …'}]} and run the returned curl line; or pass notRun: '<platform>: <screen you could not run>'.`
}

/** The changed paths of base...head (GitHub caps a compare at 300 files). */
export async function compareChangedPaths(opts: {
  repo: string
  base: string
  head: string
  token: string
}): Promise<string[]> {
  const res = await fetch(
    `https://api.github.com/repos/${opts.repo}/compare/${encodeURIComponent(opts.base)}...${encodeURIComponent(opts.head)}`,
    { headers: githubApiHeaders(opts.token) }
  )
  if (!res.ok) throw new Error(`GitHub returned ${res.status} comparing ${opts.base}...${opts.head}`)
  const data = (await res.json()) as {
    files?: { filename?: string; previous_filename?: string }[]
  }
  return (data.files ?? []).flatMap((file) =>
    [file.filename, file.previous_filename].filter((path): path is string => Boolean(path))
  )
}
