// #EDE-46: the shell's geometry, read from fixtures/shell-layout.json (its
// $comment says where each number comes from). eDE's documents and the IDE
// take these; the uicore-model crate reads the same file.

import fixture from "../fixtures/shell-layout.json" with { type: "json" }

export interface ShellLayout {
  version: number
  railWidth: number
  /** #EDE-65: the settings nav's column, where the rail is while Settings are open. */
  settingsWidth: number
  taskbarHeight: number
  panelMargin: number
  panelRadius: number
  sidebarRowHeight: number
  sectionLabelHeight: number
  taskbarButtonSize: number
  appletButtonSize: number
  startMenu: { width: number; height: number }
  previewsTile: { width: number; height: number }
  /** #VAPP-273 (C26): the desktop widgets' grid: cells per size class, the cell, the gap, the margin. */
  widgets: { cell: number; gap: number; margin: number; sizes: Record<string, [number, number]> }
}

export const LAYOUT: ShellLayout = fixture

/** A widget's size in px for its class (`s` when unknown). */
export function widgetSize(size: string): { width: number; height: number } {
  const { cell, gap, sizes } = LAYOUT.widgets
  const [cols, rows] = sizes[size] ?? [1, 1]
  return { width: cols * cell + Math.max(cols - 1, 0) * gap, height: rows * cell + Math.max(rows - 1, 0) * gap }
}
