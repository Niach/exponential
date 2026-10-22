// FEED-131: the `controls` lane's primitive specimens (Solid = the candidate).
// One entry per `primitive-specimens.json` row whose `primitive` is "controls";
// the SAME ids in packages/exponential-ui-react/harness/primitives/controls.tsx.
import type { JSX } from "solid-js"
import { LayoutGrid, List } from "lucide-solid"
import { Avatar, AvatarFallback, AvatarImage } from "../../src/primitives/avatar"
import { Checkbox } from "../../src/primitives/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "../../src/primitives/collapsible"
import { ScrollArea } from "../../src/primitives/scroll-area"
import { SegmentedControl } from "../../src/primitives/segmented-control"
import { Switch } from "../../src/primitives/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "../../src/primitives/tabs"

/** A 1×1 PNG: loads on both harnesses without the network. */
const AVATAR_PIXEL = `data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==`
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "../../src/primitives/select"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandSeparator, CommandShortcut } from "../../src/primitives/command"

function selectItems() {
  return (
    <>
      <SelectGroup>
        <SelectLabel>Fruits</SelectLabel>
        <SelectItem value="apple">Apple</SelectItem>
        <SelectItem value="banana">Banana</SelectItem>
      </SelectGroup>
      <SelectSeparator />
      <SelectGroup>
        <SelectLabel>Vegetables</SelectLabel>
        <SelectItem value="carrot">Carrot</SelectItem>
        <SelectItem value="leek" disabled>
          Leek
        </SelectItem>
      </SelectGroup>
    </>
  )
}

export const SPECIMENS: Record<string, () => JSX.Element> = {
  // ── checkbox · switch · tabs · segmented-control · collapsible · scroll-area · avatar ──
  "controls/checkbox": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; align-items: center; gap: 16px;`)}>
      <Checkbox aria-label="Unchecked" data-xui-target="" />
      <Checkbox aria-label="Checked" defaultChecked />
      <Checkbox aria-label="Indeterminate" checked="indeterminate" />
      <Checkbox aria-label="Disabled" disabled />
      <Checkbox aria-label="Disabled checked" disabled defaultChecked />
      <Checkbox aria-label="Invalid" aria-invalid />
      <form>
        <Checkbox aria-label="In a form" name="terms" defaultChecked />
      </form>
    </div>
  ),
  "controls/switch": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; align-items: center; gap: 16px;`)}>
      <Switch aria-label="Off" />
      <Switch aria-label="On" defaultChecked />
      <Switch aria-label="Disabled" disabled />
      <Switch aria-label="Small off" size="sm" />
      <Switch aria-label="Small on" size="sm" defaultChecked />
      <Switch aria-label="Small disabled" size="sm" disabled defaultChecked />
    </div>
  ),
  "controls/tabs": () => (
    <Tabs defaultValue="password" class="w-96">
      <TabsList>
        <TabsTrigger value="account" data-xui-target="">
          Account
        </TabsTrigger>
        <TabsTrigger value="password">Password</TabsTrigger>
        <TabsTrigger value="billing" disabled>
          Billing
        </TabsTrigger>
      </TabsList>
      <TabsContent value="account">Account settings</TabsContent>
      <TabsContent value="password">Change your password</TabsContent>
      <TabsContent value="billing">Billing details</TabsContent>
    </Tabs>
  ),
  "controls/segmented-control": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; flex-direction: column; gap: 16px; width: 360px;`)}>
      <SegmentedControl
        value="list"
        onValueChange={() => {}}
        options={[
          { value: `list`, label: `List`, icon: List },
          { value: `board`, label: `Board`, icon: LayoutGrid },
          { value: `off`, label: `Off`, disabled: true },
        ]}
      />
      <SegmentedControl
        value="mine"
        onValueChange={() => {}}
        fill
        options={[
          { value: `inbox`, label: `Inbox` },
          { value: `mine`, label: `My issues` },
        ]}
      />
    </div>
  ),
  "controls/collapsible": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; flex-direction: column; gap: 16px; width: 320px;`)}>
      <Collapsible defaultOpen>
        <CollapsibleTrigger>Open section</CollapsibleTrigger>
        <CollapsibleContent>Visible body</CollapsibleContent>
      </Collapsible>
      <Collapsible>
        <CollapsibleTrigger>Closed section</CollapsibleTrigger>
        <CollapsibleContent>Hidden body</CollapsibleContent>
      </Collapsible>
    </div>
  ),
  // See the React twin: the hovered area is `type="always"`, the second the
  // default `hover` type at rest.
  "controls/scroll-area": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; gap: 16px;`)}>
      <ScrollArea type="always" class="rounded-md border" style={{ height: `${160}px`, width: `${192}px` }} data-xui-target="">
        <div class="p-4">
          {Array.from({ length: 24 }, (_, i) => (
            <div class="text-sm">{`Row ${i + 1}`}</div>
          ))}
        </div>
      </ScrollArea>
      <ScrollArea class="rounded-md border" style={{ height: `${160}px`, width: `${192}px` }}>
        <div class="p-4">
          {Array.from({ length: 24 }, (_, i) => (
            <div class="text-sm">{`Item ${i + 1}`}</div>
          ))}
        </div>
      </ScrollArea>
    </div>
  ),
  "controls/avatar": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; align-items: center; gap: 12px;`)}>
      <Avatar>
        <AvatarFallback>DS</AvatarFallback>
      </Avatar>
      <Avatar>
        <AvatarFallback userId="user-42">AB</AvatarFallback>
      </Avatar>
      <Avatar>
        <AvatarImage src={AVATAR_PIXEL} alt="Pixel" />
        <AvatarFallback userId="pixel">PX</AvatarFallback>
      </Avatar>
    </div>
  ),
  // ── select ──
  "controls/select-closed": () => (
    <div ref={(el) => el.setAttribute(`style`, `display: flex; gap: 12px;`)}>
      <Select>
        <SelectTrigger class="w-44">
          <SelectValue placeholder="Pick a fruit" />
        </SelectTrigger>
        <SelectContent>{selectItems()}</SelectContent>
      </Select>
      <Select defaultValue="banana">
        <SelectTrigger class="w-44">
          <SelectValue placeholder="Pick a fruit" />
        </SelectTrigger>
        <SelectContent>{selectItems()}</SelectContent>
      </Select>
      <Select disabled>
        <SelectTrigger size="sm">
          <SelectValue placeholder="Disabled" />
        </SelectTrigger>
        <SelectContent>{selectItems()}</SelectContent>
      </Select>
    </div>
  ),
  "controls/select-open": () => (
    <Select defaultOpen defaultValue="banana">
      <SelectTrigger class="w-44">
        <SelectValue placeholder="Pick a fruit" />
      </SelectTrigger>
      <SelectContent>{selectItems()}</SelectContent>
    </Select>
  ),
  "controls/select-open-popper": () => (
    <Select defaultOpen defaultValue="carrot">
      <SelectTrigger class="w-44">
        <SelectValue placeholder="Pick a fruit" />
      </SelectTrigger>
      <SelectContent position="popper">{selectItems()}</SelectContent>
    </Select>
  ),
  // ── command ──
  "controls/command": () => (
    <Command label="Commands" class="w-80" data-xui-target="">
      <CommandInput defaultValue="app" placeholder="Search" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Fruit">
          <CommandItem value="Banana">Banana</CommandItem>
          <CommandItem value="Apple">
            Apple
            <CommandShortcut>⌘A</CommandShortcut>
          </CommandItem>
          <CommandItem value="Pineapple" disabled>
            Pineapple
          </CommandItem>
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Tools">
          <CommandItem value="Hammer">Hammer</CommandItem>
          <CommandItem value="Apple pie tool">Apple pie tool</CommandItem>
        </CommandGroup>
      </CommandList>
    </Command>
  ),
  "controls/command-empty": () => (
    <Command label="Commands" class="w-80">
      <CommandInput defaultValue="zzz" placeholder="Search" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Fruit">
          <CommandItem value="Apple">Apple</CommandItem>
        </CommandGroup>
      </CommandList>
    </Command>
  ),
}
