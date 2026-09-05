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
  CutoutPanel,
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
  WEB,
  WebBrowserMock,
  WindowsIcon,
} from "../surfaces/platformmocks"
import { PHONE, PHONE_TOTAL_H, PhoneChassis } from "../surfaces/steerphone"
import { BoardScreen } from "../surfaces/mobileui"
import {
  CL,
  CL_BOARD,
  CL_ISSUE,
  CL_PHONE_BOARD,
  LIVE_EDIT_ID,
  NEW_ISSUE_ID,
  PLATFORMS_COPY,
  REMOTE_DRAG_ID,
} from "../fixtures"
import {
  SEGMENT_DURATIONS,
  STORY_FRAMES,
  WRAP_FADE_FROM,
  WRAP_FADE_TO,
} from "../timeline"
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
// The intro beats sit early (EXP-482): this clip cross-fades in over the
// feedback outro, so its content must be rising DURING the overlap window or
// the dissolve lands on a bare canvas. The outro fade window is DERIVED from
// the timeline's loop-wrap constants — the Reel raises boardlive's f0 under
// exactly this fade, and the two must never drift apart.
const B = {
  logoDrawFrom: 4,
  logoDrawTo: 26,
  brandAt: 4,
  subAt: 12,
  webAt: 16,
  macAt: 22,
  phoneAt: 28,
  iconsAt: 38,
  fadeFrom: DUR - (STORY_FRAMES - WRAP_FADE_FROM), // 126
  fadeTo: DUR - (STORY_FRAMES - WRAP_FADE_TO), // 144
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
  const paneH = WIN.panel.h - dockH
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
        <CutoutPanel>
          <SidebarPane actions={<BoardActions />} bottomInset={dockH}>
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
              status="done"
              priority="none"
              issue={CL_ISSUE}
              width={CENTER_W}
              height={paneH}
            />
          </div>
          <DockCollapsedStrip
            frame={FROZEN}
            tabs={[{ id: "shell", label: "acme-shop", shell: true }]}
            activeTab="shell"
          />
        </CutoutPanel>
      </WindowChassis>
    </div>
  )
}

// ── The lineup layout (recovered EXP-217 shelf geometry) ─────────────────────
const PHONE_SCALE = 0.78
const COLS = { web: 560, mac: 818, phone: Math.ceil(PHONE.w * PHONE_SCALE) } as const

// ── Portrait layout (EXP-482, 1080×1350) ─────────────────────────────────────
// NO camera in either framing — this is a title card laid out in raw comp
// coords, and a camera would only re-raster it (the wide comment below still
// protects the checked-in poster/mp4). Portrait re-authors the horizontal
// shelf as a STACK: brand lockup on top, the MacBook as the hero, then web +
// phone side by side, each device over its platform icon row. Same beats,
// same contentO outro.
const PT = {
  brandTop: 56,
  subTop: 126,
  macTop: 200,
  macScreenW: 640, // MacBook lands ≈ 446 tall at this screen width
  macIconsTop: 672,
  rowTop: 736,
  rowIconsTop: 1128,
  webScale: 0.82, // WebBrowserMock 560×382 → 459×313
  phoneScale: 0.52, // phone 330×678 → 172×353
  gap: 48,
} as const

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
export const PlatformsSegment: React.FC<SegmentProps> = ({
  frame,
  portrait,
}) => {
  const drawT = interpolate(
    frame,
    [B.logoDrawFrom, B.logoDrawTo],
    [0, 1],
    EASED
  )
  const contentO =
    1 - interpolate(frame, [B.fadeFrom, B.fadeTo], [0, 1], EASED)

  const body = (
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
              {/* the real mobile board, frozen post-story */}
              <PhoneChassis>
                <BoardScreen
                  frame={FROZEN}
                  boardName={CL.project}
                  rows={CL_PHONE_BOARD}
                  overrides={{
                    [NEW_ISSUE_ID]: "done",
                    [REMOTE_DRAG_ID]: "in_progress",
                  }}
                  moveT={1}
                />
              </PhoneChassis>
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
  )

  // The frozen post-story mobile board, shared by both framings' phone.
  const frozenPhone = (scale: number) => (
    <div
      style={{
        width: PHONE.w * scale,
        height: PHONE_TOTAL_H * scale,
      }}
    >
      <div
        style={{
          transform: `scale(${scale})`,
          transformOrigin: "0 0",
        }}
      >
        {/* the real mobile board, frozen post-story */}
        <PhoneChassis>
          <BoardScreen
            frame={FROZEN}
            boardName={CL.project}
            rows={CL_PHONE_BOARD}
            overrides={{
              [NEW_ISSUE_ID]: "done",
              [REMOTE_DRAG_ID]: "in_progress",
            }}
            moveT={1}
          />
        </PhoneChassis>
      </div>
    </div>
  )

  const portraitBody = (
    <AbsoluteFill style={{ opacity: contentO, fontFamily: PAGE_FONT }}>
      {/* brand header — logo stroke-draw + wordmark + tagline */}
      <div
        style={{
          position: "absolute",
          top: PT.brandTop,
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
          top: PT.subTop,
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

      {/* the MacBook hero over its platform icon row */}
      <div
        style={{
          position: "absolute",
          top: PT.macTop,
          left: 0,
          right: 0,
          display: "flex",
          justifyContent: "center",
          ...rise(frame, B.macAt, 12, 22),
        }}
      >
        <MacBook screenW={PT.macScreenW}>
          <MacScreenFrozen />
        </MacBook>
      </div>
      <div
        style={{
          position: "absolute",
          top: PT.macIconsTop,
          left: 0,
          right: 0,
        }}
      >
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

      {/* web + phone side by side, bottom-aligned */}
      <div
        style={{
          position: "absolute",
          top: PT.rowTop,
          left: 0,
          right: 0,
          display: "flex",
          alignItems: "flex-end",
          justifyContent: "center",
          gap: PT.gap,
        }}
      >
        <div style={{ ...rise(frame, B.webAt, 12, 22) }}>
          <div
            style={{
              width: WEB.w * PT.webScale,
              height: (WEB.chrome + WEB.viewport) * PT.webScale,
            }}
          >
            <div
              style={{
                transform: `scale(${PT.webScale})`,
                transformOrigin: "0 0",
              }}
            >
              <WebBrowserMock />
            </div>
          </div>
        </div>
        <div style={{ ...rise(frame, B.phoneAt, 12, 22) }}>
          {frozenPhone(PT.phoneScale)}
        </div>
      </div>

      {/* per-client platform icon rows under the pair */}
      <div
        style={{
          position: "absolute",
          top: PT.rowIconsTop,
          left: 0,
          right: 0,
          display: "flex",
          justifyContent: "center",
          gap: PT.gap,
        }}
      >
        <div style={{ width: WEB.w * PT.webScale }}>
          <IconRow
            frame={frame}
            at={B.iconsAt}
            icons={[<GlobeIcon key="web" size={34} />]}
          />
        </div>
        <div style={{ width: PHONE.w * PT.phoneScale }}>
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
  )

  return (
    <SegmentShell frame={frame} dur={DUR}>
      {/* NO camera in either framing — even an identity camera would promote
          a transform layer and shift subpixel AA, enough to move the
          checked-in wide poster and mp4, which must not change. Portrait is
          its own layout instead of a crop. */}
      {portrait ? portraitBody : body}
    </SegmentShell>
  )
}
