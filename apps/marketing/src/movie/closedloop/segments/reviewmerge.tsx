// closedloop/segments/reviewmerge.tsx — clip 3 (235f, refocused by EXP-388):
// ONE surface, one framing. The rail sits on Reviews, the PR's diff paints
// into the PrDiff center screen (the screen Reviews rows open — the old
// Changes tab is gone), and the two-stage merge runs ON the review row
// (Merge → Confirm merge → Merging…). The phone beside the window shows the
// REAL mobile issue detail: its coding/PR card flips "Ready for review" →
// "Merged" and the status chip lands on Done as the merge completes.
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
  ReviewsTool,
  SidebarPane,
  type MergeState,
} from "../../ships/surfaces/board"
import {
  DockCollapsedStrip,
  ExpandedRail,
  TitleBar,
  type ChromeTab,
} from "../../ships/surfaces/chrome"
import { PrDiffPane } from "../../ships/surfaces/diffview"
import { PhoneChassis } from "../surfaces/steerphone"
import { IssueScreen } from "../surfaces/mobileui"
import {
  CL,
  CL_DIFF_FILES,
  CL_DIFF_ROWS,
  CL_FILE_STATS,
  CL_ISSUE,
  CL_LABELS,
  CL_PR_HEAD,
  CL_REVIEW_ROW,
  COPY,
  NEW_ISSUE_ID,
  PHONE_START,
  REPORT,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP_EASE,
  CONTENT_TOP,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["review-merge"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  statsRoll: 14,
  paint: 18, // the diff paints into the PrDiff screen
  mergeHover: 96,
  confirmAt: 106, // click 1 → Confirm merge
  mergingAt: 120, // click 2 → Merging…
  mergedAt: 138, // the phone card flips Merged · chip lands on Done
  rowFadeFrom: 148,
  rowFadeTo: 162,
} as const

const CAPTIONS = {
  rm1: { in: 12, out: 88 },
  rm2: { in: 126, out: 200 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// ONE framing holds reviews sidebar + diff + phone for the whole clip
// (EXP-388: no camera moves).
const CAMERA_KEYS: CamKey[] = [{ f: 0, s: 1.12, x: 850, y: 470 }]

// ── Cursor ────────────────────────────────────────────────────────────────────
const MERGE_BTN = { x: 641, y: 118 }
const CONFIRM_BTN = { x: 618, y: 118 }

const CURSOR_KEYS: CursorKey[] = [
  { f: 74, x: 900, y: 400 },
  { f: 90, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 104, x: MERGE_BTN.x, y: MERGE_BTN.y },
  { f: 112, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 132, x: CONFIRM_BTN.x, y: CONFIRM_BTN.y },
  { f: 152, x: 780, y: 480 },
]
const CURSOR_CLICKS = [B.confirmAt, B.mergingAt]

const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
}

// Phone placement in COMP coordinates inside the camera layer.
const PHONE_POS = { x: 1490, y: 280, scale: 1 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const ReviewMergeSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const dockH = WIN.dockStrip
  const paneH = WIN.h - CONTENT_TOP - dockH
  const captionSize = Math.round(72 * textScale)

  const heroStatus =
    frame >= B.mergedAt ? ("done" as const) : ("in_progress" as const)

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

  const phoneRise = interpolate(frame, [4, 18], [0, 1], CLAMP_EASE)

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar
              frame={frame}
              tabs={[{ ...TAB_151, status: heroStatus }]}
              activeId="exp151"
            />
            <ExpandedRail
              frame={frame}
              active="reviews"
              boardName={CL.project}
              userName={CL.user}
              userInitial={CL.initials}
            />

            {/* sidebar: reviews — the merge runs on the row */}
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

            {/* center: the PrDiff screen (what a Reviews row opens) */}
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
              <PrDiffPane
                frame={frame}
                paintAt={B.paint}
                statsRollAt={B.statsRoll}
                scrollY={0}
                head={CL_PR_HEAD}
                files={CL_DIFF_FILES}
                rows={CL_DIFF_ROWS}
                fileStats={CL_FILE_STATS}
              />
            </div>

            <DockCollapsedStrip frame={frame} count={2} />

            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={CURSOR_CLICKS}
              frame={frame}
              from={74}
              to={170}
            />
          </WindowChassis>

          {/* the phone: the real mobile issue detail — the merge lands there
              as "Merged" + Done, live */}
          {phoneRise > 0.01 ? (
            <div
              style={{
                position: "absolute",
                left: PHONE_POS.x,
                top: PHONE_POS.y,
                opacity: phoneRise,
                translate: `0px ${(1 - phoneRise) * 46}px`,
              }}
            >
              <div
                style={{
                  transform: `scale(${PHONE_POS.scale})`,
                  transformOrigin: "0 0",
                }}
              >
                <PhoneChassis glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}>
                  <IssueScreen
                    frame={frame}
                    identifier={CL_ISSUE.id}
                    title={CL_ISSUE.title}
                    origin="Feedback widget"
                    status="in_progress"
                    statusLabel="In Progress"
                    priorityLabel="No priority"
                    labelChip={CL_LABELS.widget}
                    description={REPORT.details}
                    pr={{
                      number: CL.pr,
                      device: PHONE_START.device,
                      user: CL.user.split(" ")[0],
                      mergedAt: B.mergedAt,
                    }}
                    sessionLive
                  />
                </PhoneChassis>
              </div>
            </div>
          ) : null}
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
