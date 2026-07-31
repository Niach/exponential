// closedloop/segments/feedback.tsx — clip 4 (235f, given more room by
// EXP-385): a visitor hits the dead Pay-now button on acme.shop, reports it
// through the embedded feedback widget (the success card now HOLDS before the
// cut), and the whip-pan lands on the board as EXP-151 pops into Todo —
// exactly where clip 1 begins, so the loop wraps into the live board.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT, WIN } from "../../ships/theme"
import {
  Camera,
  Caption,
  CursorLayer,
  WindowChassis,
  type CamKey,
  type CursorKey,
} from "../../ships/rig"
import {
  BoardActions,
  BoardTool,
  SidebarPane,
} from "../../ships/surfaces/board"
import {
  CenterEmptyState,
  DockCollapsedStrip,
  ExpandedRail,
  TitleBar,
} from "../../ships/surfaces/chrome"
import {
  BrowserChassis,
  FeedbackFab,
  SITE_ANCHORS,
  SiteViewport,
} from "../surfaces/sitemock"
import { WIDGET_ANCHORS, WidgetPanel } from "../surfaces/widgetmock"
import {
  CL,
  CL_BOARD,
  COPY,
  LIVE_EDIT_ID,
  NEW_ISSUE_ID,
  REMOTE_DRAG_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import { CLAMP, SegmentShell, type SegmentProps } from "./common"

const DUR = SEGMENT_DURATIONS.feedback

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  payClick: 26,
  fabHover: 48,
  fabClick: 58,
  panelAppear: 64,
  fabRest: 72,
  annotate: 76,
  titleClick: 88,
  titleType: 92,
  detailsClick: 110,
  detailsType: 114,
  sendHover: 138,
  sendClick: 142,
  success: 154, // "Thanks — sent!" holds a beat before the cut (EXP-385)
  whip: 182, // hard cut site → board under the whip blur
  cascade: 184,
  insert: 198, // EXP-151 pops into Todo
} as const

const CAPTIONS = {
  fb1: { in: 12, out: 60 },
  fb2: { in: 206, out: 226 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.1, x: 784, y: 490 },
  { f: 14, s: 1.1, x: 784, y: 490 },
  { f: 34, s: 1.55, x: 920, y: 475 },
  { f: 44, s: 1.55, x: 920, y: 475 },
  { f: 56, s: 1.85, x: 1049, y: 688 },
  { f: B.whip - 1, s: 1.85, x: 1049, y: 688 },
  { f: B.whip, s: 1.9, x: 640, y: 300 },
]

const whipBlurAt = (frame: number): number =>
  interpolate(
    frame,
    [B.whip - 4, B.whip - 1, B.whip + 1, B.whip + 4],
    [0, 3, 3, 0],
    CLAMP
  )

// ── Cursor (site side) ────────────────────────────────────────────────────────
const PAY = SITE_ANCHORS.payButton
const FAB = SITE_ANCHORS.fab
const WT = WIDGET_ANCHORS.titleInput
const WD = WIDGET_ANCHORS.detailsInput
const WS = WIDGET_ANCHORS.send

const CURSOR_KEYS: CursorKey[] = [
  { f: 0, x: 900, y: 560 },
  { f: 20, x: PAY.x, y: PAY.y },
  { f: 40, x: PAY.x, y: PAY.y },
  { f: 48, x: FAB.x, y: FAB.y },
  { f: 64, x: FAB.x, y: FAB.y },
  { f: 70, x: 1440, y: 820 },
  { f: 84, x: WT.x, y: WT.y },
  { f: 104, x: WT.x, y: WT.y },
  { f: 108, x: WD.x, y: WD.y },
  { f: 132, x: WD.x, y: WD.y },
  { f: 138, x: WS.x, y: WS.y },
  { f: 148, x: WS.x, y: WS.y },
  { f: 164, x: 1430, y: 900 },
]
const CURSOR_CLICKS = [
  B.payClick,
  B.fabClick,
  B.titleClick,
  B.detailsClick,
  B.sendClick,
]

// ── The clip ──────────────────────────────────────────────────────────────────
export const FeedbackSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const onSite = frame < B.whip
  const blur = whipBlurAt(frame)
  const captionSize = Math.round(72 * textScale)

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill
        style={{
          filter: blur > 0.05 ? `blur(${blur.toFixed(2)}px)` : undefined,
        }}
      >
        <Camera keys={CAMERA_KEYS} frame={frame}>
          {onSite ? (
            <BrowserChassis>
              <SiteViewport frame={frame} shakeAts={[B.payClick]} />
              <FeedbackFab
                frame={frame}
                hoverAt={B.fabHover}
                pressAt={B.fabClick}
                restAt={B.fabRest}
              />
              {frame >= B.panelAppear ? (
                <WidgetPanel
                  frame={frame}
                  appearAt={B.panelAppear}
                  annotateAt={B.annotate}
                  titleTypeAt={B.titleType}
                  detailsTypeAt={B.detailsType}
                  sendHoverAt={B.sendHover}
                  sendingAt={B.sendClick}
                  successAt={B.success}
                />
              ) : null}
              <CursorLayer
                keys={CURSOR_KEYS}
                clicks={CURSOR_CLICKS}
                frame={frame}
                from={0}
                to={176}
              />
            </BrowserChassis>
          ) : (
            <WindowChassis>
              <TitleBar frame={frame} />
              <ExpandedRail
                frame={frame}
                active="board"
                boardName={CL.project}
                userName={CL.user}
                userInitial={CL.initials}
              />
              <SidebarPane
                title="All Issues"
                actions={<BoardActions />}
                pills
                bottomInset={WIN.dockStrip}
              >
                <BoardTool
                  frame={frame}
                  rows={CL_BOARD}
                  overrides={{
                    [REMOTE_DRAG_ID]: { status: "in_progress" },
                    [LIVE_EDIT_ID]: { assignee: "JL" },
                  }}
                  cascadeAt={B.cascade}
                  insertAt={{ id: NEW_ISSUE_ID, at: B.insert }}
                  showLabels={false}
                />
              </SidebarPane>
              <CenterEmptyState
                frame={frame}
                bottom={WIN.dockStrip}
                contentCenter={{ x: 1000, y: 330 }}
              />
              <DockCollapsedStrip frame={frame} count={1} />
            </WindowChassis>
          )}
        </Camera>
      </AbsoluteFill>

      {(Object.keys(CAPTIONS) as (keyof typeof CAPTIONS)[]).map((key) => (
        <Caption
          key={key}
          frame={frame}
          in={CAPTIONS[key].in}
          out={CAPTIONS[key].out}
          size={captionSize}
          fontFamily={PAGE_FONT}
          letterSpacing="-0.03em"
        >
          {COPY[key]}
        </Caption>
      ))}
    </SegmentShell>
  )
}
