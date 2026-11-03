/**
 * VAPP-93 — the styleguide split. The per-platform parity tables of every
 * GENERIC specimen that moved off `apps/styleguide` onto exponential.at/ui:
 * one row per old spec / entry id, its blurb and its four platform cells
 * verbatim, keyed to the core catalog components it maps onto. Plain data
 * (no app, @exp/ui or React imports); `app-parity.test.ts` gates the files,
 * the component names, the ids and the note length.
 */

export type ParityPlatform = `web` | `desktop` | `ios` | `android`
export type ParityStatus = `ok` | `leftover` | `n/a`
export interface ParityCell {
  status: ParityStatus
  symbol?: string
  file?: string
  note?: string
}
export interface AppParity {
  /** the old styleguide spec/entry id */
  id: string
  title: string
  /** the spec's blurb, one paragraph */
  blurb: string
  /** core catalog component names it maps onto (≥1, each in components.generated.json) */
  components: string[]
  status: Record<ParityPlatform, ParityCell>
  leftovers?: { file: string; note: string }[]
}

export const APP_PARITY: readonly AppParity[] = [
  {
    "id": "section-header",
    "title": "Group band",
    "blurb": "The Linear group header — a full-width strip on the section fill, radius 10, padding 6/12, 14/20 at 85% foreground, a trailing slot, 4px over its flat rows. Counts are optional and off by default (the catalog `Section.count`). Never uppercase and never a divider. A band heads a LIST; the bare fold INSIDE a row is the disclosure header below, which draws no strip at all. `loading` spins a 12px glyph at the band's trailing edge while a list REFETCHES (the first load is a skeleton or a spinner, never this); it replaces a Refresh button, which only a list over an external source (GitHub, the local disk or git) keeps.",
    "components": [
      "Section"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassSectionHeader",
        "file": "packages/ui/src/glass-rows.tsx",
        "note": "`loading` = the trailing spinner. Emoji picker category headers stay uppercase on purpose (shared exception)."
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::glass_section_header",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs",
        "note": "a loading flag = the trailing spinner. Emoji picker category headers stay uppercase on purpose (shared exception)."
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSectionBand",
        "file": "apps/ios/ExpUI/Sources/GlassTheme.swift",
        "note": "GlassSectionBand(loading:) = the trailing spinner. Emoji picker category headers stay uppercase on purpose (shared exception)."
      },
      "android": {
        "status": "ok",
        "symbol": "Modifier.glassSectionBand()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/theme/Glass.kt",
        "note": "SectionHeader(loading=) (Scaffolding.kt) wraps it. Emoji picker category headers stay uppercase on purpose (shared exception)."
      }
    }
  },
  {
    "id": "disclosure-header",
    "title": "Disclosure header",
    "blurb": "The fold toggle INSIDE a row, and the whole of it is one line of bare text — a 12px chevron pointing right folded and down open, the label muted and brightening under the pointer, `aria-expanded` stating the fold, the entire line the target. The steer feed's tool groups, its Exponential calls, its subagent lanes and its long bodies, the workflow card's agents and the issue group's own header each drew this by hand before it was one component. `chevron=\"trailing\"` parks the glyph at the far edge instead, for a row whose siblings carry none and must not indent out of line with them. It is NOT the group band above: that is a filled strip heading a LIST. And it may not contain another button — a fold's own action renders beside it, because a button inside a button is invalid markup.",
    "components": [
      "Collapsible"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "DisclosureHeader",
        "file": "packages/ui/src/disclosure-header.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::disclosure_header",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs",
        "note": "the steer feed's tool groups and Exponential runs fold on it (lanes moved into the Work card)"
      },
      "ios": {
        "status": "leftover",
        "symbol": "ToolGroupRow / ExpToolGroupRow",
        "file": "apps/ios/Exponential/UI/Session/AgentSessionView.swift",
        "note": "two private structs repeat the 11pt chevron row, each with its own @State expanded"
      },
      "android": {
        "status": "leftover",
        "symbol": "ToolGroupRow / ExpToolGroupRow",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt",
        "note": "the same two private composables, each rebuilding the chevron row"
      }
    }
  },
  {
    "id": "group",
    "title": "Group container",
    "blurb": "THE settings shell, one per platform — borderless: radius 12, the row fill, a hairline BETWEEN every pair of children, overflow hidden. The fill is the edge, never an outer stroke, and never a card inside a card: inside an overlay that already is a surface (the composer's ⋯ popover, a bottom sheet) the group goes BARE — dividers only, no fill, no radius — so the host's edge is the only edge. Every grouped settings list on every platform draws this; the second specimen is the bare form. Its picker rows are `Picker triggerVariant=\"row\"`, never a Combobox (the Combobox engine is internal to Picker).",
    "components": [
      "Group"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassGroup",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "surface::glass_group / glass_group_rows",
        "file": "apps/desktop/crates/ui/src/surface.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSection",
        "file": "apps/ios/ExpUI/Sources/GlassTheme.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "Modifier.glassGroup()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/theme/Glass.kt",
        "note": "OptionGroup in ui/components/SheetOptionRows.kt is the list wrapper around it."
      }
    }
  },
  {
    "id": "property-row",
    "title": "Property row",
    "blurb": "THE phone properties sheet row (issue detail + create form, ×3 phones), the list row's VALUE variant (was PropertyRow; the catalog `Row` with a `value`): the label left and muted, the value right and readable with its glyph riding beside it as one trailing unit, 44px, the WHOLE row the target (a Button, so it can be a picker's trigger), no chevron. Rows stack inside a `GlassGroup`. A set-valued property (Labels) shows the picks joined by \", \" in the team's order and opens the shared picker sheet, never a cloud of toggle chips; nothing picked reads \"None\".",
    "components": [
      "Row"
    ],
    "status": {
      "web": {
        "status": "leftover",
        "symbol": "PropertyValueRow",
        "file": "apps/web/src/components/issue-editor/mobile-properties.tsx",
        "note": "the value variant is ListRow asChild over a Button; ListRow itself has no value prop (the catalog Row's `value` is the contract)"
      },
      "desktop": {
        "status": "n/a",
        "note": "Desktop shows the properties inline as header chips; no sheet rows."
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassMetaRow",
        "file": "apps/ios/ExpUI/Sources/GlassMetaRow.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "MetaRow",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt"
      }
    }
  },
  {
    "id": "row",
    "title": "Glass row",
    "blurb": "The GAPPED card item (the catalog `Row surface: card`): radius 10, row fill, its own hairline border, padding 12. It is kept for REAL CARDS only (a transcript's tool output, a diff) — never a settings list: the settings ladder is a `GlassSectionHeader` band over `SETTINGS_LIST_CLASS` + the flat list row below.",
    "components": [
      "Row"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassRow",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "surface::glass_row_card",
        "file": "apps/desktop/crates/ui/src/surface.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassRow",
        "file": "apps/ios/ExpUI/Sources/GlassTheme.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "Modifier.glassRow()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/theme/Glass.kt"
      }
    }
  },
  {
    "id": "list-row",
    "title": "List row",
    "blurb": "The flat list item every list wears — no stroke, no fill, radius 10, padding 12, NO gap between rows under a group band; hover takes the row fill, the selected row the active fill. Rows read as a table, not as cards. It has a second density: `compact` is the 28px one-line row the narrow column runs at (the sidebar's pinned and draft arms, the compact inbox) — the same 14px type, 8px of side padding, 8px to the glyph — and `SidebarMenuButton density=\"compact\"` is its exact twin, so a nav entry and a list row sitting in the same 17rem slot are the same height.",
    "components": [
      "Row",
      "Section"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "ListRow",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "surface::flat_row / flat_row_compact",
        "file": "apps/desktop/crates/ui/src/surface.rs",
        "note": "flat_row_compact is the 28px density the rail's entries run at"
      },
      "ios": {
        "status": "ok",
        "symbol": "FlatRow / .flatRow()",
        "file": "apps/ios/ExpUI/Sources/GlassTheme.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "Modifier.flatRow()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/theme/Glass.kt"
      }
    },
    "leftovers": []
  },
  {
    "id": "list-item",
    "title": "List item",
    "blurb": "THE anatomy (fixture `list-item.json`, ×4) every entity row derives from, on a flat list row: [tree guides][lead glyph in a 14px box at x = 12 + 14·depth][mono identifier][title][caption, two-line rows only][trailing meta], which is the catalog `Row` (`depth`, `icon`, `identifier`, `title`, `subtitle`, `meta`). The lead box sits at the same x on every row of a depth, so glyphs and titles align and a child's elbow ends at its own lead. No fold chevron, no trailing chevron, no buttons: a row opens, its menu is a context menu / long-press. Two densities: `compact` (one line, 28-32px: the sidebar's Running, pinned rows) and `list` (36px one-liners, 52px with a caption). Derivations differ only in lead and meta: SessionRow = the session mark + the host device's glyph; an issue row = the status glyph + the assignee; PrRow = the PR ring (a stack swaps the guides for its 1px rail).",
    "components": [
      "Row"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SessionRow / PrRow over ListRow",
        "file": "packages/ui/src/session-row.tsx",
        "note": "PrRow + StackRail in pr-row.tsx; the issue row is apps/web components/issue-list.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "run_rows::run_row",
        "file": "apps/desktop/crates/ui/src/run_rows.rs",
        "note": "over surface::flat_row; pr_rows.rs = PrRow, issue_list.rs = the issue row"
      },
      "ios": {
        "status": "ok",
        "symbol": "SessionRow / PrRow over .flatRow()",
        "file": "apps/ios/ExpUI/Sources/SessionRow.swift",
        "note": "PrRow.swift beside it; caption ExpCore Domain/SessionRow.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "ThreadListRow / PrRow over Modifier.flatRow()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/threads/ThreadsScreen.kt",
        "note": "PrRow.kt beside it; caption domain/SessionTree.kt"
      }
    }
  },
  {
    "id": "tree-guides",
    "title": "Tree guides",
    "blurb": "The connector every NESTED list draws instead of bare indentation. A row used to hang under its parent by left padding alone, so three levels of sessions read as three arbitrary margins. The indent stays 14px per level; on top of it a row at depth d draws, in its PARENT's 14px gutter, a 1px vertical from its top edge to its vertical centre, a rounded elbow (radius 5) and a stub out to the gutter's right edge — and the vertical carries on to the bottom edge when a sibling follows (a tee). Every ancestor level whose subtree continues below draws a straight full-height line, so a deep child stays attached to every level above it. One hairline weight throughout (the strong glass stroke); a parent draws nothing of its own. The RULE is pure and shared ×4 — it reads nothing but the visible rows' depths — so only the painting is per-platform. In the catalog the guides are a PART of `Row` (its `depth`, inside a `Section tree`), filled by the core's post-expansion pass and never authored on their own.",
    "components": [
      "Row"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "TreeGuides / treeGuides",
        "file": "packages/ui/src/tree-guides-view.tsx",
        "note": "the pure rule is tree-guides.ts; drawn as one absolutely positioned SVG inside the row"
      },
      "desktop": {
        "status": "ok",
        "symbol": "Guides / guides_for",
        "file": "apps/desktop/crates/domain/src/tree_guides.rs",
        "note": "domain::tree_guides; the painter is crates/ui/src/tree_guides.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "TreeGuides / TreeGuidesOverlay",
        "file": "apps/ios/ExpCore/Sources/Domain/TreeGuides.swift",
        "note": "the overlay is ExpUI/Sources/TreeGuidesOverlay.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "TreeGuides / TreeGuidesOverlay",
        "file": "apps/android/app/src/main/java/com/exponential/app/domain/TreeGuides.kt",
        "note": "the overlay is ui/components/TreeGuidesOverlay.kt"
      }
    }
  },
  {
    "id": "row-shell",
    "title": "Row shell",
    "blurb": "The rhythm every grouped row inherits: padding 12/16, gap 12, 14px text. The shell never draws a stroke — the group's hairlines do.",
    "components": [
      "Group",
      "Row"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassInputRow / GlassToggleRow / Picker triggerVariant=\"row\"",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::glass_row_shell",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassPickerRow",
        "file": "apps/ios/ExpUI/Sources/GlassOptionRows.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "PickerRow",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt"
      }
    }
  },
  {
    "id": "picker-row",
    "title": "Picker row",
    "blurb": "Label left, value right-aligned at 70% foreground, a 14px chevron at 50%. The whole row is the target, never just the value.",
    "components": [
      "Row"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Picker triggerVariant=\"row\"",
        "file": "packages/ui/src/picker/picker.tsx",
        "note": "the row IS the picker — its own Select and sheet are gone"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::glass_picker_row",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassPickerRow",
        "file": "apps/ios/ExpUI/Sources/GlassOptionRows.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "PickerRow",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt"
      }
    }
  },
  {
    "id": "input-row",
    "title": "Input row",
    "blurb": "A bare right-aligned field at 70% foreground inside the shell — no box, no border. The row is the field's chrome.",
    "components": [
      "Input",
      "Group"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassInputRow",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::glass_input_row",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs"
      },
      "ios": {
        "status": "n/a",
        "note": "Form text rows use the system field inside GlassSection"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassTextField(bordered = false)",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassTextField.kt"
      }
    }
  },
  {
    "id": "toggle-row",
    "title": "Toggle row",
    "blurb": "Label, an optional 12px description at 50%, and a 36×20 switch: on is the primary track with a primary-foreground thumb, off the active fill.",
    "components": [
      "Switch",
      "Group"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassToggleRow",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "surface::glass_toggle_row",
        "file": "apps/desktop/crates/ui/src/surface.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassToggleStyle",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "SwitchRow",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt"
      }
    }
  },
  {
    "id": "tabs-row",
    "title": "Embedded tabs row",
    "blurb": "The segmented control as the FIRST row of a group: padding 8, full width, no fill and no stroke of its own.",
    "components": [
      "Segmented",
      "Group"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassTabsRow",
        "file": "packages/ui/src/glass-rows.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::glass_tabs_row",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSegmentedControl(style: .embedded)",
        "file": "apps/ios/ExpUI/Sources/GlassSegmentedControl.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassSegmentedControl(embedded = true)",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSegmentedControl.kt"
      }
    }
  },
  {
    "id": "segmented",
    "title": "Segmented control",
    "blurb": "The standalone capsule: 36 tall, padding 3, the section fill under a section stroke. Segments share the embedded row's geometry, and that second form is a prop rather than a second component, so the settings strips and the free-floating ones are one control. Eight strips wired their own Tabs + TabsList + N triggers by hand before, and drifted in padding and in whether a segment carried a glyph; the class recipe still lives in tabs.tsx, which other surfaces read directly.",
    "components": [
      "Segmented"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SegmentedControl",
        "file": "packages/ui/src/segmented-control.tsx",
        "note": "the SEGMENTED_* class constants stay in tabs.tsx byte-identical; this renders the strip from an option array"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::segmented",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSegmentedControl",
        "file": "apps/ios/ExpUI/Sources/GlassSegmentedControl.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassSegmentedControl",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSegmentedControl.kt"
      }
    }
  },
  {
    "id": "icon-button",
    "title": "Primary icon button",
    "blurb": "A 32px circle of card fill under a card stroke, glyph 16px at 70% foreground; hover fills to active and the glyph goes full strength. The SHAPE is the meaning: a circle marks the PRIMARY action and nothing else wears one. That is play / start, send, the rail's New issue and Search, a mobile FAB, and the \"+\" that adds. Every other icon-only control is the ghost icon button. The remaining exception is a picker TRIGGER, which is a rounded square: see icon picker.",
    "components": [
      "Button"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "buttonVariants variant=\"glass\" size=\"icon-sm\"",
        "file": "packages/ui/src/button.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::glass_icon_button",
        "file": "apps/desktop/crates/ui/src/controls.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "CircleIconButton",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "CircleIconButton",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/CircleIconButton.kt"
      }
    }
  },
  {
    "id": "ghost-icon-button",
    "title": "Ghost icon button",
    "blurb": "The SECONDARY icon button: the same 32px box and the same 16px glyph at 70% foreground, with no circle, no fill and no border at rest. Hover is the only paint it carries, the row wash under the MD corner, and the glyph goes full strength; a toggle that is ON says so with aria-pressed and keeps that wash (the editor rail's marks). Everything that is not the primary action wears this one: the \"…\" overflow, close, the folder and file-list toggles, the chevrons (back, fold, reorder), trash and remove, refresh. Put a circle here and the surface ends up with three things asking to be pressed and no way to tell which one it wants.",
    "components": [
      "Button"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "buttonVariants variant=\"ghost\" size=\"icon-sm\"",
        "file": "packages/ui/src/button.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::buttons::ghost_icon_button",
        "file": "apps/desktop/crates/eui-gpui/src/controls/buttons.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GhostIconButton",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "CircleIconButton(borderless = true)",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/CircleIconButton.kt",
        "note": "one composable, two shapes: borderless drops the circle and the stroke and keeps the hover fill"
      }
    }
  },
  {
    "id": "button-primary",
    "title": "Primary submit",
    "blurb": "Full width, padding 14/16, radius 10, solid primary. Disabled drops to card fill with a card stroke and 50% foreground. ONE shape on every client — the specimen is the web's own Button, a radius-10 rectangle like the mobile sheet submit and desktop web_md. A text button is never a capsule; the capsule is the pill.",
    "components": [
      "Button"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Button (variant default)",
        "file": "packages/ui/src/button.tsx",
        "note": "the same radius-10 rectangle on every client; the mobile sheet submit is the full-width form of it"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::pills::glass_pill_button_primary",
        "file": "apps/desktop/crates/eui-gpui/src/controls/pills.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSubmitButton",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassSubmitButton",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSubmitButton.kt"
      }
    }
  },
  {
    "id": "text-button",
    "title": "Text button",
    "blurb": "A control made of WORDS — `size=\"inline\"`, 12px, no box, no height of its own, sitting in the run of muted text around it. Two variants, and the difference is what happens when it is pressed: `text` is muted, brightens under the pointer and never underlines, because it toggles something IN PLACE (a fold's Show more / Show less, \"Back to the current step\"); `link` takes the primary colour and underlines on hover, because it GOES somewhere (a session band's \"Continues in a newer session\", a stack band's `↓ #APP-14`). Anything that wants a box is the pill or the primary submit — four call sites hand-drew one of these two shapes before.",
    "components": [
      "Button"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Button variant=\"text\" / variant=\"link\", size=\"inline\"",
        "file": "packages/ui/src/button.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::text_button (TextButtonVariant::Text)",
        "file": "apps/desktop/crates/ui/src/controls.rs",
        "note": "the output card's and the body's Show more, the edited-files footer ride it"
      },
      "ios": {
        "status": "leftover",
        "symbol": "Button(\"Show more\").buttonStyle(.plain)",
        "file": "apps/ios/Exponential/UI/Session/AgentSessionView.swift",
        "note": "inlined twice with its own caption2 font and tertiary opacity; no shared text button exists"
      },
      "android": {
        "status": "leftover",
        "symbol": "ShowMoreToggle",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt",
        "note": "a private clickable Text on the session screen; nothing else may reach it"
      }
    }
  },
  {
    "id": "pill",
    "title": "Pill",
    "blurb": "The ONE capsule, a 2×3 matrix: size md 32 or sm 24, mode action / select / readonly, plus a primary PAINT flag that crosses all six. Card fill under a card stroke, label at 70% — action and select go active on hover, a selected one also takes the active stroke, readonly is metadata and never a target. There is no chip and no header button: those WERE this, under a second name. A conversation or subagent tab is sm select; a members-list role chip is sm readonly, 12px from its neighbours in a row. A bare COUNT is none of the six: a number with no word beside it is the 16px `Badge` below.",
    "components": [
      "Chip"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Pill",
        "file": "packages/ui/src/pill.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::pills::glass_pill",
        "file": "apps/desktop/crates/eui-gpui/src/controls/pills.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassPill",
        "file": "apps/ios/ExpUI/Sources/GlassPill.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassPill",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassPill.kt"
      }
    }
  },
  {
    "id": "badge",
    "title": "Count badge",
    "blurb": "The smallest chip there is — a 16px capsule carrying a NUMBER and nothing else, 10px semibold and tabular so a count can climb without the box twitching. `muted` is a count you parked (the rail's drafts), `primary` one that wants you (unread). Zero renders NOTHING, because a badge is a signal and an empty signal is noise, and past `max` it reads `99+`. PLACEMENT stays at the call site — a row's trailing edge, a nav glyph's corner — so the badge owns only its shape. A `Pill size=\"sm\"` is 24 tall and carries a word; this carries a quantity.",
    "components": [
      "Badge"
    ],
    "status": {
      "web": {
        "status": "n/a",
        "note": "the caller-less web Badge was deleted in a UI cleanup; no web count capsule today"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::pills::count_badge",
        "file": "apps/desktop/crates/eui-gpui/src/controls/pills.rs",
        "note": "RailBadge::Count hangs it on the rail's Drafts entry"
      },
      "ios": {
        "status": "leftover",
        "symbol": "GlassSegmentedControl",
        "file": "apps/ios/ExpUI/Sources/GlassSegmentedControl.swift",
        "note": "the one count capsule is inlined in a segment; the tab bar's unread mark is a FloatingBarBadgeDot"
      },
      "android": {
        "status": "leftover",
        "symbol": "GlassSegmentedControl",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSegmentedControl.kt",
        "note": "the same inline capsule (BadgeFill) inside a segment, reachable by nothing else"
      }
    }
  },
  {
    "id": "entity-chip",
    "title": "Entity chip",
    "blurb": "The issue chip's box, opened to every kind an Exponential MCP answer can name: a settled tool row in a session transcript draws ONE chip per entity it touched — board, action, comment, session, label, status, workflow, device, member… — as a glyph and a short label in the SAME rounded rect the issue chip owns (the issue chip now renders THROUGH it, so the two cannot drift by a pixel). An issue keeps its three parts (status glyph · mono identifier · title); every other kind draws its icon CONCEPT (`entityRefIcon`) and its name; a LIST answer folds into one chip that counts its members in the product noun (`3 issues`, `1 session`) and never navigates — its card lists them. Label, detail and grouping are the contract's (`@exp/domain-contract/entity-preview`, fixture-locked ×4). A row the viewer has not synced draws the same chip muted with no target and no card.",
    "components": [
      "Chip"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "EntityChip",
        "file": "packages/ui/src/entity-chip.tsx",
        "note": "The app's components/entity-preview/ resolves the row, the target and the hover card per kind."
      },
      "desktop": {
        "status": "ok",
        "symbol": "entity_chip",
        "file": "apps/desktop/crates/ui/src/entity_chip.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "EntityChip",
        "file": "apps/ios/ExpUI/Sources/EntityChip.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "EntityChip",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/EntityChip.kt"
      }
    }
  },
  {
    "id": "avatar",
    "title": "Avatar",
    "blurb": "Picture first: a circle filled edge to edge by the person's image. Without one the initials sit on THEIR hue — one of eight token colours picked by fnv1a32(utf8(userId)) % 8 — as a 20% fill under the glyph at full strength, no stroke. The hash is byte-identical on all four clients, so one person is one colour everywhere; a subject with no id at all (a bot, an unresolved reporter) keeps the muted fallback.",
    "components": [
      "Avatar"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "UserAvatar",
        "file": "packages/ui/src/user-avatar.tsx",
        "note": "AvatarFallback (./avatar.tsx) paints the hue; UserAvatar is the composition every site renders."
      },
      "desktop": {
        "status": "ok",
        "symbol": "user_avatar::avatar_element",
        "file": "apps/desktop/crates/ui/src/user_avatar.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "UserAvatar",
        "file": "apps/ios/ExpUI/Sources/UserAvatar.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "UserAvatar",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/Avatars.kt"
      }
    }
  },
  {
    "id": "text-field",
    "title": "Text field",
    "blurb": "36 tall, padding 0/12, radius 12, card fill under a card stroke; focus swaps the stroke to active — no ring. Placeholder at 50%.",
    "components": [
      "Input"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Input",
        "file": "packages/ui/src/input.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::inputs::glass_input",
        "file": "apps/desktop/crates/eui-gpui/src/controls/inputs.rs",
        "note": "focus swaps the stroke to strokeActive, no ring; the corner is radius.lg"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassTextField",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassTextField",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassTextField.kt"
      }
    }
  },
  {
    "id": "textarea",
    "title": "Text area",
    "blurb": "The field's own recipe, grown: radius 12, card fill under a card stroke, focus swaps the stroke to active — no ring. Padding 8/12, three rows tall, and it GROWS with content; the drag handle is off everywhere. Inside a group it goes borderless, because the row is already the chrome.",
    "components": [
      "Textarea"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Textarea",
        "file": "packages/ui/src/textarea.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::inputs::web_textarea",
        "file": "apps/desktop/crates/eui-gpui/src/controls/inputs.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassTextField(lines:)",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassTextField(minLines/maxLines)",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassTextField.kt"
      }
    }
  },
  {
    "id": "glass-card",
    "title": "Glass card",
    "blurb": "The ONE translucent card, and only its box: radius XL, the card hairline, the glass card fill. Everything a card owns on its own stays at the call site, because those genuinely differ — its padding, a blur, a shadow, and the divide-y + overflow hidden that turns the same box into a GROUP of rows. Five surfaces painted it by copy-paste (the comment row, the agent AskCard, the usage card, the mobile issue properties sheet and the repo picker, which had drifted to the MD corner and the bare hairline). The shadcn Card derives its recipe from the same constant, so the two cannot disagree; a group of list ROWS is the group container instead, on the row fill with no outer stroke.",
    "components": [
      "Card"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "GlassCard / GLASS_CARD_CLASS",
        "file": "packages/ui/src/glass-card.tsx",
        "note": "Card (./card.tsx) derives its own recipe from the same constant."
      },
      "desktop": {
        "status": "ok",
        "symbol": "surface::glass_card",
        "file": "apps/desktop/crates/ui/src/surface.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassCard",
        "file": "apps/ios/ExpUI/Sources/GlassTheme.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "Modifier.glassCard()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/theme/Glass.kt"
      }
    }
  },
  {
    "id": "sheet",
    "title": "Sheet shell",
    "blurb": "Top radius 24 over the page's bottom gradient, a card hairline, a 36×4 grabber. Header gutter 20, content gutter 16. Dismissal is the grabber drag or the backdrop — the header's trailing slot holds an optional ACTION, never a Cancel — and the bottom carries exactly one primary.",
    "components": [
      "Drawer"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SheetContent side=\"bottom\"",
        "file": "packages/ui/src/sheet.tsx"
      },
      "desktop": {
        "status": "n/a",
        "note": "dialogs are OS windows"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSheetChrome + GlassSheetTokens",
        "file": "apps/ios/ExpUI/Sources/GlassSheet.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassSheet + GlassSheetDefaults",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSheet.kt"
      }
    }
  },
  {
    "id": "composer",
    "title": "Composer",
    "blurb": "ONE composer for comments, steering and reporter replies: a radius-16 card of card fill under a card hairline, holding an optional attachment strip, a borderless 36-min field and a tool row of 24px ghost glyph buttons with a right-aligned submit whose glyph is the primary tint. The `inline` arm is the steer card — the round submit rides the field's own row instead of a tool row under it. The opaque variant swaps to the opaque card fill and the strong stroke — it floats over a feed on mobile, and an alpha fill there shows the conversation through it. It lives in @exp/ui: the card owns CHROME AND LAYOUT only, and every caller keeps its own field, upload and send.",
    "components": [
      "Composer"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Composer / ComposerTool / ComposerSubmit",
        "file": "packages/ui/src/composer.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "composer::glass_composer",
        "file": "apps/desktop/crates/ui/src/composer.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassComposer",
        "file": "apps/ios/ExpUI/Sources/GlassComposer.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassComposer",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassComposer.kt"
      }
    }
  },
  {
    "id": "meter",
    "title": "Meter",
    "blurb": "The ONE bar every usage surface draws — the rate-limit windows, the session's context, the mini line. A capsule track in the strong stroke with a capsule fill, and exactly three tones: foreground at 30% normally, the yellow semantic from 75%, the destructive from 95%. The height is the caller's (6px full, 4px mini); the tone is the only decision. Before it there were two bars two rows apart — a bare Progress with the primary fill, and a hand-rolled span with its own tone map — reading the same percent in different colours.",
    "components": [
      "Meter"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Meter",
        "file": "packages/ui/src/meter.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "usage_bar::meter",
        "file": "apps/desktop/crates/ui/src/usage_bar.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "AgentUsageTrack",
        "file": "apps/ios/ExpUI/Sources/UsageTrack.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "UsageTrack",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/UsageTrack.kt"
      }
    }
  },
  {
    "id": "segmented-bar",
    "title": "Segmented bar",
    "blurb": "The context window's breakdown — the Meter's track, filled left to right by one slice per layer the session's context_layout named (base, tools, playbook, team prompt, project, task) and the derived conversation, in the contract's tones, clipped at 100% and never rescaled; free space is the bare track. Three hairline ticks mark the compaction floor (50%) and the two usage thresholds (75%, 95%). The legend swatch beside each row reuses the slice's tone, so a square and its slice cannot disagree.",
    "components": [
      "Meter"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SegmentedBar",
        "file": "packages/ui/src/segmented-bar.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "render_context_window",
        "file": "apps/desktop/crates/ui/src/usage_sheet.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "SegmentedTrack",
        "file": "apps/ios/ExpUI/Sources/SegmentedTrack.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "SegmentedTrack",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SegmentedTrack.kt"
      }
    }
  },
  {
    "id": "divider",
    "title": "Hairline divider",
    "blurb": "One pixel of the row stroke. The only rule allowed inside a group, and the only one anywhere in the glass set.",
    "components": [
      "Separator"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Separator",
        "file": "packages/ui/src/separator.tsx",
        "note": "Inside a group the same hairline comes from GlassGroup's divide-y, not from a Separator element."
      },
      "desktop": {
        "status": "ok",
        "symbol": "surface::glass_row_divider",
        "file": "apps/desktop/crates/ui/src/surface.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassDivider",
        "file": "apps/ios/ExpUI/Sources/GlassTheme.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GroupDivider",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt"
      }
    }
  },
  {
    "id": "checkbox",
    "title": "Checkbox",
    "blurb": "THE check ×4 (owner 2026-10-27: every check and selection mark is this square): 16px box, 1px glass stroke-strong over the row fill, 4px corner; checked = the primary fill under the primary-foreground ui-check (indeterminate: ui-minus). It holds a table's selection (the issue list's bulk-select column on every form factor), a multi-select ask/plan choice and a markdown task item; the chat list's select badge is the same look at 18px.",
    "components": [
      "Checkbox"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Checkbox",
        "file": "packages/ui/src/checkbox.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::inputs::checkbox",
        "file": "apps/desktop/crates/eui-gpui/src/controls/inputs.rs",
        "note": "glass box (row fill, strong stroke, 4px corner), primary when checked, ui-check / ui-minus; bulk-select, task items, multi choices"
      },
      "ios": {
        "status": "ok",
        "symbol": "ExpCheckbox",
        "file": "apps/ios/ExpUI/Sources/ExpCheckbox.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "ExpCheckbox",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/ExpCheckbox.kt"
      }
    }
  },
  {
    "id": "switch",
    "title": "Switch",
    "blurb": "The 36×20 capsule that flips a setting the moment it is pressed — there is no Save beside one. Off is the active fill under the foreground knob, on the primary fill under the primary-foreground knob, and the travel is one fast duration. It almost always rides a toggle ROW, which owns the label and the description; this is the bare control.",
    "components": [
      "Switch"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Switch",
        "file": "packages/ui/src/switch.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::buttons::web_switch",
        "file": "apps/desktop/crates/eui-gpui/src/controls/buttons.rs",
        "note": "a lint (only_controls_constructs_switches) refuses Switch::new anywhere else"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassToggleStyle",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift",
        "note": "applied app-wide as .toggleStyle(.glass), so call sites keep the stock Toggle"
      },
      "android": {
        "status": "ok",
        "symbol": "glassSwitchColors()",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt",
        "note": "the tokens for the stock M3 Switch; there is no GlassSwitch composable"
      }
    }
  },
  {
    "id": "select",
    "title": "Select",
    "blurb": "The closed single-select: a field-height trigger of card fill under a card hairline, the value left and a chevron right, focus swapping the stroke to active. Only the PLACEHOLDER arm can be photographed — SelectValue resolves against items that live inside the portalled list, so a valued trigger renders empty until the browser opens it (see @exp/ui's island limits). On the natives there is no free-standing select at all: the closed trigger is always a picker ROW opening a sheet.",
    "components": [
      "Select"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Select / SelectTrigger",
        "file": "packages/ui/src/select.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::rows::glass_picker_select",
        "file": "apps/desktop/crates/eui-gpui/src/controls/rows.rs"
      },
      "ios": {
        "status": "n/a",
        "note": "no free-standing select: the closed single-select is a GlassPickerRow opening a sheet (see picker row)"
      },
      "android": {
        "status": "n/a",
        "note": "same as iOS — PickerRow (SheetOptionRows.kt) is the closed arm, and the sheet is the list"
      }
    }
  },
  {
    "id": "label",
    "title": "Field label",
    "blurb": "The 14px medium line that names a field, tied to it by htmlFor so the label is part of the hit box, and dimmed with the field when it is disabled. It exists on the WEB only: every native lays a field out as a ROW that already carries its name on the left, so a label above the control there would say the same thing twice.",
    "components": [
      "Input"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Label",
        "file": "packages/ui/src/label.tsx"
      },
      "desktop": {
        "status": "n/a",
        "note": "a field's name is the glass row's own leading text — see input row"
      },
      "ios": {
        "status": "n/a",
        "note": "same — the name is the row title inside GlassTextField / GlassPickerRow"
      },
      "android": {
        "status": "n/a",
        "note": "same — TextFieldRow / PickerRow carry the label themselves"
      }
    }
  },
  {
    "id": "empty-state",
    "title": "Empty state",
    "blurb": "ONE empty state, two sizes (copy ×4 = `empty-state.json`). `page` = a whole screen with nothing: the 48px icon disc, a semibold title, an optional muted sentence that teaches the next step and an optional actions row, centred on at most 28rem, 48px vertical padding. `list` = inside a section, a sidebar or a picker: a bare 20px muted glyph over ONE muted line, 24px vertical padding, never a description or a button (the band's trailing pill is the call to action). A title is at most four words (`No <noun> yet`, `No <noun>`, `All caught up`) and never explains. A first load is a skeleton or a spinner, a refetch the band's `loading` glyph, and no list over synced or server data carries a Refresh button. `EmptyCta` (next entry) is the dashed box that STARTS a list.",
    "components": [
      "EmptyState"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "EmptyState",
        "file": "packages/ui/src/empty-state.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::feedback::empty_state",
        "file": "apps/desktop/crates/eui-gpui/src/controls/feedback.rs",
        "note": "empty_state_list = the list size"
      },
      "ios": {
        "status": "ok",
        "symbol": "EmptyState",
        "file": "apps/ios/ExpUI/Sources/EmptyState.swift",
        "note": "built on the SDK EmptyStateView; size .page / .list from EmptyStateCopy"
      },
      "android": {
        "status": "ok",
        "symbol": "EmptyState",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/Scaffolding.kt",
        "note": "title/description + the 48dp disc; size = EmptyStateSize.Page / List"
      }
    }
  },
  {
    "id": "empty-cta",
    "title": "Empty call to action",
    "blurb": "The third empty, and the only one that is a BUTTON. A dashed, full-width box standing exactly where the first row will go — a 16px glyph, one title line, one muted sentence under it, the row wash and full-strength text on hover. Dashed because it is a placeholder for the row it invites; clickable because the shortest path to that row is the box itself. `EmptyState` teaches a PAGE with nothing on it, `ListEmpty` reports a list that filtered down to nothing, and this one STARTS a list: the actions panel's \"describe one\" nudge is the call site it was cut from.",
    "components": [
      "EmptyState"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "EmptyCta",
        "file": "packages/ui/src/empty-state.tsx",
        "note": "EmptyState and ListEmpty are the other two, in the same file"
      },
      "desktop": {
        "status": "n/a",
        "note": "the actions panel's nudge went with actions; no desktop call site renders this box yet"
      },
      "ios": {
        "status": "n/a",
        "note": "the creator session needs a device: ActionsListView.emptyState is a read-only page empty instead"
      },
      "android": {
        "status": "n/a",
        "note": "same: ActionsScreen's ActionsEmptyState reads, it does not invite — creation lives on web or desktop"
      }
    }
  },
  {
    "id": "skeleton",
    "title": "Skeleton",
    "blurb": "Skeleton ROWS (feedback-states.json `placeholder`) in the real row geometry stand in for a list that is loading: the row's height, a dot where its glyph sits, bars where its text sits, hairlines between rows, one slow opacity pulse (0.45 to 0.9 over 1.6 s), static under reduce motion. It replaces every Loading text and the phones' centred first-load spinners. The catalog's `Skeleton rows` is the same piece.",
    "components": [
      "Skeleton"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SkeletonRows",
        "file": "packages/ui/src/skeleton.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "feedback::skeleton_rows",
        "file": "apps/desktop/crates/ui/src/feedback.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "SkeletonRows",
        "file": "apps/ios/ExpUI/Sources/Feedback/Spinner.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "SkeletonRows",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/Feedback.kt"
      }
    }
  },
  {
    "id": "feedback-inline",
    "title": "Inline spinner",
    "blurb": "A 10 px spinner (feedback-states.json `inline`) and a progressive word in a band, header or caption (Saving, then Saved). The list-head spinner of a refetching band is this primitive. The catalog's `Spinner size xs`.",
    "components": [
      "Spinner",
      "Section"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Spinner",
        "file": "packages/ui/src/feedback.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "feedback::spinner",
        "file": "apps/desktop/crates/ui/src/feedback.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "InlineSpinner",
        "file": "apps/ios/ExpUI/Sources/Feedback/Spinner.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "InlineSpinner",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/Feedback.kt"
      }
    }
  },
  {
    "id": "feedback-pending",
    "title": "Pending button",
    "blurb": "A button that started something refuses presses at once, then after 150 ms swaps its icon slot for the spinner and its label for the progressive word (Starting), never narrower than it was. The catalog's `Button loading` + `pendingLabel`.",
    "components": [
      "Button"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Button pending + pendingLabel",
        "file": "packages/ui/src/button.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "feedback::pending (the 150 ms gate)",
        "file": "apps/desktop/crates/ui/src/feedback.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "PendingLabel",
        "file": "apps/ios/ExpUI/Sources/Feedback/Spinner.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "PendingPill",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/Feedback.kt"
      }
    }
  },
  {
    "id": "searchable-list",
    "title": "Search list",
    "blurb": "THE search list (search-list.json). An input row (40 px, the search glyph, the result count once a query is typed) over the picker rows, at most eight visible, the rest scrolling under a 24 px bottom fade. The top row is highlighted, arrows move it, Enter or Tab pick it. It is the `#` typeahead, the issue pickers, the global search and the composer menu's searchable submenus. The catalog's `SearchList` and `menuItem.searchable`.",
    "components": [
      "SearchList",
      "Menu"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SearchList / SearchListPopover",
        "file": "packages/ui/src/search-list.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "picker::search_list::SearchList",
        "file": "apps/desktop/crates/ui/src/picker/search_list.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "SearchListView / SearchListSheet",
        "file": "apps/ios/ExpUI/Sources/Picker/SearchList.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "SearchList",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/SearchList.kt"
      }
    }
  },
  {
    "id": "tab-strip",
    "title": "Tab strip",
    "blurb": "The work tabs. The close glyph shows only while its tab is hovered or keyboard-focused (the active one too); a ghost + after the last tab opens a New session tab. The catalog's `Tabs closable: hover` + `addable`.",
    "components": [
      "Tabs"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "WorkTabsStrip",
        "file": "apps/web/src/components/team/work-tabs-strip.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "render_tab_strip",
        "file": "apps/desktop/crates/ui/src/screens.rs"
      },
      "ios": {
        "status": "n/a",
        "note": "phones have no tab strip: one Work screen with pager tabs"
      },
      "android": {
        "status": "n/a",
        "note": "same: one Work screen with pager tabs"
      }
    }
  },
  {
    "id": "dialog",
    "title": "Dialog",
    "blurb": "The centred modal: a radius-16 card on the OPAQUE card fill under a card hairline, a semibold title, one line of body, and a footer whose LAST button is the primary. Cancel is borderless — two boxed buttons side by side ask the reader to choose between two equals. On a phone the same component drops to the bottom sheet arm. A confirm or a choice is never this shell: it is the Prompt. Hand-written here because a closed Radix portal renders nothing at all statically (PORTAL_ONLY_IDS).",
    "components": [
      "Dialog"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Dialog / DialogContent",
        "file": "packages/ui/src/dialog.tsx",
        "note": "a confirm or a choice is the Prompt entry, never this shell"
      },
      "desktop": {
        "status": "ok",
        "symbol": "native_dialog::DialogShell",
        "file": "apps/desktop/crates/ui/src/native_dialog.rs",
        "note": "every IDE dialog is a real OS window, not an in-window overlay"
      },
      "ios": {
        "status": "n/a",
        "note": "no shared shell: a confirm is GlassAlert (the Prompt entry), a content dialog is the sheet"
      },
      "android": {
        "status": "n/a",
        "note": "same: a confirm is GlassAlert (the Prompt entry), a content dialog is GlassSheet"
      }
    }
  },
  {
    "id": "prompt",
    "title": "Prompt",
    "blurb": "THE confirm and choice prompt: every \"Delete X?\", merge confirm and multi-answer choice on the web is this one card, the same on every width, its wording read from the contract fixture `prompts.json` (web mirror `apps/web/src/lib/prompts.ts`, the same strings on iOS and Android). A centred card over the dimmed scrim, no ✕ (scrim tap and Esc are its cancel path); ONE question as the title; a body line only for a fact the title cannot carry (what is deleted, a name, a count, a consequence for other people), never a restatement of the buttons; an optional content slot between the text and the row (the blocked-start graph, an input); ONE row of the 32px `md` Pill capsules; only when that row cannot fit do the pills stack, one per line at their own width, trailing-aligned, in reverse display order: the default on top, Cancel below it, a quiet destructive answer last (iOS `GlassAlertLayout.stacked`); never a two-row hybrid. Roles: `cancel` = the plain pill that only dismisses; `primary` = the safe/expected answer, the accent pill at the trailing edge; `default` = a plain pill that does something; `destructive` = the answer of a plain \"Delete X?\" confirm, the plain pill with a destructive label and tinted border next to Cancel (no primary, Cancel takes focus); `quietDestructive` = destructive TEXT set apart on the leading edge when a safe primary exists too. No solid red blocks, no full-width buttons; initial focus and Enter never land on a destructive answer; a busy answer keeps its label and shows the pill's spinner while the row and the dismiss paths lock. The natives draw the same card (GlassAlert); the IDE asks in native alert windows with the same words, roles and focus (`AlertSpec::from_prompt`).",
    "components": [
      "Dialog"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Prompt / PromptLayout",
        "file": "packages/ui/src/prompt.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "native_dialog::AlertSpec::from_prompt",
        "file": "apps/desktop/crates/ui/src/native_dialog.rs",
        "note": "the fixture's words, roles and focus (domain prompts.rs) in a native alert window"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassAlert",
        "file": "apps/ios/ExpUI/Sources/GlassAlert.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassAlert",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassAlert.kt"
      }
    }
  },
  {
    "id": "picker-surface",
    "title": "Picker surface",
    "blurb": "The searchable SURFACE every Picker opens (Picker = the only public picker; its Combobox engine is internal): a popover on a pointer, the bottom sheet on a phone. Selection speaks ONE language: SINGLE marks the picked row with a trailing ui-check, MULTI by the row's own highlight wash (data-picked), and a bulk edit over rows that disagree marks the row mixed; never a checkbox, never a circle pair. `value` is the identity and `keywords` the search text, so two boards may share a name; `noneLabel` renders a row that reports null. Single closes on pick, multi stays open. PickerItem is the row shape; the trigger comes in four `triggerVariant`s, pill | field | row | inline (row = the form ladder's picker row, the catalog `Row chevron: selector`). PickerMenuRows draws the same rows inside a Menu.",
    "components": [
      "Select",
      "Row"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Picker / PickerList",
        "file": "packages/ui/src/picker/picker.tsx",
        "note": "PickerItem is the row shape; PickerList the body without the popover; PickerMenuRows the rows inside a menu"
      },
      "desktop": {
        "status": "ok",
        "symbol": "picker::Picker",
        "file": "apps/desktop/crates/ui/src/picker/mod.rs",
        "note": "searchable_picker is retired; the primitive owns its query + cursor, so a host holds no picker entities"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassPicker",
        "file": "apps/ios/ExpUI/Sources/Picker/Picker.swift",
        "note": "GlassPickerSheet is retired: every pick outside the typed subjects goes through GlassPicker"
      },
      "android": {
        "status": "leftover",
        "symbol": "GlassSheetRow",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSheet.kt",
        "note": "the generic picker is Picker, its own entry; this row is what the sheets outside the ten assemble"
      }
    }
  },
  {
    "id": "search-field",
    "title": "Search field",
    "blurb": "The ONE \"filter this list\" field: the text field with the search glyph INSIDE it and a ghost clear that appears only once there is something to clear — and puts the caret back in the field, so typing continues. Eight of them existed at five heights, most a bare Input re-dressed by hand and none with either affordance, while both natives had drawn exactly this for years. Two rungs: md is the stock 36 field, sm the 28 one dense columns use — the Changes file tree's filter, a sidebar filter. It is an Input, not a new box: every chrome decision still comes from there.",
    "components": [
      "Input"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "SearchField",
        "file": "packages/ui/src/search-field.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::inputs::search_field",
        "file": "apps/desktop/crates/eui-gpui/src/controls/inputs.rs",
        "note": "SearchFieldSize::Md 36 / Sm 28; the diff pane filter, every picker query and the search dialog draw it"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassSheetSearchField",
        "file": "apps/ios/ExpUI/Sources/GlassControls.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassSheetSearchField",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassSheet.kt"
      }
    }
  },
  {
    "id": "date-picker",
    "title": "Date picker",
    "blurb": "The ONE date picker. Popover + Calendar was inlined three times — the properties panel, the editor chips, the mobile tray — and each copy converted between a Date and the wire's YYYY-MM-DD its own way, two of them through new Date(value), which the spec parses as UTC and which therefore shows the PREVIOUS day west of Greenwich. This one speaks the wire format on both sides and converts in exactly one place. A due date is a DATE, never an instant (REV2-49), so there is no time arm; the trigger is a Pill showing the short form, and Clear is a row under the grid rather than a second control beside it.",
    "components": [
      "DatePicker"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "DatePicker",
        "file": "packages/ui/src/date-picker.tsx",
        "note": "parseDateValue / formatDateLabel are the only place the wire date becomes a Date"
      },
      "desktop": {
        "status": "ok",
        "symbol": "pickers::due_date_popover",
        "file": "apps/desktop/crates/ui/src/pickers.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "DueDateSheet",
        "file": "apps/ios/Exponential/UI/Issue/Sheets/DueDateSheet.swift",
        "note": "CreateIssueView keeps a second unfoldable form, UI/Issue/DueDatePicker.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "DueDateSheet",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/issue/DueDateSheet.kt",
        "note": "the grid itself is IssueDatePickerDialog.kt"
      }
    }
  },
  {
    "id": "alert",
    "title": "Alert",
    "blurb": "The inline banner: a message that belongs to the page it interrupts, not a toast that flies past and not a dialog that blocks. Two variants only — the neutral card fill for a notice, and the destructive tint for a failure — and the leading glyph earns its own column only when one is passed. The admin console carried two byte-identical copies of the destructive recipe before this existed.",
    "components": [
      "Alert"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Alert / AlertTitle / AlertDescription",
        "file": "packages/ui/src/alert.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::feedback::alert / alert_title",
        "file": "apps/desktop/crates/eui-gpui/src/controls/feedback.rs",
        "note": "default and destructive on the glass tokens, a glyph column only when one is passed; the repository dialog banner is one"
      },
      "ios": {
        "status": "n/a",
        "note": "no boxed banner: an error renders as a red Text line on DesignTokens.Semantic.red"
      },
      "android": {
        "status": "n/a",
        "note": "no boxed banner: the caller-less GlassNotice is gone; an error renders as a red Text line"
      }
    }
  },
  {
    "id": "context-ring",
    "title": "Context ring",
    "blurb": "How full the agent's context window is, as a 16px radial where the context pill used to be — and the trigger of the usage overlay the session already had. The arc is a stroked circle rotated a quarter turn, the track the same circle at 20% opacity, and the tone is the session's, not the ring's: the app maps its own thresholds onto normal / warning / danger and passes one in. A session nothing has measured renders NOTHING unless `showEmpty` says the session has other usage worth opening.",
    "components": [
      "Ring"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "ContextRing",
        "file": "packages/ui/src/context-ring.tsx",
        "note": "ringGeometry + RING_TONE_CLASS ship with it; the app's lib/context-ring.ts derives percent and tone"
      },
      "desktop": {
        "status": "ok",
        "symbol": "usage_sheet::context_ring",
        "file": "apps/desktop/crates/ui/src/usage_sheet.rs",
        "note": "mounted by steer_viewer::render_context_ring; the percentage comes from usage_bar::context_percent"
      },
      "ios": {
        "status": "ok",
        "symbol": "ContextRing",
        "file": "apps/ios/ExpUI/Sources/ContextRing.swift"
      },
      "android": {
        "status": "ok",
        "symbol": "ContextRing",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/ContextRing.kt"
      }
    }
  },
  {
    "id": "progress-ring",
    "title": "Sub-issue progress ring",
    "blurb": "The completion ring that leads the issue detail's Sub-issues band — done of total as an arc over a 20% track, the SAME 16-unit box, 2-unit stroke and track as the context ring (`ringGeometry`), drawn at 14px and painted in the team's COMPLETED status colour. The band reads ring · \"Sub-issues\" · `done/total` · `+` over flat rows (`lib/issue-relations-view.ts`, fixture-locked ×4).",
    "components": [
      "Ring"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "ProgressRing",
        "file": "packages/ui/src/progress-ring.tsx",
        "note": "the Sub-issues band in components/issue-relations-card.tsx mounts it"
      },
      "desktop": {
        "status": "ok",
        "symbol": "issue_relations::render_sub_issues",
        "file": "apps/desktop/crates/ui/src/issue_relations.rs",
        "note": "a ProgressCircle on the context ring's geometry, the COMPLETED status colour"
      },
      "ios": {
        "status": "ok",
        "symbol": "ProgressRing",
        "file": "apps/ios/ExpUI/Sources/ProgressRing.swift",
        "note": "IssueSubIssuesSection (IssueRelationRows.swift) leads its band with it"
      },
      "android": {
        "status": "ok",
        "symbol": "SubIssueProgressRing",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/issue/RelationsSection.kt",
        "note": "SubIssuesSection leads its band with it"
      }
    }
  },
  {
    "id": "list-empty",
    "title": "In-list empty line",
    "blurb": "This IS `EmptyState size=\"list\"` without a glyph — one muted centred line inside a list that filtered down to nothing, the line `CommandEmpty` draws. A search that matched nothing needs a different query, not a next step, so it never teaches.",
    "components": [
      "EmptyState"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "ListEmpty",
        "file": "packages/ui/src/empty-state.tsx",
        "note": "EmptyState size=\"list\" with no icon, same file"
      },
      "desktop": {
        "status": "ok",
        "symbol": "controls::feedback::empty_state_list",
        "file": "apps/desktop/crates/eui-gpui/src/controls/feedback.rs",
        "note": "with no icon; pickers::empty_picker_row is the picker's CommandEmpty row"
      },
      "ios": {
        "status": "ok",
        "symbol": "EmptyState",
        "file": "apps/ios/ExpUI/Sources/EmptyState.swift",
        "note": "EmptyState(title:icon:size:) with size .list and no icon"
      },
      "android": {
        "status": "ok",
        "symbol": "EmptyState",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/Scaffolding.kt",
        "note": "EmptyStateSize.List with no icon"
      }
    }
  },
  {
    "id": "picker",
    "title": "Picker",
    "blurb": "THE picker primitive: one surface per platform (a popover at the trigger on a pointer, a bottom sheet of plain rows on a phone), single or multi, optional search. Presentation belongs to the primitive, never to the caller — a row is a glyph or a dot, a label and a muted second line, and it is never a card. The selection language is the whole point: a single pick wears a trailing check, a MULTI pick reads as the row's own highlight, never a leading circle, so the picker that links a relation and the picker that batches issues finally look like one thing. The trigger is the caller's (four shapes: pill, field, row, inline); the surface, the search field and the keys are the primitive's.",
    "components": [
      "Select"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Picker",
        "file": "packages/ui/src/picker/picker.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "picker::Picker",
        "file": "apps/desktop/crates/ui/src/picker/mod.rs"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassPicker",
        "file": "apps/ios/ExpUI/Sources/Picker/Picker.swift",
        "note": "SwiftUI owns the bare name Picker, so the primitive is GlassPicker; the ten typed ones keep theirs."
      },
      "android": {
        "status": "ok",
        "symbol": "Picker",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/picker/Picker.kt"
      }
    }
  },
  {
    "id": "menu",
    "title": "Menu",
    "blurb": "The one floating-menu surface (opaque card fill, hairline, radius 12, no blur, no shadow) and the one ROW recipe on it, in two densities from tokens.json `menu`: POINTER for web from md up (36 row · 8 pad · 8 gap · 16 glyph · 180–280 wide), TOUCH for mobile web, iOS and Android (48 row · 12 pad · 12 gap · 16 glyph · 180–280 wide). Every menu-like row on web reads the same --menu-* vars — context and dropdown items, their sub-triggers, Select, Command, the typeahead — so one change moves every menu. A destructive row is red and never fenced off by a divider.",
    "components": [
      "Menu"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Menu",
        "file": "packages/ui/src/menu.tsx",
        "note": "ONE data-driven Menu (MenuEntry[], trigger | pointer | sheet) over the SDK menu + sheet primitives; rows wear menu-surface's --menu-* constants"
      },
      "desktop": {
        "status": "leftover",
        "symbol": "menu::pointer",
        "file": "apps/desktop/crates/theme/src/tokens.generated.rs",
        "note": "gpui-component's PopupMenu draws 26px rows / 8px pad from the crate; the tokens record 36 / 8"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassMenuTokens",
        "file": "apps/ios/ExpUI/Sources/GlassMenu.swift",
        "note": "the touch set: 48pt rows, 12pt padding"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassMenuDefaults",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassMenu.kt",
        "note": "the touch set: M3's 48dp rows, 12dp padding"
      }
    }
  },
  {
    "id": "tooltip",
    "title": "Tooltip",
    "blurb": "The pointer's one-line label: a glass panel (12 radius, 12px text, padding 6 × 12, no arrow: a translucent arrow double-renders against the blurred panel) that opens at once on hover or focus and never holds a control. `IconTooltip` is the common arm: a ghost glyph button whose tooltip IS its accessible name, so an icon-only control can never ship unlabelled. Touch has no hover, so the phones carry no tooltip: the label is the control's accessibility name there.",
    "components": [
      "Tooltip"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Tooltip / TooltipContent",
        "file": "packages/ui/src/tooltip.tsx",
        "note": "IconTooltip (icon-tooltip.tsx) = the glyph-button arm"
      },
      "desktop": {
        "status": "n/a",
        "note": "gpui-component's Tooltip::new on .tooltip(); no app wrapper"
      },
      "ios": {
        "status": "n/a",
        "note": "touch: no hover tooltips; the label is the accessibility name"
      },
      "android": {
        "status": "n/a",
        "note": "touch: no hover tooltips; the label is the content description"
      }
    }
  },
  {
    "id": "hover-card",
    "title": "Hover card",
    "blurb": "The pointer's preview (the catalog `Popover openOn: hover`): the opaque menu card (12 radius, card hairline, no blur, no shadow), 320 wide, padding 12, opening after 400 ms so a pointer crossing a dense list of #IDENT chips never flashes one, closing after 100 ms so the pointer can travel into it. It carries the issue preview body (status · identifier · title · the first lines) and the account picker's usage preview; a tap on touch just activates the trigger, so the phones have none.",
    "components": [
      "Popover"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "HoverCard / HoverCardContent",
        "file": "packages/ui/src/hover-card.tsx"
      },
      "desktop": {
        "status": "ok",
        "symbol": "issue_preview::IssuePreviewHost",
        "file": "apps/desktop/crates/ui/src/issue_preview.rs"
      },
      "ios": {
        "status": "n/a",
        "note": "touch: a tap opens the issue, there is no hover preview"
      },
      "android": {
        "status": "n/a",
        "note": "touch: a tap opens the issue, there is no hover preview"
      }
    }
  },
  {
    "id": "collapsible",
    "title": "Collapsible",
    "blurb": "The fold BEHIND a disclosure header: Radix Collapsible owns the open state and the content's mount, the header owns the words and the chevron. It never draws chrome of its own, so a fold reads as the surface it sits in (a settings group, a transcript's Show work, a properties section). The natives fold with their own animation primitives under the same disclosure header. In the catalog: `Collapsible`, or a `Section collapsible` when the fold heads a list.",
    "components": [
      "Collapsible",
      "Section"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Collapsible / CollapsibleContent",
        "file": "packages/ui/src/collapsible.tsx"
      },
      "desktop": {
        "status": "n/a",
        "note": "a fold = disclosure_header + the caller's own child toggle"
      },
      "ios": {
        "status": "n/a",
        "note": "a fold = DisclosureHeader + a conditional view"
      },
      "android": {
        "status": "n/a",
        "note": "a fold = DisclosureHeader + AnimatedVisibility"
      }
    }
  },
  {
    "id": "menu-family",
    "title": "Menu · Picker · Typeahead",
    "blurb": "ONE data-driven Menu for every action menu, a list of MenuEntry rows (item, submenu with more rows or a picker body, toggle, separator, header) under one renderer in three modes: TRIGGER (a dropdown at a button), POINTER (a context menu at the pointer; one gesture host per layout, a row opts in with menuProps(kind, id)) and SHEET (the same rows as a bottom sheet below md, submenus as pushed pages). Every row is the one --menu-* recipe (pointer 36 · 8 · 8 · 16, touch 48 · 12 · 12 · 16), a destructive row red with no divider above it. A submenu's picker body is PickerMenuRows, and Menu, Picker and the typeahead speak ONE highlight language: single = a trailing check, multi = the row's wash, mixed = a softer wash. In the catalog: `Menu` (`openOn: press | contextmenu`, submenu `items` bindable to a host source) and `Select`.",
    "components": [
      "Menu",
      "Select"
    ],
    "status": {
      "web": {
        "status": "ok",
        "symbol": "Menu / MenuGestureHost / MenuPanel",
        "file": "packages/ui/src/menu.tsx",
        "note": "rows wear MENU_ITEM_CLASS (packages/exponential-ui-react/src/primitives/menu-surface.ts)"
      },
      "desktop": {
        "status": "leftover",
        "symbol": "PopupMenu + controls::PointerMenu / pointer_label_item",
        "file": "apps/desktop/crates/ui/src/controls.rs",
        "note": "issue, tab, file and session menus point; settings, actions and team menus still plain crate rows"
      },
      "ios": {
        "status": "ok",
        "symbol": "GlassMenu",
        "file": "apps/ios/ExpUI/Sources/GlassMenu.swift",
        "note": "the touch set: 48pt rows, 12pt padding"
      },
      "android": {
        "status": "ok",
        "symbol": "GlassDropdownMenu",
        "file": "apps/android/app/src/main/java/com/exponential/app/ui/components/GlassMenu.kt",
        "note": "the touch set: M3's 48dp rows, 12dp padding"
      }
    },
    "leftovers": [
      {
        "file": "apps/desktop/crates/ui/src/controls.rs",
        "note": "gpui-component's PopupMenu draws 26px rows / 8px pad from the crate; the tokens record 36 / 8 (a leftover by decision)"
      }
    ]
  }
]

/** every AppParity whose components include `name` */
export function appParityFor(name: string): AppParity[] {
  return APP_PARITY.filter((row) => row.components.includes(name))
}
