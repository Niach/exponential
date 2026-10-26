// FEED-128: which natives file paints which core native (React's split,
// file for file). natives/index.ts composes NATIVES from the per-file
// records in THIS key order (React's), so the fan-out lanes never edit one
// shared file; natives.test.ts checks every file paints only what it owns.

export const NATIVE_ORDER: readonly string[] = [`Box`, `List`, `Resizable`, `Text`, `Markdown`, `CodeBlock`, `Image`, `Icon`, `Video`, `AudioPlayer`, `Avatar`, `Carousel`, `Tabs`, `Segmented`, `Accordion`, `Dialog`, `Drawer`, `Popover`, `Tooltip`, `Menu`, `Toast`, `Ring`, `Spinner`, `Skeleton`, `VoiceWave`, `Table`, `Chart`, `Button`, `Link`, `Toggle`, `Form`, `Input`, `Textarea`, `NumberField`, `Checkbox`, `Radio`, `ChoiceTiles`, `Switch`, `Slider`, `Select`, `SearchList`, `ChipInput`, `DatePicker`, `DateRangePicker`, `TimePicker`, `FileUpload`, `Composer`, `TreeGuides`, `Unknown`]

export const OWNERS: Readonly<Record<string, readonly string[]>> = {
  layout: [`Box`, `List`, `Unknown`],
  text: [`Text`, `Markdown`, `Icon`],
  form: [`Form`],
  "actions": [`Button`, `Link`, `Toggle`],
  "chart": [`Chart`],
  "chips": [`ChipInput`, `FileUpload`],
  "choice-tiles": [`ChoiceTiles`],
  "code": [`CodeBlock`],
  "dates": [`DatePicker`, `DateRangePicker`, `TimePicker`],
  "feedback": [`Ring`, `Spinner`, `Skeleton`, `TreeGuides`, `VoiceWave`],
  "inputs": [`Input`, `Textarea`, `NumberField`, `Checkbox`, `Radio`, `Switch`, `Slider`, `Composer`],
  "media": [`Image`, `Video`, `AudioPlayer`, `Avatar`, `Carousel`],
  "nav": [`Tabs`, `Segmented`, `Accordion`],
  "overlays": [`Dialog`, `Drawer`, `Popover`, `Tooltip`, `Menu`, `Toast`],
  "resizable": [`Resizable`],
  "search": [`SearchList`],
  "select": [`Select`],
  "table": [`Table`],
}
