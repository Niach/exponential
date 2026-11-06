// #EDE-46: the shell's geometry, read from fixtures/shell-layout.json (its
// $comment says where each number comes from). eDE's documents and the IDE
// take these; the uicore-model crate reads the same file.

import fixture from "../fixtures/shell-layout.json" with { type: "json" }

export interface ShellLayout {
  version: number
  sidebarWidth: number
  railWidth: number
  taskbarHeight: number
  panelMargin: number
  panelRadius: number
  sidebarRowHeight: number
  sectionLabelHeight: number
  taskbarButtonSize: number
  appletButtonSize: number
  startMenu: { width: number; height: number }
  previewsTile: { width: number; height: number }
}

export const LAYOUT: ShellLayout = fixture

/** The sidebar's width for a rail state: the full column or the icon rail. */
export const sidebarWidth = (rail: boolean): number => (rail ? LAYOUT.railWidth : LAYOUT.sidebarWidth)
