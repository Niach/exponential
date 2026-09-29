// EXP-273 — the shared icon registry.
//
// One Lucide name per concept, projected into all four clients by
// `scripts/generate.ts`. Everything exported here is generated from
// `icons.json`; edit that file (and re-run the generator), never `generated.ts`.
//
// Clients consume this as:
//   web      `ICON_COMPONENTS` / `conceptIcon()` in packages/ui/src/icons.generated.ts
//   iOS      `AppIcons` + the `AppIcon` view in ExpUI
//   Android  `ExpIcons` in ui/icons
//   desktop  `icons.generated.rs` in crates/ui
//
// The MCP catalog's brand marks (`brand` in icons.json) are a set apart:
// `BRAND_ICON_NAMES` here, the components in packages/ui/src/brand-icons
// .generated.tsx, the desktop assets `brand-<slug>.svg` + `registry::brand`.

export {
  BRAND_ICON_NAMES,
  CUSTOM_ICONS,
  DEVICE_ICONS,
  ICON_NAMES,
  PICKABLE_ICONS,
  SEMANTIC_ICONS,
  isBrandIcon,
  isDeviceIcon,
  isIconName,
  isPickableIcon,
  type BrandIconName,
  type CustomIcon,
  type DeviceIconName,
  type IconConcept,
  type IconName,
  type PickableIcon,
} from "./generated"
