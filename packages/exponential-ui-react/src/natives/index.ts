// VAPP-87: the native component map — every `kind: native` of the core
// catalog (macros expand in the reducer and never reach here). An
// extension may add to or override this through `registerExtension`.

import type { ComponentType } from "react"
import type { NativeProps } from "../node-view"
import { ButtonNative, LinkNative, ToggleNative } from "./actions"
import { ChartNative, RingNative, SkeletonNative, SpinnerNative, TreeGuidesNative } from "./feedback"
import { CheckboxNative, ComposerNative, DatePickerNative, InputNative, RadioNative, SelectNative, SliderNative, SwitchNative, TextareaNative } from "./inputs"
import { BoxNative, ListNative, UnknownNative } from "./layout"
import { AudioPlayerNative, AvatarNative, CarouselNative, ImageNative, VideoNative } from "./media"
import { AccordionNative, TabsNative, ToggleGroupNative } from "./nav"
import { DialogNative, DrawerNative, DropdownMenuNative, PopoverNative, TooltipNative } from "./overlays"
import { IconNative, MarkdownNative, TextNative } from "./text"

export const NATIVES: Record<string, ComponentType<NativeProps>> = {
  Box: BoxNative,
  List: ListNative,
  Text: TextNative,
  Markdown: MarkdownNative,
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
  Ring: RingNative,
  Spinner: SpinnerNative,
  Skeleton: SkeletonNative,
  Chart: ChartNative,
  Button: ButtonNative,
  Link: LinkNative,
  Toggle: ToggleNative,
  Input: InputNative,
  Textarea: TextareaNative,
  Checkbox: CheckboxNative,
  Radio: RadioNative,
  Switch: SwitchNative,
  Slider: SliderNative,
  Select: SelectNative,
  DatePicker: DatePickerNative,
  Composer: ComposerNative,
  TreeGuides: TreeGuidesNative,
  Unknown: UnknownNative,
}

export const NATIVE_NAMES: readonly string[] = Object.keys(NATIVES)
