import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from "react"
import {
  PLAN_PAGE_COPY,
  PLAN_PAGE_LAYOUT,
  PLAN_PAGE_PANEL,
  PLAN_PAGE_TEST_IDS,
  planBlocks,
  planOutline,
  planPaneLayout,
  type PlanPicture,
} from "@exp/domain-contract/plan-page"
import { askDockTitle } from "@exp/domain-contract/ask-dock"
import type { SessionChatRow } from "@exp/domain-contract/session-tree"
import { Button } from "./button"
import { cn } from "./cn"
import { conceptIcon } from "./icons.generated"
import { ImagePreviewDialog } from "./image-preview-dialog"
import { Outline } from "./outline"
import { Mosaic, PictureCarousel } from "./picture-group"
import { PlanCardSurface } from "./plan-card"
import { PlanRail, type ChatPlanDecision } from "./session-chat-view"
import { Sheet, SheetContent, SheetDescription, SheetTitle } from "./sheet"

// #EXP-1493 → #EXP-1662: THE plan ×web (fixtures/plan-page.json; desktop
// crates/ui plan_page, iOS ExpUI PlanPage, Android ui/plan/PlanPage): the BODY
// of a chat's plan tab, under the chat's own header + strip, on its ground.
// Wide (md+) = THE Outline (scroll-spy) + the document (the big title, THE
// markdown, h2 anchors, picture groups as THE Mosaic) + THE questions rail on
// the right (#FEED-191). Phones = the document (groups as THE Carousel), the
// outline in a sheet (☰), the questions floating over the document's foot.
// #EXP-1786: wide measures the body's OWN width (`planPaneLayout`): a narrow
// pane (beside the chat) drops the outline column, then floats the questions.
// Both = THE stepper + the decision. Every picture opens THE
// lightbox over all of the plan's pictures.

const MenuIcon = conceptIcon(`ui-menu`)

type PlanRow = Extract<SessionChatRow, { type: `plan` }>

const MARKDOWN_PICTURE = /!\[([^\]]*)\]\(([^)\s]+)\)/g

/** Every picture the document shows, in order (groups and inline). */
function documentPictures(blocks: ReturnType<typeof planBlocks>): PlanPicture[] {
  const out: PlanPicture[] = []
  for (const block of blocks) {
    if (block.kind === `pictures`) out.push(...block.pictures)
    else if (block.kind === `markdown`) {
      let fenced = false
      for (const line of block.text.split(`\n`)) {
        if (/^\s*(```|~~~)/.test(line)) fenced = !fenced
        else if (!fenced) for (const match of line.matchAll(MARKDOWN_PICTURE)) out.push({ alt: match[1]!, src: match[2]! })
      }
    }
  }
  return out
}

export function PlanPage({
  row,
  mobile,
  renderText,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  busy,
  onDecide,
  topInset = 0,
  initialActive = null,
  className,
}: {
  row: PlanRow
  mobile: boolean
  renderText: (text: string) => ReactNode
  attachmentSrc?: (attachmentId: string) => string
  busy: boolean
  onDecide?: (input: ChatPlanDecision) => void
  /** The phone header band over the body. */
  topInset?: number
  /** The outline's active row before any scroll (the styleguide's). */
  initialActive?: string | null
  className?: string
}) {
  const blocks = useMemo(() => planBlocks(row.plan), [row.plan])
  const outline = useMemo(() => planOutline(row.plan), [row.plan])
  const pictures = useMemo(() => documentPictures(blocks), [blocks])

  // ── the lightbox over every picture ──
  const [lightbox, setLightbox] = useState<number | null>(null)
  const openPicture = useCallback(
    (picture: PlanPicture) => {
      const at = pictures.findIndex((entry) => entry.src === picture.src)
      setLightbox(at >= 0 ? at : null)
    },
    [pictures]
  )
  const shown = lightbox === null ? null : (pictures[lightbox] ?? null)
  // A picture inside the prose (a lone one or one mid-sentence) opens THE
  // lightbox too, never the renderer's own preview: matched by its path.
  const openInlinePicture = useCallback(
    (event: MouseEvent<HTMLDivElement>) => {
      const target = event.target
      if (!(target instanceof HTMLImageElement)) return
      const path = (src: string) => new URL(src, window.location.href).pathname
      const at = pictures.findIndex((entry) => path(entry.src) === path(target.currentSrc || target.src))
      if (at < 0) return
      event.preventDefault()
      event.stopPropagation()
      setLightbox(at)
    },
    [pictures]
  )

  // ── scroll-spy: the last section whose heading passed the column top ──
  const scroller = useRef<HTMLDivElement | null>(null)
  const [active, setActive] = useState<string | null>(initialActive)
  useEffect(() => {
    const node = scroller.current
    if (!node) return
    const update = () => {
      let hit: string | null = null
      for (const section of outline) {
        const heading = node.querySelector<HTMLElement>(`[data-section="${section.id}"]`)
        if (heading && heading.offsetTop - node.offsetTop <= node.scrollTop + 16) hit = section.id
      }
      setActive(hit)
    }
    if (!initialActive) update()
    node.addEventListener(`scroll`, update, { passive: true })
    return () => node.removeEventListener(`scroll`, update)
  }, [outline])
  const [outlineOpen, setOutlineOpen] = useState(false)
  const jump = (id: string) => {
    setOutlineOpen(false)
    const node = scroller.current
    const heading = node?.querySelector<HTMLElement>(`[data-section="${id}"]`)
    if (node && heading) node.scrollTo({ top: heading.offsetTop - node.offsetTop - 8, behavior: `smooth` })
  }
  const entries = outline.map((section) => ({ id: section.id, title: section.title, count: section.pictures }))
  const hasOutline = entries.length > 0 || row.questions.length > 0
  const questionsRow =
    row.questions.length > 0 ? (
      <div className="flex min-h-8 items-center gap-1 px-2 text-xs text-muted-foreground">
        <span>{PLAN_PAGE_COPY.questions}</span>
        <span>· {row.questions.length}</span>
      </div>
    ) : null

  const padding = mobile ? PLAN_PAGE_LAYOUT.columnPadding.phone : PLAN_PAGE_LAYOUT.columnPadding.wide
  const document = (
    <div
      data-testid={PLAN_PAGE_TEST_IDS.document}
      className="mx-auto flex w-full min-w-0 flex-col"
      style={{ maxWidth: PLAN_PAGE_LAYOUT.columnMaxWidth + 2 * padding, padding, gap: PLAN_PAGE_LAYOUT.blockGap }}
    >
      {blocks.map((block, at) =>
        block.kind === `title` ? (
          <h1
            key={at}
            data-slot="plan-title"
            className={cn(`font-semibold leading-tight`, mobile && hasOutline && `pr-8`)}
            style={{ fontSize: PLAN_PAGE_LAYOUT.titleSize }}
          >
            {block.text}
          </h1>
        ) : block.kind === `section` ? (
          <h2 key={`${block.id}-${at}`} data-section={block.id} id={block.id} className="mt-2 text-base font-semibold">
            {block.title}
          </h2>
        ) : block.kind === `pictures` ? (
          mobile ? (
            <PictureCarousel
              key={at}
              pictures={block.pictures}
              onOpen={(index) => openPicture(block.pictures[index]!)}
              className="-mr-2"
            />
          ) : (
            <Mosaic key={at} pictures={block.pictures} onOpen={(index) => openPicture(block.pictures[index]!)} />
          )
        ) : (
          <div key={at} className="min-w-0 text-sm" onClickCapture={openInlinePicture}>
            {renderText(block.text)}
          </div>
        )
      )}
      {row.surface && (
        <PlanCardSurface surface={row.surface} data={row.surfaceData} disabled testKey={`page-${row.id}`} dataRef={{ current: null }} />
      )}
    </div>
  )

  // #EXP-1786: the columns the body's own width holds (phones: neither).
  const root = useRef<HTMLDivElement | null>(null)
  const [width, setWidth] = useState<number | null>(null)
  useLayoutEffect(() => {
    const node = root.current
    if (!node) return
    // 0 = not laid out (yet): unmeasured.
    const measure = () => setWidth(node.getBoundingClientRect().width || null)
    measure()
    if (typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    return () => observer.disconnect()
  }, [])
  // Unmeasured (server render) = the full wide form.
  const pane = mobile ? { outline: false, rail: false } : width === null ? { outline: true, rail: true } : planPaneLayout(width)
  const floating = !pane.rail

  // The floating panel: the document scrolls clear of it (#FEED-191: the
  // rail needs no padding).
  const [panelHeight, setPanelHeight] = useState(0)
  const panel = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    const node = panel.current
    if (!node || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => setPanelHeight(node.offsetHeight))
    observer.observe(node)
    return () => observer.disconnect()
  }, [floating])

  return (
    <div ref={root} data-testid={PLAN_PAGE_TEST_IDS.page} data-decision={row.decision ?? undefined} className={cn(`flex h-full min-h-0`, className)}>
      {pane.outline && hasOutline && (
        <div
          data-testid={PLAN_PAGE_TEST_IDS.outline}
          className="min-h-0 shrink-0 overflow-y-auto border-r border-border px-3 py-5"
          style={{ width: PLAN_PAGE_LAYOUT.outlineWidth }}
        >
          <Outline caption={PLAN_PAGE_COPY.outline} entries={entries} active={active} onSelect={jump} footer={questionsRow} />
        </div>
      )}
      <div className="relative min-h-0 min-w-0 flex-1">
        <div
          ref={scroller}
          className="absolute inset-0 overflow-y-auto overscroll-contain"
          style={{ paddingTop: topInset, paddingBottom: floating ? panelHeight + 2 * PLAN_PAGE_PANEL.gap : undefined }}
        >
          {document}
        </div>
        {mobile && hasOutline && (
          <Button
            variant="ghost"
            size="icon-sm"
            className="absolute right-2 text-muted-foreground"
            style={{ top: topInset + 8 }}
            aria-label={PLAN_PAGE_COPY.outline}
            data-testid={PLAN_PAGE_TEST_IDS.outlineToggle}
            onClick={() => setOutlineOpen(true)}
          >
            <MenuIcon />
          </Button>
        )}
        {floating && (
        <div
          className="pointer-events-none absolute inset-0 flex items-end justify-center"
          style={{ padding: PLAN_PAGE_PANEL.gap, paddingTop: topInset + PLAN_PAGE_PANEL.gap }}
        >
          <div
            ref={panel}
            data-slot="plan-questions-panel"
            className="pointer-events-auto flex w-full flex-col overflow-hidden border border-glass-stroke-card bg-glass-card-opaque shadow-lg"
            style={{
              maxWidth: PLAN_PAGE_PANEL.maxWidth,
              maxHeight: `${PLAN_PAGE_PANEL.maxHeightFraction * 100}%`,
              padding: PLAN_PAGE_PANEL.padding,
              borderRadius: PLAN_PAGE_PANEL.radius,
            }}
          >
            <PlanRail
              row={row}
              attachmentSrc={attachmentSrc}
              busy={busy}
              onDecide={onDecide}
              className="min-h-0 flex-auto"
            />
          </div>
        </div>
        )}
        {mobile && (
          <Sheet open={outlineOpen} onOpenChange={setOutlineOpen}>
            <SheetContent side="bottom" showGrabber className="gap-0 p-3 pb-[max(1.5rem,env(safe-area-inset-bottom))]">
              <SheetTitle className="sr-only">{PLAN_PAGE_COPY.outline}</SheetTitle>
              <SheetDescription className="sr-only">{askDockTitle(row.plan)}</SheetDescription>
              <Outline
                caption={PLAN_PAGE_COPY.outline}
                entries={entries}
                active={active}
                onSelect={jump}
                footer={questionsRow}
              />
            </SheetContent>
          </Sheet>
        )}
      </div>
      {pane.rail && (
        <div
          data-slot="plan-questions-rail"
          className="flex min-h-0 shrink-0 flex-col border-l border-border bg-sidebar"
          style={{ width: PLAN_PAGE_LAYOUT.railWidth, padding: PLAN_PAGE_PANEL.padding }}
        >
          <PlanRail
            row={row}
            attachmentSrc={attachmentSrc}
            busy={busy}
            onDecide={onDecide}
            heading
            className="min-h-0 flex-auto"
          />
        </div>
      )}
      <ImagePreviewDialog
        open={shown !== null}
        onOpenChange={(open) => (open ? undefined : setLightbox(null))}
        src={shown?.src ?? ``}
        alt={shown?.alt ?? ``}
        label={shown?.alt ?? ``}
        onPrev={lightbox !== null && lightbox > 0 ? () => setLightbox(lightbox - 1) : undefined}
        onNext={lightbox !== null && lightbox < pictures.length - 1 ? () => setLightbox(lightbox + 1) : undefined}
      />
    </div>
  )
}
