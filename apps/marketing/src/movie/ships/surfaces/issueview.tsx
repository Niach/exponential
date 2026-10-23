// surfaces/issueview.tsx — the issue detail as it ships (EXP-1294 issue
// header; shots/issue-detail/desktop.webp + crates/ui issue_detail.rs,
// issue_header.rs, work_header.rs, timeline.rs, comments.rs). Authored in
// PANEL-LOCAL LOGICAL px (render inside shell.tsx CutoutPanel).
//
// Header (WORK_COLUMN_W 896 centred, WORK_GUTTER 20): the 24px semibold
// title with the right cluster on its line — the primary `▷ Start` Lg pill,
// the pin, the `…`; under it the "Threads" row once a thread holds the issue
// (the muted label hanging left of the column, then each thread as a Sm pill:
// its agent mark + name); then the property chips as bare Sm glass pills
// (Status · Priority · Assignee · Labels · Due date · Board · Origin) — no
// tray card, no Start in it. Body: the description (text_base), the emoji /
// image / attach affordances, "+ Add sub-issues", a hairline, "Activity (N)"
// (the synthesized "… created the issue" row first, never counted), the
// comment cards and the bottom "Leave a reply…" composer.

import React from "react"
import { interpolate } from "remotion"
import {
  CalendarDays,
  CircleUser,
  Code,
  Ellipsis,
  Image as ImageIcon,
  MessageSquare,
  Minus,
  Paperclip,
  Pin,
  Play,
  Plus,
  Smile,
  Tag,
} from "lucide-react"
import { EASE, UI_FONT } from "../theme"
import type { IssueStatus } from "../fixtures"
import { G, Ico, StatusGlyph, T } from "./shell"
import { ClaudeMark } from "../../closedloop/surfaces/agentmarks"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const WORK_COLUMN_W = 896
export const WORK_GUTTER = 20

const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: `Backlog`,
  in_progress: `In Progress`,
  in_review: `In Review`,
  done: `Done`,
}

export const SmPill: React.FC<{
  children: React.ReactNode
  color?: string
  weight?: number
}> = ({ children, color = `rgba(250,250,250,0.7)`, weight = 500 }) => (
  <div
    style={{
      height: 24,
      boxSizing: `border-box`,
      display: `flex`,
      alignItems: `center`,
      gap: 4,
      padding: `0 8px`,
      borderRadius: 999,
      border: `1px solid ${G.strokeCard}`,
      backgroundColor: G.fillCard,
      fontSize: T.xs,
      fontWeight: weight,
      color,
      whiteSpace: `nowrap`,
      flex: `none`,
    }}
  >
    {children}
  </div>
)

export type IssueViewContent = {
  title: string
  descriptionParas: readonly string[]
  /** the timeline's first, synthesized row ("Feedback widget") */
  createdBy: string
  createdAgo: string
  comments?: readonly {
    actor: string
    initials: string
    time: string
    body: string
  }[]
  label?: { name: string; dot: string }
  assigneeName?: string
  due?: string
  board: string
  boardColor: string
  /** "Feedback widget" for a widget-filed issue */
  origin?: string
}

/** A markdown image in the description (gpui-markdown-editor
 * image_root_max_height 420: a wide screenshot renders 420 tall). */
export type IssueViewImage = { node: React.ReactNode; w: number; h: number }

export type IssueViewProps = {
  frame: number
  issue: IssueViewContent
  status: IssueStatus
  /** global frame the status chip last changed (a soft pop) */
  statusAt?: number
  /** a thread holding the issue: its name and the global frame it lands */
  thread?: { title: string; at: number }
  image?: IssueViewImage
  width: number
  height: number
}

export const IssueView: React.FC<IssueViewProps> = ({
  frame,
  issue,
  status,
  statusAt,
  thread,
  image,
  width,
  height,
}) => {
  const colX = (width - WORK_COLUMN_W) / 2 + WORK_GUTTER
  const colW = WORK_COLUMN_W - 2 * WORK_GUTTER
  const threadT =
    thread === undefined
      ? 0
      : interpolate(frame, [thread.at, thread.at + 10], [0, 1], {
          ...CLAMP,
          easing: EASE,
        })
  // The Threads row (24 + its gap) pushes the chips and body down.
  const shift = 33 * threadT
  const statusPop =
    statusAt === undefined
      ? 1
      : interpolate(frame, [statusAt, statusAt + 4, statusAt + 10], [1, 1.08, 1], CLAMP)

  return (
    <div
      style={{
        position: `absolute`,
        left: 0,
        top: 0,
        width,
        height,
        overflow: `hidden`,
        fontFamily: UI_FONT,
        color: G.fg,
      }}
    >
      {/* title + the right cluster */}
      <div
        style={{
          position: `absolute`,
          left: colX,
          top: 5,
          width: colW - 150,
          height: 32,
          display: `flex`,
          alignItems: `center`,
          fontSize: 24,
          fontWeight: 600,
          letterSpacing: -0.2,
          whiteSpace: `nowrap`,
          overflow: `hidden`,
          textOverflow: `ellipsis`,
        }}
      >
        {issue.title}
      </div>
      <div
        style={{
          position: `absolute`,
          right: width - (colX + colW) - 8,
          top: 8,
          height: 36,
          display: `flex`,
          alignItems: `center`,
          gap: 12,
        }}
      >
        <div
          style={{
            height: 36,
            boxSizing: `border-box`,
            display: `flex`,
            alignItems: `center`,
            gap: 6,
            padding: `0 14px 0 12px`,
            borderRadius: 999,
            backgroundColor: G.primary,
            color: G.primaryFg,
            fontSize: T.base,
            fontWeight: 500,
          }}
        >
          <Ico I={Play} size={14} />
          Start
        </div>
        <span style={{ display: `flex`, color: G.muted }}>
          <Ico I={Pin} size={13} />
        </span>
        <span style={{ display: `flex`, color: G.muted, width: 24, justifyContent: `center` }}>
          <Ico I={Ellipsis} size={14} />
        </span>
      </div>

      {/* Threads row */}
      {thread !== undefined && threadT > 0 ? (
        <div
          style={{
            position: `absolute`,
            left: colX - 20,
            top: 42,
            height: 24,
            display: `flex`,
            alignItems: `center`,
            gap: 8,
            opacity: threadT,
          }}
        >
          <span style={{ width: 38, fontSize: T.xs, color: G.muted }}>Threads</span>
          <SmPill color={G.fg} weight={600}>
            <span style={{ position: `relative`, display: `flex` }}>
              <ClaudeMark size={12} />
            </span>
            {thread.title}
          </SmPill>
        </div>
      ) : null}

      {/* the property chips (bare Sm pills) */}
      <div
        style={{
          position: `absolute`,
          left: colX,
          top: 52 + shift,
          width: colW,
          display: `flex`,
          flexWrap: `wrap`,
          gap: 8,
        }}
      >
        <div style={{ scale: String(statusPop) }}>
          <SmPill>
            <StatusGlyph status={status} size={12} />
            {STATUS_LABEL[status]}
          </SmPill>
        </div>
        <SmPill>
          <Ico I={Minus} size={12} color={G.muted} />
          No priority
        </SmPill>
        {issue.assigneeName ? (
          <SmPill>
            <Ico I={CircleUser} size={12} />
            {issue.assigneeName}
          </SmPill>
        ) : null}
        {issue.label ? (
          <SmPill>
            <Ico I={Tag} size={12} />
            {issue.label.name}
          </SmPill>
        ) : null}
        {issue.due ? (
          <SmPill>
            <Ico I={CalendarDays} size={12} />
            {issue.due}
          </SmPill>
        ) : null}
        <SmPill>
          <Ico I={Code} size={12} color={issue.boardColor} />
          {issue.board}
        </SmPill>
        {issue.origin ? (
          <SmPill>
            <Ico I={MessageSquare} size={12} />
            {issue.origin}
          </SmPill>
        ) : null}
      </div>

      {/* body */}
      <div
        style={{
          position: `absolute`,
          left: colX,
          top: 100 + shift,
          width: colW,
          display: `flex`,
          flexDirection: `column`,
        }}
      >
        {issue.descriptionParas.map((p) => (
          <p
            key={p.slice(0, 24)}
            style={{ margin: `0 0 10px`, fontSize: T.base, lineHeight: `22px` }}
          >
            {p}
          </p>
        ))}
        {image ? (
          <div
            style={{
              marginTop: 4,
              width: image.w,
              height: image.h,
              borderRadius: 6,
              overflow: `hidden`,
              position: `relative`,
            }}
          >
            {image.node}
          </div>
        ) : null}
        <div
          style={{
            marginTop: 26,
            height: 20,
            display: `flex`,
            alignItems: `center`,
            gap: 18,
            color: G.fg,
            paddingLeft: 1,
          }}
        >
          <Ico I={Smile} size={14} />
          <Ico I={ImageIcon} size={14} />
          <Ico I={Paperclip} size={14} />
        </div>
        <div
          style={{
            marginTop: 4,
            height: 22,
            display: `flex`,
            alignItems: `center`,
            gap: 5,
            fontSize: T.xs,
            color: G.fg,
          }}
        >
          <Ico I={Plus} size={11} />
          Add sub-issues
        </div>
        <div style={{ marginTop: 6, height: 1, backgroundColor: G.border }} />
        <div style={{ marginTop: 14, fontSize: T.sm, fontWeight: 600 }}>
          {`Activity (${issue.comments?.length ?? 0})`}
        </div>
        {/* the synthesized creation row */}
        <div
          style={{
            marginTop: 13,
            height: 20,
            display: `flex`,
            alignItems: `center`,
            fontSize: T.xs,
            color: G.muted,
            whiteSpace: `nowrap`,
          }}
        >
          <span style={{ width: 35, display: `flex`, justifyContent: `center` }}>
            <span style={{ width: 5, height: 5, borderRadius: 999, backgroundColor: G.muted }} />
          </span>
          <span style={{ color: G.fg, fontWeight: 500, marginRight: 3.5 }}>{issue.createdBy}</span>
          {`created the issue · ${issue.createdAgo}`}
        </div>
        {(issue.comments ?? []).map((c) => (
          <div key={c.actor + c.time} style={{ marginTop: 10, display: `flex`, gap: 11 }}>
            <span
              style={{
                width: 24,
                height: 24,
                marginTop: 2,
                flex: `none`,
                borderRadius: 999,
                backgroundColor: `rgba(20,184,166,0.25)`,
                color: `#5eead4`,
                fontSize: 9,
                fontWeight: 600,
                display: `flex`,
                alignItems: `center`,
                justifyContent: `center`,
              }}
            >
              {c.initials}
            </span>
            <div
              style={{
                flex: 1,
                minWidth: 0,
                boxSizing: `border-box`,
                padding: `10px 12px 8px`,
                borderRadius: 16,
                border: `1px solid ${G.strokeCard}`,
                backgroundColor: G.fillCard,
              }}
            >
              <div style={{ display: `flex`, alignItems: `baseline`, gap: 8 }}>
                <span style={{ fontSize: T.sm, fontWeight: 500 }}>{c.actor}</span>
                <span style={{ fontSize: T.xs, color: G.muted }}>{c.time}</span>
              </div>
              <div style={{ marginTop: 4, fontSize: T.sm, lineHeight: `20px` }}>{c.body}</div>
              <div
                style={{
                  marginTop: 8,
                  paddingTop: 7,
                  borderTop: `1px solid ${G.strokeCard}`,
                  fontSize: T.xs,
                  color: G.muted,
                }}
              >
                Leave a reply…
              </div>
            </div>
          </div>
        ))}
        <div
          style={{
            marginTop: 14,
            height: 56,
            boxSizing: `border-box`,
            padding: `11px 12px`,
            borderRadius: 12,
            border: `1px solid ${G.strokeStrong}`,
            backgroundColor: G.fillSection,
            fontSize: T.sm,
            color: G.muted,
          }}
        >
          Leave a reply…
        </div>
      </div>
    </div>
  )
}
