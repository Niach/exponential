/* Small shared bits of the component pages: group names, kind badges, the
   platform dots and inline `code` in catalog sentences. */
import { Fragment } from "react"
import type { ComponentDoc } from "../lib/catalog"
import { PLATFORMS } from "./platforms"
import { shotExists } from "./Shot"

const GROUP_LABELS: Record<string, string> = {
  layout: `Layout`,
  list: `Lists`,
  text: `Text`,
  media: `Media`,
  navigation: `Navigation`,
  overlay: `Overlays`,
  feedback: `Feedback`,
  data: `Data`,
  action: `Actions`,
  input: `Inputs`,
  other: `Other`,
}

/** A group id → its heading (an unknown group shows capitalised). */
export const groupLabel = (group: string) => GROUP_LABELS[group] ?? group.charAt(0).toUpperCase() + group.slice(1)

export function KindBadges({ doc }: { doc: Pick<ComponentDoc, `kind` | `lite`> }) {
  return (
    <span className="sdk-badges">
      <span className={`sdk-badge is-${doc.kind}`} title={doc.kind === `native` ? `Painted by every renderer` : `Expands into natives before painting`}>
        {doc.kind}
      </span>
      {doc.lite && (
        <span className="sdk-badge is-lite" title="In the core-lite catalog">
          lite
        </span>
      )}
    </span>
  )
}

/** A catalog sentence with its `backticked` words as code. */
export function RichText({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`)/g)
  return (
    <>
      {parts.map((p, i) => (p.startsWith(`\``) && p.endsWith(`\``) && p.length > 2 ? <code key={i}>{p.slice(1, -1)}</code> : <Fragment key={i}>{p}</Fragment>))}
    </>
  )
}

/** One dot per platform: filled = a native shot is stored, hollow = pending. */
export function PlatformDots({ viewId }: { viewId: string }) {
  const have = PLATFORMS.filter((p) => shotExists(viewId, p.id))
  const label = have.length === PLATFORMS.length ? `Shots on all four platforms` : `Shots: ${have.map((p) => p.label).join(`, `) || `none`}; pending: ${PLATFORMS.filter((p) => !have.includes(p)).map((p) => p.label).join(`, `)}`
  return (
    <span className="sdk-dots" role="img" aria-label={label} title={label}>
      {PLATFORMS.map((p) => (
        <span key={p.id} className={`sdk-pdot${shotExists(viewId, p.id) ? ` is-on` : ``}`} />
      ))}
    </span>
  )
}
