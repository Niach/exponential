// The shared fixture TYPES the closedloop film draws its world with (the world
// itself lives in closedloop/fixtures.ts).

export type IssueStatus = "backlog" | "in_progress" | "in_review" | "done"
export type Priority = "none" | "urgent" | "high" | "medium" | "low"

export type BoardRow = {
  id: string // "EXP-142"
  title: string
  status: IssueStatus
  priority: Priority
  label?: { name: string; dot: string }
  assignee?: string // avatar initials; undefined = no avatar
  due?: string // "Jul 15"
}

export type SteerItem =
  | { kind: "tool"; name: string; summary?: string }
  | { kind: "narration"; text: string }

export type DiffRow = { t: "ctx" | "del" | "add" | "hunk"; text: string; old?: number; new?: number }
