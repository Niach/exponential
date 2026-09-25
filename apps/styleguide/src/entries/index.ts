/**
 * EXP-1029 contract — every entry under the four sections, by fixed export
 * name. A leaf fills ITS file; nobody edits this list (EXP-1019 splices it
 * into the page when it moves the existing entries into the sections).
 */
import { entry as picker } from "./picker.tsx"
import { entry as pickerBoard } from "./picker-board.tsx"
import { entry as pickerIssue } from "./picker-issue.tsx"
import { entry as pickerAction } from "./picker-action.tsx"
import { entry as pickerAccount } from "./picker-account.tsx"
import { entry as pickerDevice } from "./picker-device.tsx"
import { entry as pickerAssignee } from "./picker-assignee.tsx"
import { entry as pickerIcon } from "./picker-icon.tsx"
import { entry as pickerStatus } from "./picker-status.tsx"
import { entry as pickerPriority } from "./picker-priority.tsx"
import { entry as pickerLabel } from "./picker-label.tsx"
import { entry as subShell } from "./sub-shell.tsx"
import { entry as menu } from "./menu.tsx"
import { entry as toast } from "./toast.tsx"
import { entry as composerDialog } from "./composer-dialog.tsx"
import { entry as issueContextMenu } from "./issue-context-menu.tsx"
import { entry as sessionTree } from "./session-tree.tsx"
import { entry as deviceSettings } from "./device-settings.tsx"
import { entry as workflowGraph } from "./workflow-graph.tsx"
import type { StyleguideEntry } from "./types.ts"

export type { StyleguideEntry } from "./types.ts"

export const ENTRIES: readonly StyleguideEntry[] = [
  picker,
  pickerBoard,
  pickerIssue,
  pickerAction,
  pickerAccount,
  pickerDevice,
  pickerAssignee,
  pickerIcon,
  pickerStatus,
  pickerPriority,
  pickerLabel,
  subShell,
  menu,
  toast,
  composerDialog,
  issueContextMenu,
  sessionTree,
  deviceSettings,
  workflowGraph,
]

export function entryById(id: string): StyleguideEntry | undefined {
  return ENTRIES.find((entry) => entry.id === id)
}
