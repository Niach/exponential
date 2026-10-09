/* One example file of guides/** on a guide page: exactly what CI compiles
   (guides/check.ts), read through guideSource(), never a copy. Its own module
   so only the guide pages carry the bundled sources. */
import type { ReactNode } from "react"
import { DocsCode } from "@exp/site-shell"
import { guideSource } from "../lib/guide-source"
import { LINKS } from "../lib/site"

const LANGUAGES: [RegExp, string][] = [
  [/\.tsx$/, `tsx`],
  [/\.ts$/, `ts`],
  [/\.json$/, `json`],
  [/\.swift$/, `swift`],
  [/\.kts$/, `kotlin (gradle)`],
  [/\.kt$/, `kotlin`],
  [/\.rs$/, `rust`],
  [/\.toml$/, `toml`],
]

/** One example file of guides/**: exactly what CI compiles, never a copy. */
export function GuideFile({ path, caption }: { path: string; caption?: ReactNode }) {
  const language = LANGUAGES.find(([re]) => re.test(path))?.[1] ?? `text`
  return (
    <figure className="guide-file">
      <figcaption>
        <a href={LINKS.source(`apps/ui-site/guides/${path}`)} className="guide-file-path">
          guides/{path}
        </a>
        {caption && <span className="guide-file-caption">{caption}</span>}
      </figcaption>
      <DocsCode language={language}>{guideSource(path)}</DocsCode>
    </figure>
  )
}
