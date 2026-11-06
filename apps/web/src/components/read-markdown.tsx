import { useCallback, useMemo, useState, type ComponentType, type MouseEvent, type SVGProps } from "react"
import { AppMarkdown, ICON_COMPONENTS, ImagePreviewDialog } from "@exp/ui"
import type { HostPlugin, IconComponent } from "@exponential_app/eui-react"
import type { MarkdownOptions, TextLinkSegment } from "@exponential_app/eui/web"
import type { IconName } from "@exp/icons"
import { ENTITY_REF_ICON } from "@exp/domain-contract/entity-preview"
import { useIssueRefs, type IssueRefContextValue } from "@/components/issue-ref-provider"
import { useMentions, type MentionContextValue } from "@/components/mention-provider"
import { useAppLinkClick } from "@/hooks/use-app-link-click"
import { teamSlugOfPath } from "@/lib/app-link"
import { chipBareAppLinks } from "@/lib/prose-links"
import { agentCollection, attachmentCollection, boardCollection, sessionCollection, teamCollection } from "@/lib/collections"
import { exponentialUiIcons } from "@/lib/exponential-ui-icons"
import { splitIssueRefs } from "@/lib/issue-refs"
import { attachmentIdFromSrc, stripQuery, widthParamFromSrc } from "@/lib/markdown-image"
import { isOwnAttachmentHref } from "@/lib/markdown-media"
import { createMentionRegExp } from "@/lib/mention-refs"
import { isAudioContentType, isVideoContentType } from "@/lib/storage/issue-attachments"
import { ownPrivateHosts } from "@exponential_app/eui/web"

// #VAPP-258: every read-only Markdown of the web app (descriptions, comments,
// chats, plans, results) is genUI's Markdown (`AppMarkdown`, @exp/ui) with
// the app as its host (catalog/host.json `markdown`): `#IDENT` refs (the
// status glyph), `@email` mentions and bare in-app links chip through
// `textLinks`; attachment pictures take their synced size and open THE
// lightbox; a lone video / audio link is a media block. Links route in-app
// through THE opener (EXP-1188), a capture handler on the root.

/** #EXP-1674 (prose-links.json): agent prose with its bare in-app URLs
 *  rewritten to their chips: an issue through THE `#IDENT` chip, any other
 *  synced row as the entity chip's link run. Display text only. */
export function useChippedProse(text: string): string {
  const issueRefs = useIssueRefs()
  if (typeof window === `undefined` || !text.includes(`://`)) return text
  const teamId = issueRefs?.teamId
  return chipBareAppLinks(text, window.location.origin, teamSlugOfPath(window.location.pathname), (kind, id) => {
    switch (kind) {
      case `issue`:
        return issueRefs?.resolve(id)?.title
      case `board`:
        return boardCollection.toArray.find((board) => board.teamId === teamId && board.slug === id)?.name
      case `session`: {
        const session = sessionCollection.get(id)
        return session && session.teamId === teamId ? session.title?.trim() || `Session` : null
      }
      case `agent`: {
        const agent = agentCollection.get(id)
        return agent && agent.teamId === teamId ? agent.name : null
      }
    }
  })
}

export type ReadMarkdownSurface = `document` | `chat`

export interface ReadMarkdownProps {
  text: string
  /** `document` (descriptions, previews) or `chat` (comments, messages, plans). */
  surface?: ReadMarkdownSurface
  ariaLabel?: string
  /** Chat text is line-broken by hand: every newline is a break. */
  hardBreaks?: boolean
  /** Agent prose: bare identifiers chip too (EXP-760) and bare URLs link. */
  agentProse?: boolean
  /** The text is a growing prefix (a reply mid-stream). */
  streaming?: boolean
  className?: string
}

/** A chat picture's height cap (px); a document has none. */
export const CHAT_IMAGE_MAX_HEIGHT = 240

const ENTITY_CHIP = /\[((?:\\.|[^\]\\])*)\]\(([^()\s]*)#exp-chip-(board|session|agent)\)/g
const TOKEN_OPEN = ``
const TOKEN_CLOSE = ``
const TOKEN = /(\d+)/g
const BARE_URL = /(?<![\w[<(/])https?:\/\/[^\s<>()[\]`"']+/gi
const TRAILING = /[.,;:!?*_~]+$/
const STATUS_ICON = `exp-status|`

export type Entity = { label: string; href: string; icon: string }

/** The `#exp-chip-` links `chipBareAppLinks` wrote, as private tokens the
 *  text links turn into entity chips (a markdown link's label never asks). */
export function entityTokens(text: string): { text: string; entities: Entity[] } {
  if (!text.includes(`#exp-chip-`)) return { text, entities: [] }
  const entities: Entity[] = []
  const out = text.replace(ENTITY_CHIP, (_whole, label: string, href: string, kind: string) => {
    entities.push({ label: label.replace(/\\(.)/g, `$1`), href, icon: ENTITY_REF_ICON[kind] ?? ENTITY_REF_ICON.issue! })
    return `${TOKEN_OPEN}${entities.length - 1}${TOKEN_CLOSE}`
  })
  return { text: out, entities }
}

/** Agent prose's bare URLs as links (EXP-440 `linkify`): outside fences and
 *  code spans, never one already inside a link. */
export function linkBareUrls(text: string): string {
  const lowered = text.toLowerCase()
  if (!lowered.includes(`http://`) && !lowered.includes(`https://`)) return text
  let fence: string | null = null
  return text
    .split(`\n`)
    .map((line) => {
      const trimmed = line.trimStart()
      if (trimmed.startsWith("```") || trimmed.startsWith(`~~~`)) {
        const mark = trimmed.slice(0, 3)
        if (fence === null) fence = mark
        else if (fence === mark) fence = null
        return line
      }
      if (fence !== null) return line
      return line
        .split(/(`+[^`]*`+)/)
        .map((part, i) =>
          i % 2 === 1
            ? part
            : part.replace(BARE_URL, (raw) => {
                const tail = raw.match(TRAILING)?.[0] ?? ``
                const url = raw.slice(0, raw.length - tail.length)
                return url ? `[${url.replace(/[\\[\]*_`~]/g, (c) => `\\${c}`)}](${url})${tail}` : raw
              })
        )
        .join(``)
    })
    .join(`\n`)
}

type Segment = TextLinkSegment

/** catalog/host.json `textLinks` over one text piece: the entity tokens, the
 *  `#IDENT` refs that resolve in the team (identifier + title, the status
 *  glyph) and the known `@email` mentions (the member's name). */
export function proseSegments(
  piece: string,
  entities: readonly Entity[],
  issueRefs: Pick<IssueRefContextValue, `resolve`> | null,
  mentions: Pick<MentionContextValue, `resolve`> | null,
  bare: boolean
): Segment[] | null {
  const hits: { start: number; end: number; segment: Segment }[] = []
  if (entities.length > 0 && piece.includes(TOKEN_OPEN)) {
    for (const m of piece.matchAll(TOKEN)) {
      const e = entities[Number(m[1])]
      if (e) hits.push({ start: m.index, end: m.index + m[0].length, segment: { text: e.label, href: e.href, chip: { kind: `entity`, label: e.label, icon: e.icon } } })
    }
  }
  if (issueRefs) {
    let at = 0
    for (const s of splitIssueRefs(piece, { bare })) {
      const resolved = s.identifier ? issueRefs.resolve(s.identifier) : null
      if (resolved) {
        // Today's issue chip: the status glyph, the mono identifier, the title.
        const title = resolved.title.trim()
        const chip = title ? { kind: `issue` as const, ident: resolved.identifier, label: title } : { kind: `issue` as const, label: resolved.identifier }
        hits.push({ start: at, end: at + s.text.length, segment: { text: s.text, href: `~/issues/${resolved.identifier}`, chip: { ...chip, icon: `${STATUS_ICON}${resolved.statusIcon}|${resolved.statusColor}` } } })
      }
      at += s.text.length
    }
  }
  if (mentions && piece.includes(`@`)) {
    for (const m of piece.matchAll(createMentionRegExp())) {
      const email = m[1]!
      const member = mentions.resolve(email)
      if (member) hits.push({ start: m.index, end: m.index + m[0].length, segment: { text: m[0], href: `mailto:${email}`, chip: { kind: `mention`, label: `@${member.name.trim() || email}` } } })
    }
  }
  if (hits.length === 0) return null
  hits.sort((a, b) => a.start - b.start)
  const out: Segment[] = []
  let at = 0
  for (const hit of hits) {
    if (hit.start < at) continue
    if (hit.start > at) out.push({ text: piece.slice(at, hit.start) })
    out.push(hit.segment)
    at = hit.end
  }
  if (at < piece.length) out.push({ text: piece.slice(at) })
  return out
}

/** A synced attachment's display size: its probed pixels, scaled to the
 *  `?w=` display width when the src carries one (EXP-52). */
export function attachmentImageSize(src: string, row: { width?: number | null; height?: number | null } | undefined): { width: number; height: number } | null {
  if (!row || typeof row.width !== `number` || typeof row.height !== `number` || row.width <= 0 || row.height <= 0) return null
  const w = widthParamFromSrc(src)
  return w ? { width: w, height: (row.height * w) / row.width } : { width: row.width, height: row.height }
}

const statusIcons = new Map<string, IconComponent>()
/** `exp-status|<icon>|<css colour>` → the status glyph in that colour. */
function statusIcon(name: string): IconComponent | undefined {
  if (!name.startsWith(STATUS_ICON)) return undefined
  let hit = statusIcons.get(name)
  if (!hit) {
    const [, icon, color] = name.split(`|`)
    const Glyph = ((ICON_COMPONENTS[icon as IconName] as IconComponent | undefined) ?? appIcon(icon!)) as ComponentType<SVGProps<SVGSVGElement>> | undefined
    if (!Glyph) return undefined
    hit = (props) => <Glyph {...props} style={{ ...props.style, color }} />
    statusIcons.set(name, hit)
  }
  return hit
}
const appIcon = exponentialUiIcons as (name: string) => IconComponent | undefined
const appIcons = (name: string) => statusIcon(name) ?? appIcon(name)

/** The app as genUI's Markdown host for one surface. */
export function useReadMarkdownHost(options: MarkdownOptions, agentProse: boolean, entities: readonly Entity[], onImagePress?: (src: string) => void): HostPlugin {
  const issueRefs = useIssueRefs()
  const mentions = useMentions()
  const { breaks, density, streaming, imageMaxHeight, selectable } = options
  const origin = typeof window === `undefined` ? `http://localhost` : window.location.origin
  // `~/x` = the team on screen, else the team the refs resolve in.
  const team = (typeof window === `undefined` ? null : teamSlugOfPath(window.location.pathname)) ?? (issueRefs?.teamId ? teamCollection.get(issueRefs.teamId)?.slug : undefined) ?? null
  return useMemo(() => {
    const allowPrivate = ownPrivateHosts()
    return {
      icons: appIcons,
      markdownOptions: { breaks, density, streaming, imageMaxHeight, selectable },
      media: { baseUrl: origin, allowPrivate },
      urls: { baseUrl: origin, allowPrivate, ...(team ? { linkBase: `/t/${encodeURIComponent(team)}` } : {}) },
      imageSize: (src) => {
        const id = attachmentIdFromSrc(src)
        return id ? attachmentImageSize(src, attachmentCollection.get(id)) : null
      },
      onImagePress,
      attachment: (href) => {
        if (!isOwnAttachmentHref(href, origin)) return null
        const row = attachmentCollection.get(attachmentIdFromSrc(href)!)
        if (!row) return null
        const kind = isVideoContentType(row.contentType) ? `video` : isAudioContentType(row.contentType) ? `audio` : null
        return kind ? { kind, name: row.filename } : null
      },
      textLinks: (piece) => proseSegments(piece, entities, issueRefs, mentions, agentProse),
    } satisfies HostPlugin
  }, [origin, team, breaks, density, streaming, imageMaxHeight, selectable, onImagePress, issueRefs, mentions, agentProse, entities])
}

/** THE read-only Markdown of the app. */
export function ReadMarkdown({ text, surface = `chat`, ariaLabel, hardBreaks, agentProse = false, streaming, className }: ReadMarkdownProps) {
  const onLinkClick = useAppLinkClick()
  const [preview, setPreview] = useState<string | null>(null)
  const chipped = useChippedProse(agentProse ? text : ``)
  const prose = agentProse ? linkBareUrls(chipped) : text
  const { text: shown, entities } = useMemo(() => entityTokens(prose), [prose])
  const host = useReadMarkdownHost(
    {
      breaks: hardBreaks ? `hard` : undefined,
      density: surface,
      streaming: streaming || undefined,
      imageMaxHeight: surface === `document` ? null : CHAT_IMAGE_MAX_HEIGHT,
    },
    agentProse,
    entities,
    setPreview
  )
  const onClickCapture = useCallback(
    (event: MouseEvent<HTMLElement>) => {
      // A mention chip names a member; it opens nothing.
      if ((event.target as Element | null)?.closest?.(`a[data-chip="mention"]`)) return void event.preventDefault()
      onLinkClick(event)
    },
    [onLinkClick]
  )
  const row = preview ? attachmentCollection.get(attachmentIdFromSrc(preview) ?? ``) : undefined
  return (
    <div onClickCapture={onClickCapture}>
      <AppMarkdown text={shown} host={host} ariaLabel={ariaLabel} className={className} />
      {preview ? (
        <ImagePreviewDialog
          open
          onOpenChange={(open) => !open && setPreview(null)}
          src={attachmentIdFromSrc(preview) ? stripQuery(preview) : preview}
          alt={row?.filename ?? ``}
          label={row?.filename ?? `Image`}
          naturalSize={row && typeof row.width === `number` && typeof row.height === `number` ? { width: row.width, height: row.height } : null}
        />
      ) : null}
    </div>
  )
}
