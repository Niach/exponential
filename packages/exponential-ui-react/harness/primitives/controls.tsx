// FEED-131: the `controls` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "controls";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/controls.tsx.
import type { ReactElement } from "react"
import { LayoutGridIcon, ListIcon } from "lucide-react"
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

export const SPECIMENS: Record<string, () => ReactElement> = {
  // ── checkbox · switch · tabs · segmented-control · collapsible · scroll-area · avatar ──
  "controls/checkbox": () => (
    <div style={{ display: `flex`, alignItems: `center`, gap: 16 }}>
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
    <div style={{ display: `flex`, alignItems: `center`, gap: 16 }}>
      <Switch aria-label="Off" />
      <Switch aria-label="On" defaultChecked />
      <Switch aria-label="Disabled" disabled />
      <Switch aria-label="Small off" size="sm" />
      <Switch aria-label="Small on" size="sm" defaultChecked />
      <Switch aria-label="Small disabled" size="sm" disabled defaultChecked />
    </div>
  ),
  "controls/tabs": () => (
    <Tabs defaultValue="password" className="w-96">
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
    <div style={{ display: `flex`, flexDirection: `column`, gap: 16, width: 360 }}>
      <SegmentedControl
        value="list"
        onValueChange={() => {}}
        options={[
          { value: `list`, label: `List`, icon: ListIcon },
          { value: `board`, label: `Board`, icon: LayoutGridIcon },
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
    <div style={{ display: `flex`, flexDirection: `column`, gap: 16, width: 320 }}>
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
  // The hovered area is `type="always"`: a `hover` bar mounts a ResizeObserver
  // + frame + 10 ms after the pointer enters, a race against the gate's two
  // frames (browser/primitives-controls.test.ts waits for it instead). The
  // second area is the default `hover` type at rest.
  "controls/scroll-area": () => (
    <div style={{ display: `flex`, gap: 16 }}>
      <ScrollArea type="always" className="rounded-md border" style={{ height: 160, width: 192 }} data-xui-target="">
        <div className="p-4">
          {Array.from({ length: 24 }, (_, i) => (
            <div key={i} className="text-sm">{`Row ${i + 1}`}</div>
          ))}
        </div>
      </ScrollArea>
      <ScrollArea className="rounded-md border" style={{ height: 160, width: 192 }}>
        <div className="p-4">
          {Array.from({ length: 24 }, (_, i) => (
            <div key={i} className="text-sm">{`Item ${i + 1}`}</div>
          ))}
        </div>
      </ScrollArea>
    </div>
  ),
  "controls/avatar": () => (
    <div style={{ display: `flex`, alignItems: `center`, gap: 12 }}>
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
    <div style={{ display: `flex`, gap: 12 }}>
      <Select>
        <SelectTrigger className="w-44">
          <SelectValue placeholder="Pick a fruit" />
        </SelectTrigger>
        <SelectContent>{selectItems()}</SelectContent>
      </Select>
      <Select defaultValue="banana">
        <SelectTrigger className="w-44">
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
      <SelectTrigger className="w-44">
        <SelectValue placeholder="Pick a fruit" />
      </SelectTrigger>
      <SelectContent>{selectItems()}</SelectContent>
    </Select>
  ),
  "controls/select-open-popper": () => (
    <Select defaultOpen defaultValue="carrot">
      <SelectTrigger className="w-44">
        <SelectValue placeholder="Pick a fruit" />
      </SelectTrigger>
      <SelectContent position="popper">{selectItems()}</SelectContent>
    </Select>
  ),
  // ── command ──
  "controls/command": () => (
    <Command label="Commands" className="w-80" data-xui-target="">
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
    <Command label="Commands" className="w-80">
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
