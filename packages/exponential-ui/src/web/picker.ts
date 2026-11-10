// #EXP-1760: IconPicker and ColorPicker are macros over a Popover; a pick in
// the grid (a cell, a swatch, the No icon row) closes that popover, as the
// IDE's pickers do. The core does the same (`surface/events.rs` picker_root).

type Recipe = { macro?: string; part?: string } | undefined

/** Whether a node is an IconPicker / ColorPicker's root (its Popover). */
export function isPickerPopover(recipe: Recipe): boolean {
  return recipe?.part === `root` && (recipe.macro === `IconPicker` || recipe.macro === `ColorPicker`)
}

/** Whether a press on this node is a pick that closes the picker. */
export function isPickerPick(recipe: Recipe): boolean {
  if (recipe?.macro === `IconPicker`) return recipe.part === `cell` || recipe.part === `none`
  return recipe?.macro === `ColorPicker` && recipe.part === `swatch`
}
