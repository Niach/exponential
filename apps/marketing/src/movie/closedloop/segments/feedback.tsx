// closedloop/segments/feedback.tsx — clip 4 (235f, calmed by EXP-388): a
// visitor hits the dead Pay-now button on acme.shop and reports it through
// the embedded feedback widget. ONE framing on the site the whole clip — no
// whip-pan: after "Thanks — sent!" the phone rises with the REAL mobile board
// list, the "New feedback: EXP-151" push drops and the issue row pops into
// Todo — exactly the state clip 1 opens on, so the loop wraps into the live
// board. All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT } from "../../ships/theme"
import {
  Camera,
  Caption,
  CursorLayer,
  type CamKey,
  type CursorKey,
} from "../../ships/rig"
import {
  BrowserChassis,
  FeedbackFab,
  SITE_ANCHORS,
  SiteViewport,
} from "../surfaces/sitemock"
import { WIDGET_ANCHORS, WidgetPanel } from "../surfaces/widgetmock"
import { PhoneChassis } from "../surfaces/steerphone"
import { BoardScreen } from "../surfaces/mobileui"
import {
  CL,
  CL_PHONE_BOARD,
  COPY,
  NEW_ISSUE_ID,
  PUSH_FEEDBACK,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import { CLAMP_EASE, SegmentShell, type SegmentProps } from "./common"

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
  success: 154, // "Thanks — sent!" holds
  phoneIn: 168, // the phone rises with the mobile board
  pushAt: 176, // "New feedback: EXP-151"
  insert: 190, // the EXP-151 row pops into Todo
} as const

const CAPTIONS = {
  fb1: { in: 12, out: 60 },
  fb2: { in: 198, out: 226 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// ONE framing holds checkout + widget + the rising phone (EXP-388: no
// whip-pan, no camera moves).
const CAMERA_KEYS: CamKey[] = [{ f: 0, s: 1.45, x: 1050, y: 600 }]

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

// Phone placement in COMP coordinates inside the camera layer (rises over
// the checkout page, left of the widget panel).
const PHONE_POS = { x: 950, y: 300, scale: 0.95 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const FeedbackSegment: React.FC<SegmentProps> = ({
  frame,
  textScale,
}) => {
  const captionSize = Math.round(72 * textScale)

  const phoneRise = interpolate(
    frame,
    [B.phoneIn, B.phoneIn + 14],
    [0, 1],
    CLAMP_EASE
  )
  const insertT = interpolate(
    frame,
    [B.insert, B.insert + 14],
    [0, 1],
    CLAMP_EASE
  )

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={CAMERA_KEYS} frame={frame}>
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

          {/* the phone: the report lands as a push + a new Todo row on the
              real mobile board */}
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
                  <BoardScreen
                    frame={frame}
                    boardName={CL.project}
                    rows={CL_PHONE_BOARD}
                    insertId={NEW_ISSUE_ID}
                    moveT={insertT}
                    banner={{
                      at: B.pushAt,
                      title: PUSH_FEEDBACK.title,
                      body: PUSH_FEEDBACK.body,
                    }}
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
