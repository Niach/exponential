// VAPP-87 + round 1: the native component map — every `kind: native` of the
// core catalog (macros expand in the reducer and never reach here). An
// extension may add to or override this through `registerExtension`.
// `natives.test.ts` gates it against the catalog: every native has a
// painter, no painter is left without a catalog entry.

import type { ComponentType } from "react"
import { FormNative } from "../form"
import type { NativeProps } from "../node-view"
import { ButtonNative, LinkNative, ToggleNative } from "./actions"
import { ChartNative } from "./chart"
import { ChipInputNative, FileUploadNative } from "./chips"
import { CodeBlockNative } from "./code"
import { DatePickerNative, DateRangePickerNative, TimePickerNative } from "./dates"
import { RingNative, SkeletonNative, SpinnerNative, TreeGuidesNative } from "./feedback"
import { CheckboxNative, ComposerNative, InputNative, NumberFieldNative, RadioNative, SliderNative, SwitchNative, TextareaNative } from "./inputs"
import { BoxNative, ListNative, UnknownNative } from "./layout"
import { AudioPlayerNative, AvatarNative, CarouselNative, ImageNative, VideoNative } from "./media"
import { AccordionNative, TabsNative, ToggleGroupNative } from "./nav"
import { ContextMenuNative, DialogNative, DrawerNative, DropdownMenuNative, PopoverNative, ToastNative, TooltipNative } from "./overlays"
import { SelectNative } from "./select"
import { TableNative } from "./table"
import { IconNative, MarkdownNative, TextNative } from "./text"


export const NATIVES: Record<string, ComponentType<NativeProps>> = {
  Box: BoxNative,
  List: ListNative,
  Text: TextNative,
  Markdown: MarkdownNative,
  CodeBlock: CodeBlockNative,
  Image: ImageNative,
  Icon: IconNative,
  Video: VideoNative,
  AudioPlayer: AudioPlayerNative,
  Avatar: AvatarNative,
  Carousel: CarouselNative,
  Tabs: TabsNative,
  ToggleGroup: ToggleGroupNative,
  Accordion: AccordionNative,
  Dialog: DialogNative,
  Drawer: DrawerNative,
  Popover: PopoverNative,
  Tooltip: TooltipNative,
  DropdownMenu: DropdownMenuNative,
  ContextMenu: ContextMenuNative,
  Toast: ToastNative,
  Ring: RingNative,
  Spinner: SpinnerNative,
  Skeleton: SkeletonNative,
  Table: TableNative,
  Chart: ChartNative,
  Button: ButtonNative,
  Link: LinkNative,
  Toggle: ToggleNative,
  Form: FormNative,
  Input: InputNative,
  Textarea: TextareaNative,
  NumberField: NumberFieldNative,
  Checkbox: CheckboxNative,
  Radio: RadioNative,
  Switch: SwitchNative,
  Slider: SliderNative,
  Select: SelectNative,
  ChipInput: ChipInputNative,
  DatePicker: DatePickerNative,
  DateRangePicker: DateRangePickerNative,
  TimePicker: TimePickerNative,
  FileUpload: FileUploadNative,
  Composer: ComposerNative,
  TreeGuides: TreeGuidesNative,
  Unknown: UnknownNative,
}

export const NATIVE_NAMES: readonly string[] = Object.keys(NATIVES)
