// closedloop/segments/platforms.tsx — clip 5 (150f, re-added by EXP-385; the
// EXP-337 reel had dropped the EXP-200/217 ending): the brand finale. The
// Exponential logo stroke-draws next to the wordmark, then all three clients
// rise onto one shelf — the web app in a browser window, the IDE in a MacBook
// (a frozen live composition of the post-story board: EXP-151 shipped) and
// the mobile app in an iPhone — each above its platform icon row (web ·
// mac/windows/linux · iOS/Android). The content fades to the bare canvas
// before the settle so the END_HOLD tail and the loop wrap stay clean.
// All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { C, EASE, PAGE_FONT, WIN } from "../../ships/theme"
import { ExpLogo, WindowChassis } from "../../ships/rig"
import {
  BoardActions,
  BoardTool,
  SidebarPane,
} from "../../ships/surfaces/board"
import {
  DockCollapsedStrip,
  ExpandedRail,
  TitleBar,
  type ChromeTab,
} from "../../ships/surfaces/chrome"
import { IssueDetailPane } from "../../ships/surfaces/detail"
import {
  AndroidIcon,
  AppleIcon,
  GlobeIcon,
  LinuxIcon,
  MacBook,
  PhoneMock,
  PHONE_TOTAL_H,
  WebBrowserMock,
  WindowsIcon,
} from "../surfaces/platformmocks"
import { PHONE } from "../surfaces/steerphone"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  LIVE_EDIT_ID,
  NEW_ISSUE_ID,
  PLATFORMS_COPY,
  REMOTE_DRAG_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CENTER_W,
  CENTER_X,
  CLAMP,
  CONTENT_TOP,
  SegmentShell,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS.platforms

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  logoDrawFrom: 8,
  logoDrawTo: 30,
  brandAt: 10,
  subAt: 20,
  webAt: 24,
  macAt: 30,
  phoneAt: 36,
  iconsAt: 46,
  fadeFrom: 126, // content fades toward the canvas ahead of the settle
  fadeTo: 144,
} as const

const EASED = { ...CLAMP, easing: EASE } as const

const rise = (frame: number, at: number, dur = 10, px = 14) => ({
  opacity: interpolate(frame, [at, at + dur], [0, 1], EASED),
  translate: `0px ${interpolate(frame, [at, at + dur], [px, 0], EASED)}px`,
})

// ── The frozen IDE screen inside the MacBook (post-story board state) ────────
// WindowChassis composes at comp coords (WIN.x/WIN.y); the offset wrapper
// crops the 1920×1080 stage to exactly the 1568×980 window for the bezel.
const FROZEN = 0
const TAB_151: ChromeTab = {
  id: "exp151",
  identifier: NEW_ISSUE_ID,
  label: CL_ISSUE.title,
  status: "done",
}

const MacScreenFrozen: React.FC = () => {
  const dockH = WIN.dockStrip
  const paneH = WIN.h - CONTENT_TOP - dockH
  return (
    <div
      style={{
        position: "absolute",
        left: -WIN.x,
        top: -WIN.y,
        width: 1920,
        height: 1080,
      }}
    >
      <WindowChassis>
        <TitleBar frame={FROZEN} tabs={[TAB_151]} activeId="exp151" />
        <ExpandedRail
          frame={FROZEN}
          active="board"
          boardName={CL.project}
          userName={CL.user}
          userInitial={CL.initials}
        />
        <SidebarPane
          title="All Issues"
          actions={<BoardActions />}
          pills
          bottomInset={dockH}
        >
          <BoardTool
            frame={FROZEN}
            rows={CL_BOARD}
            overrides={{
              [NEW_ISSUE_ID]: { status: "done" },
              [REMOTE_DRAG_ID]: { status: "in_progress" },
              [LIVE_EDIT_ID]: { assignee: "JL" },
            }}
            selectedId={NEW_ISSUE_ID}
            prDotId={{ id: NEW_ISSUE_ID, at: 0 }}
            showLabels={false}
          />
        </SidebarPane>
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
            frame={FROZEN}
            tab="details"
            prChip={{ at: 0 }}
            status="done"
            priority="none"
            issue={CL_ISSUE}
            width={CENTER_W}
            height={paneH}
          />
        </div>
        <DockCollapsedStrip frame={FROZEN} count={2} />
      </WindowChassis>
    </div>
  )
}

// ── The lineup layout (recovered EXP-217 shelf geometry) ─────────────────────
const PHONE_SCALE = 0.78
const COLS = { web: 560, mac: 818, phone: Math.ceil(PHONE.w * PHONE_SCALE) } as const

const IconRow: React.FC<{
  frame: number
  at: number
  icons: React.ReactNode[]
}> = ({ frame, at, icons }) => (
  <div
    style={{
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      gap: 36,
      color: "rgba(255,255,255,0.78)",
    }}
  >
    {icons.map((icon, i) => (
      <div key={i} style={{ ...rise(frame, at + i * 3, 9, 10) }}>
        {icon}
      </div>
    ))}
  </div>
)

// ── The clip ──────────────────────────────────────────────────────────────────
export const PlatformsSegment: React.FC<SegmentProps> = ({ frame }) => {
  const drawT = interpolate(
    frame,
    [B.logoDrawFrom, B.logoDrawTo],
    [0, 1],
    EASED
  )
  const contentO =
    1 - interpolate(frame, [B.fadeFrom, B.fadeTo], [0, 1], EASED)

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill style={{ opacity: contentO, fontFamily: PAGE_FONT }}>
        {/* brand header — logo stroke-draw + wordmark + tagline */}
        <div
          style={{
            position: "absolute",
            top: 56,
            left: 0,
            right: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 18,
            ...rise(frame, B.brandAt, 12, 16),
          }}
        >
          <ExpLogo size={46} drawT={drawT} />
          <span
            style={{
              fontSize: 40,
              fontWeight: 600,
              letterSpacing: "-0.03em",
              color: C.text,
            }}
          >
            {PLATFORMS_COPY.title}
          </span>
        </div>
        <div
          style={{
            position: "absolute",
            top: 122,
            left: 0,
            right: 0,
            textAlign: "center",
            fontSize: 23,
            fontWeight: 500,
            letterSpacing: "-0.02em",
            color: C.muted,
            ...rise(frame, B.subAt, 10, 12),
          }}
        >
          {PLATFORMS_COPY.sub}
        </div>

        {/* the three clients, bottom-aligned on one shelf line */}
        <div
          style={{
            position: "absolute",
            top: 176,
            left: 0,
            right: 0,
            height: 652,
            display: "flex",
            alignItems: "flex-end",
            justifyContent: "center",
            gap: 56,
          }}
        >
          <div
            style={{
              width: COLS.web,
              display: "flex",
              justifyContent: "center",
              ...rise(frame, B.webAt, 12, 22),
            }}
          >
            <WebBrowserMock />
          </div>
          <div
            style={{
              width: COLS.mac,
              display: "flex",
              justifyContent: "center",
              ...rise(frame, B.macAt, 12, 22),
            }}
          >
            <MacBook screenW={700}>
              <MacScreenFrozen />
            </MacBook>
          </div>
          <div
            style={{
              width: COLS.phone,
              display: "flex",
              justifyContent: "center",
              ...rise(frame, B.phoneAt, 12, 22),
            }}
          >
            <div
              style={{
                width: PHONE.w * PHONE_SCALE,
                height: PHONE_TOTAL_H * PHONE_SCALE,
              }}
            >
              <div
                style={{
                  transform: `scale(${PHONE_SCALE})`,
                  transformOrigin: "0 0",
                }}
              >
                <PhoneMock />
              </div>
            </div>
          </div>
        </div>

        {/* per-client platform icon rows on one baseline */}
        <div
          style={{
            position: "absolute",
            top: 856,
            left: 0,
            right: 0,
            display: "flex",
            justifyContent: "center",
            gap: 56,
          }}
        >
          <div style={{ width: COLS.web }}>
            <IconRow
              frame={frame}
              at={B.iconsAt}
              icons={[<GlobeIcon key="web" size={34} />]}
            />
          </div>
          <div style={{ width: COLS.mac }}>
            <IconRow
              frame={frame}
              at={B.iconsAt + 3}
              icons={[
                <AppleIcon key="mac" size={34} />,
                <WindowsIcon key="win" size={32} />,
                <LinuxIcon key="linux" size={34} />,
              ]}
            />
          </div>
          <div style={{ width: COLS.phone }}>
            <IconRow
              frame={frame}
              at={B.iconsAt + 6}
              icons={[
                <AppleIcon key="ios" size={32} />,
                <AndroidIcon key="android" size={34} />,
              ]}
            />
          </div>
        </div>
      </AbsoluteFill>
    </SegmentShell>
  )
}
