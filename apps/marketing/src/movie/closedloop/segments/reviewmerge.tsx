// closedloop/segments/reviewmerge.tsx — clip 3 (190f): the PR's diff paints
// in the Changes tab, the rail switches to Reviews, the two-stage merge runs
// and the board regroups EXP-151 to Done. All beats are LOCAL frames.

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
  ReviewsTool,
  SidebarPane,
  type MergeState,
} from "../../ships/surfaces/board"
import {
  DockCollapsedStrip,
  IconRail,
  TabsBar,
  TopBar,
  railIconCenter,
  type ChromeTab,
  type IconRailProps,
} from "../../ships/surfaces/chrome"
import { IssueDetailPane } from "../../ships/surfaces/detail"
import { ChangesPane } from "../../ships/surfaces/diffview"
import {
  CL,
  CL_BOARD,
  CL_DIFF_FILES,
  CL_DIFF_HEADER,
  CL_DIFF_ROWS,
  CL_FILE_STATS,
  CL_ISSUE,
  CL_REVIEW_ROW,
  COPY,
  NEW_ISSUE_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP,
  CLAMP_EASE,
  CONTENT_TOP,
  RAIL_IDS,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["review-merge"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  tabSwitch: 8,
  statsRoll: 12,
  paint: 16,
  fileSelect: 62,
  railTransition: 84,
  railClick: 88,
  sidebarSwapOut: 86, // board → reviews crossfade
  mergeHover: 100,
  confirmAt: 106, // click 1 → Confirm merge
  mergingAt: 116, // click 2 → Merging…
  rowFadeFrom: 124,
  rowFadeTo: 134,
  doneAt: 132, // EXP-151 regroups to Done
  regroupEnd: 150,
  sidebarSwapIn: 130, // reviews → board crossfade
} as const

const CAPTIONS = {
  rm1: { in: 10, out: 78 },
  rm2: { in: 108, out: 162 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.55, x: 928, y: 400 },
  { f: 14, s: 1.55, x: 928, y: 400 },
  { f: 80, s: 1.55, x: 928, y: 455, ease: "linear" },
  { f: 84, s: 1.55, x: 928, y: 455 },
  { f: 96, s: 1.7, x: 575, y: 385 },
]

// ── Cursor ────────────────────────────────────────────────────────────────────
const railReviews = railIconCenter("reviews")
const MERGE_BTN = { x: 263, y: 122 }
const CONFIRM_BTN = { x: 238, y: 122 }

const CURSOR_KEYS: CursorKey[] = [
  { f: 74, x: 900, y: 400 },
  { f: 84, x: railReviews.x, y: railReviews.y },
  { f: 92, x: railReviews.x, y: railReviews.y },
  { f: 98, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 104, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 110, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 118, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 130, x: 500, y: 560 },
]
const CURSOR_CLICKS = [B.railClick, B.confirmAt, B.mergingAt]

const TAB_151: ChromeTab = { id: "exp151", label: NEW_ISSUE_ID, mono: true }

// ── The clip ──────────────────────────────────────────────────────────────────
export const ReviewMergeSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = WIN.dockStrip
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const heroStatus =
    frame >= B.doneAt ? ("done" as const) : ("in_progress" as const)
  const overrides = { [NEW_ISSUE_ID]: { status: heroStatus } }
  const regroup =
    frame >= B.doneAt
      ? {
          id: NEW_ISSUE_ID,
          t: interpolate(frame, [B.doneAt, B.regroupEnd], [0, 1], CLAMP),
          from: "in_progress" as const,
        }
      : undefined

  // Sidebar crossfades: board ↔ reviews.
  const boardO =
    frame < (B.sidebarSwapOut + B.sidebarSwapIn) / 2
      ? interpolate(
          frame,
          [B.sidebarSwapOut, B.sidebarSwapOut + 6],
          [1, 0],
          CLAMP
        )
      : interpolate(
          frame,
          [B.sidebarSwapIn, B.sidebarSwapIn + 6],
          [0, 1],
          CLAMP
        )
  const reviewsO =
    interpolate(
      frame,
      [B.sidebarSwapOut, B.sidebarSwapOut + 6],
      [0, 1],
      CLAMP
    ) *
    interpolate(frame, [B.sidebarSwapIn, B.sidebarSwapIn + 6], [1, 0], CLAMP)

  const mergeState: MergeState =
    frame < B.confirmAt
      ? "rest"
      : frame < B.mergingAt
        ? "confirm"
        : frame < B.rowFadeTo
          ? "merging"
          : "gone"
  const mergeMorphAt =
    mergeState === "confirm"
      ? B.confirmAt
      : mergeState === "merging"
        ? B.mergingAt
        : undefined
  const rowFade = interpolate(
    frame,
    [B.rowFadeFrom, B.rowFadeTo],
    [0, 1],
    CLAMP_EASE
  )

  const railProps: IconRailProps =
    frame < B.railTransition
      ? { frame, active: "issues", icons: RAIL_IDS }
      : {
          frame,
          active: "reviews",
          activeTransition: { from: "issues", at: B.railTransition },
          icons: RAIL_IDS,
        }
  const railDots = frame < B.railClick + 4 ? ["reviews"] : []

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TopBar
              frame={frame}
              projectName={CL.project}
              runConfig={CL.runConfig}
            />
            <IconRail {...railProps} dots={railDots} />
            <TabsBar frame={frame} tabs={[TAB_151]} activeId="exp151" />

            {/* sidebar: board */}
            {frame < B.sidebarSwapOut + 8 || frame >= B.sidebarSwapIn ? (
              <div style={{ opacity: boardO }}>
                <SidebarPane
                  title="All Issues"
                  actions={<BoardActions />}
                  pills
                  bottomInset={dockH}
                >
                  <BoardTool
                    frame={frame}
                    rows={CL_BOARD}
                    overrides={overrides}
                    prDotId={{ id: NEW_ISSUE_ID, at: 0 }}
                    regroup={regroup}
                    showLabels={false}
                  />
                </SidebarPane>
              </div>
            ) : null}

            {/* sidebar: reviews */}
            {frame >= B.railTransition && frame < B.sidebarSwapIn + 8 ? (
              <div style={{ opacity: reviewsO }}>
                <SidebarPane title="Reviews" bottomInset={dockH}>
                  <ReviewsTool
                    frame={frame}
                    mergeState={mergeState}
                    morphAt={mergeMorphAt}
                    hover={frame >= B.mergeHover && frame < B.confirmAt}
                    rowFade={rowFade}
                    row={CL_REVIEW_ROW}
                    project={CL.project}
                  />
                </SidebarPane>
              </div>
            ) : null}

            {/* center: issue detail + Changes tab */}
            <div
              style={{
                position: "absolute",
                left: CENTER_X,
                top: CONTENT_TOP,
                width: CENTER_W,
                height: paneH,
                overflow: "hidden",
              }}
            >
              <IssueDetailPane
                frame={frame}
                tab="changes"
                tabSwitchAt={B.tabSwitch}
                prChip={{ at: 0 }}
                status={heroStatus}
                priority="none"
                issue={CL_ISSUE}
                width={CENTER_W}
                height={paneH}
              />
              <div
                style={{
                  position: "absolute",
                  left: 0,
                  top: 34,
                  right: 0,
                  bottom: 0,
                  opacity: interpolate(
                    frame,
                    [B.tabSwitch, B.tabSwitch + 6],
                    [0, 1],
                    CLAMP
                  ),
                }}
              >
                <ChangesPane
                  frame={frame}
                  paintAt={B.paint}
                  statsRollAt={B.statsRoll}
                  fileSelectAt={B.fileSelect}
                  scrollY={0}
                  header={CL_DIFF_HEADER}
                  files={CL_DIFF_FILES}
                  rows={CL_DIFF_ROWS}
                  fileStats={CL_FILE_STATS}
                />
              </div>
            </div>

            <DockCollapsedStrip frame={frame} count={2} />

            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={CURSOR_CLICKS}
              frame={frame}
              from={74}
              to={140}
            />
          </WindowChassis>
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
