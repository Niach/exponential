import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from "react"
import { dockRows } from "@exp/domain-contract/dock"
import {
  PLAN_PAGE_COPY,
  PLAN_PAGE_LAYOUT,
  PLAN_PAGE_TEST_IDS,
  planBlocks,
  planCountsLine,
  planOutline,
  type PlanPicture,
} from "@exp/domain-contract/plan-page"
import { askDockTitle } from "@exp/domain-contract/ask-dock"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { Button } from "./button"
import { cn } from "./cn"
import { Dock } from "./dock"
import { conceptIcon } from "./icons.generated"
import { ImagePreviewDialog } from "./image-preview-dialog"
import { Outline } from "./outline"
import { Mosaic, PictureCarousel } from "./picture-group"
import { PlanCardSurface } from "./plan-card"
import { PlanRail, ThreadPlanSheet, type ThreadPlanDecision } from "./session-thread-view"
import { Sheet, SheetContent, SheetDescription, SheetTitle } from "./sheet"
import { StoryKindBadge } from "./stories"

// #EXP-1493: THE plan page ×web (fixtures/plan-page.json; desktop crates/ui
// plan_page, iOS ExpUI PlanPage, Android ui/plan/PlanPage): the expanded
// form of THE plan card. Wide (md+) = the header row over three columns:
// THE Outline (scroll-spy), the document (THE markdown, h2 anchors, picture
// groups as THE Mosaic), THE rail (the plan card's stepper + decision,
// sticky). Phones = the same document in one scroll (groups as THE
// Carousel), the outline in a sheet (☰), THE Dock's plan card at the foot
// opening the questions-only sheet; no composer. Every picture opens THE
// lightbox over all of the plan's pictures.

const BackIcon = conceptIcon(`ui-back`)
const CloseIcon = conceptIcon(`ui-close`)
const MenuIcon = conceptIcon(`ui-menu`)

type PlanRow = Extract<ThreadRow, { type: `plan` }>

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
  chatName,
  mobile,
  renderText,
  attachmentSrc = (id) => `/api/attachments/${id}`,
  busy,
  onDecide,
  onBack,
  onClose,
  onOpenChat,
  initialActive = null,
  className,
}: {
  row: PlanRow
  /** The chat's name (the wide breadcrumb; the Dock card's caption). */
  chatName: string
  mobile: boolean
  renderText: (text: string) => ReactNode
  attachmentSrc?: (attachmentId: string) => string
  busy: boolean
  onDecide?: (input: ThreadPlanDecision) => void
  onBack?: () => void
  onClose?: () => void
  onOpenChat?: () => void
  /** The outline's active row before any scroll (the styleguide's). */
  initialActive?: string | null
  className?: string
}) {
  const blocks = useMemo(() => planBlocks(row.plan), [row.plan])
  const outline = useMemo(() => planOutline(row.plan), [row.plan])
  const pictures = useMemo(() => documentPictures(blocks), [blocks])
  const counts = planCountsLine({
    sections: outline.length,
    pictures: pictures.length,
    questions: row.questions.length,
  })
  const waiting = row.decision === null

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
      {mobile && (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-2">
            <StoryKindBadge kind="plan" />
          </div>
          {counts && <div className="text-xs text-muted-foreground">{counts}</div>}
        </div>
      )}
      {blocks.map((block, at) =>
        block.kind === `section` ? (
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

  const dock = useMemo(
    () =>
      dockRows([
        {
          kind: `plan`,
          id: row.id,
          text: row.plan,
          sessionName: chatName || null,
          questions: row.questions.length,
          createdAt: 0,
        },
      ]),
    [row.id, row.plan, row.questions.length, chatName]
  )
  const [sheetOpen, setSheetOpen] = useState<string | null>(null)
  const [dockHeight, setDockHeight] = useState(0)

  return (
    <div data-testid={PLAN_PAGE_TEST_IDS.page} data-decision={row.decision ?? undefined} className={cn(`flex h-full min-h-0 flex-col`, className)}>
      {mobile ? (
        <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-2">
          <Button variant="ghost" size="icon-sm" aria-label={PLAN_PAGE_COPY.title} onClick={onBack ?? onClose}>
            <BackIcon />
          </Button>
          <span className="min-w-0 flex-1 truncate text-[15px] font-semibold">{PLAN_PAGE_COPY.title}</span>
          {hasOutline && (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={PLAN_PAGE_COPY.outline}
              data-testid={PLAN_PAGE_TEST_IDS.outlineToggle}
              onClick={() => setOutlineOpen(true)}
            >
              <MenuIcon />
            </Button>
          )}
        </div>
      ) : (
        <div
          className="flex shrink-0 items-center gap-3 border-b border-border px-4"
          style={{ height: PLAN_PAGE_LAYOUT.headerHeight }}
        >
          {onBack && (
            <Button variant="ghost" size="icon-sm" aria-label={PLAN_PAGE_COPY.openChat} onClick={onBack}>
              <BackIcon />
            </Button>
          )}
          <span className="min-w-0 truncate text-xs text-muted-foreground">{chatName}</span>
          <span className="text-xs text-muted-foreground">/</span>
          <span className="text-sm font-semibold">{PLAN_PAGE_COPY.title}</span>
          <StoryKindBadge kind="plan" />
          {waiting && <span aria-hidden className="size-1.5 rounded-full bg-yellow-400" />}
          <span className="flex-1" />
          {counts && <span className="hidden truncate text-xs text-muted-foreground lg:inline">{counts}</span>}
          {onOpenChat && (
            <Button variant="ghost" size="sm" onClick={onOpenChat}>
              {PLAN_PAGE_COPY.openChat}
            </Button>
          )}
          {onClose && (
            <Button variant="ghost" size="icon-sm" aria-label={PLAN_PAGE_COPY.openChat} onClick={onClose}>
              <CloseIcon />
            </Button>
          )}
        </div>
      )}
      {mobile ? (
        <div className="relative min-h-0 flex-1">
          <div ref={scroller} className="absolute inset-0 overflow-y-auto overscroll-contain" style={{ paddingBottom: dockHeight }}>
            {document}
          </div>
          <Dock
            rows={dock}
            openId={sheetOpen}
            onOpenChange={setSheetOpen}
            onHeight={setDockHeight}
            renderSheet={() => (
              <ThreadPlanSheet
                row={row}
                renderText={renderText}
                attachmentSrc={attachmentSrc}
                busy={busy}
                onDecide={onDecide}
                questionsOnly
              />
            )}
          />
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
        </div>
      ) : (
        <div
          className="grid min-h-0 flex-1"
          style={{
            gridTemplateColumns: `${hasOutline ? `${PLAN_PAGE_LAYOUT.outlineWidth}px ` : ``}minmax(0, 1fr) ${PLAN_PAGE_LAYOUT.railWidth}px`,
          }}
        >
          {hasOutline && (
            <div data-testid={PLAN_PAGE_TEST_IDS.outline} className="min-h-0 overflow-y-auto border-r border-border px-3 py-5">
              <Outline caption={PLAN_PAGE_COPY.outline} entries={entries} active={active} onSelect={jump} footer={questionsRow} />
            </div>
          )}
          <div ref={scroller} className="min-h-0 overflow-y-auto overscroll-contain">
            {document}
          </div>
          <div className="flex min-h-0 flex-col border-l border-border bg-sidebar px-4 py-5">
            <PlanRail
              row={row}
              attachmentSrc={attachmentSrc}
              busy={busy}
              onDecide={onDecide}
              heading={PLAN_PAGE_COPY.questions}
              className="h-full"
            />
          </div>
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
