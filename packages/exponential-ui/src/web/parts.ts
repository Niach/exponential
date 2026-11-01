// Round 2 §6 (catalog/recipes.json), FEED-128: hoisted from the React
// renderer so every web renderer (React, Solid) reads ONE table. The SINGLE
// parts a painter lays out and the conformance dump compares, by component.
// A placed part carries `data-xui-id="<node id>.<part>"`; per-item parts
// carry `<id>.<part>.<item>` (the renderer's `part.at`). Paint-only parts
// (thumbs, the Slider range, the check glyph, arrows, overlay content in its
// layer) carry no id.

export const PLACED: Readonly<Record<string, readonly string[]>> = {
  Switch: [`body`, `label`, `description`, `track`],
  Input: [`label`, `field`, `description`, `error`],
  Textarea: [`label`, `field`, `description`, `error`],
  NumberField: [`label`, `field`, `decrement`, `input`, `unit`, `increment`, `description`, `error`],
  ChipInput: [`label`, `field`, `input`, `description`, `error`],
  Select: [`label`, `trigger`],
  DatePicker: [`label`, `trigger`],
  DateRangePicker: [`label`, `trigger`],
  TimePicker: [`label`, `trigger`],
  Radio: [`label`, `items`],
  ChoiceTiles: [`error`],
  Checkbox: [`box`, `label`, `description`],
  Slider: [`header`, `label`, `value`, `track`],
  Tabs: [`list`, `content`, `new`],
  // EXP-1259: the search list (a searchable submenu's parts carry `.<i>`).
  SearchList: [`head`, `search`, `count`, `body`, `list`, `empty`, `fade`],
  Menu: [`trigger`, `head`, `search`, `count`, `body`, `list`, `empty`, `fade`],
  Table: [`header`, `body`, `caption`, `empty`],
  Carousel: [`page`, `indicator`, `controls`, `previous`, `next`, `counter`, `thumbnails`],
  Tooltip: [`anchor`],
  FileUpload: [`label`, `dropzone`, `icon`, `title`, `hint`, `browse`, `description`],
  CodeBlock: [`header`, `title`, `copy`, `body`],
  AudioPlayer: [`track`, `controls`],
  Video: [`controls`],
  Image: [`fallback`],
  Ring: [`label`],
}

/** The painted id of a SINGLE part (`<domId>.<part>`), or null when the
 *  component does not place it. */
export function placedPartId(component: string, part: string, domId: string): string | null {
  return PLACED[component]?.includes(part) ? `${domId}.${part}` : null
}
