// EXP-273 — the shared icon registry.
//
// One Lucide name per concept, projected into all four clients by
// `scripts/generate.ts`. Everything exported here is generated from
// `icons.json`; edit that file (and re-run the generator), never `generated.ts`.
//
// Clients consume this as:
//   web      `ICON_COMPONENTS` / `conceptIcon()` in apps/web/src/lib/icons.generated.ts
//   iOS      `AppIcons` + the `AppIcon` view in ExpUI
//   Android  `ExpIcons` in ui/icons
//   desktop  `icons.generated.rs` in crates/ui

export {
  ICON_NAMES,
  PICKABLE_ICONS,
  SEMANTIC_ICONS,
  isIconName,
  isPickableIcon,
  type IconConcept,
  type IconName,
  type PickableIcon,
} from "./generated"
