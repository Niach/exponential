// FEED-131: the `controls` lane's jsdom parity (harness/primitives/controls.tsx
// is the browser twin): every Solid twin against its React module, state by
// state, the whole <body> compared (portals and body attributes included).
// React's modules load at run time (their JSX stays out of the Solid
// transform), each case builds the React side with `createElement`.

/* eslint-disable @typescript-eslint/no-explicit-any */
import { act, createElement as h, Fragment } from "react"
import { expectElementParity, renderReactElement, renderSolidElement, serializeBody } from "../test/parity"

const reactModule = async (name: string): Promise<Record<string, any>> => (await import(/* @vite-ignore */ `@exponential_app/ui-react/primitives/${name}`)) as Record<string, any>

// ── checkbox · switch · tabs · segmented-control · collapsible · scroll-area · avatar ──

import { Search as SolidSearch, Settings as SolidSettings } from "lucide-solid"
import { Checkbox } from "./checkbox"
import { Switch } from "./switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./tabs"
import { SegmentedControl } from "./segmented-control"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./collapsible"
import { ScrollArea } from "./scroll-area"
import { Avatar, AvatarFallback, AvatarImage } from "./avatar"

describe(`plain controls parity`, () => {
  let RC: Record<string, any>
  let lucide: Record<string, any>
  beforeAll(async () => {
    RC = Object.assign({}, ...(await Promise.all([`checkbox`, `switch`, `tabs`, `segmented-control`, `collapsible`, `scroll-area`, `avatar`].map(reactModule))))
    lucide = (await import(/* @vite-ignore */ `lucide-react`)) as Record<string, any>
  })

  const checkboxStates: [string, Record<string, unknown>][] = [
    [`unchecked`, {}],
    [`checked`, { defaultChecked: true }],
    [`indeterminate`, { checked: `indeterminate` }],
    [`disabled`, { disabled: true }],
    [`disabled checked`, { disabled: true, defaultChecked: true }],
    [`aria-invalid`, { "aria-invalid": true }],
    [`required`, { required: true, name: `terms` }],
  ]
  for (const [name, p] of checkboxStates) {
    it(`Checkbox ${name}`, async () => {
      await expectElementParity({ react: () => h(RC.Checkbox, { "aria-label": `Accept`, ...p }), solid: () => <Checkbox aria-label="Accept" {...(p as object)} /> })
    })
  }
  it(`Checkbox inside a <form>: the bubble input`, async () => {
    await expectElementParity({
      react: () => h(`form`, null, h(RC.Checkbox, { name: `terms`, defaultChecked: true }), h(RC.Checkbox, { name: `news`, value: `yes`, required: true })),
      solid: () => (
        <form>
          <Checkbox name="terms" defaultChecked />
          <Checkbox name="news" value="yes" required />
        </form>
      ),
    })
  })

  for (const size of [`default`, `sm`] as const) {
    for (const [name, p] of [
      [`unchecked`, {}],
      [`checked`, { defaultChecked: true }],
      [`disabled`, { disabled: true }],
    ] as [string, Record<string, unknown>][]) {
      it(`Switch ${size} ${name}`, async () => {
        await expectElementParity({ react: () => h(RC.Switch, { size, "aria-label": `Wi-Fi`, ...p }), solid: () => <Switch size={size} aria-label="Wi-Fi" {...(p as object)} /> })
      })
    }
  }
  it(`Switch inside a <form>`, async () => {
    await expectElementParity({ react: () => h(`form`, null, h(RC.Switch, { name: `wifi`, defaultChecked: true })), solid: () => <form><Switch name="wifi" defaultChecked /></form> })
  })

  it(`Tabs: 3 triggers (one disabled, the second active) + panels`, async () => {
    await expectElementParity({
      react: () =>
        h(
          RC.Tabs,
          { defaultValue: `b` },
          h(RC.TabsList, null, h(RC.TabsTrigger, { value: `a` }, `Account`), h(RC.TabsTrigger, { value: `b` }, `Password`), h(RC.TabsTrigger, { value: `c`, disabled: true }, `Billing`)),
          h(RC.TabsContent, { value: `a` }, `Account panel`),
          h(RC.TabsContent, { value: `b` }, `Password panel`),
          h(RC.TabsContent, { value: `c` }, `Billing panel`)
        ),
      solid: () => (
        <Tabs defaultValue="b">
          <TabsList>
            <TabsTrigger value="a">Account</TabsTrigger>
            <TabsTrigger value="b">Password</TabsTrigger>
            <TabsTrigger value="c" disabled>
              Billing
            </TabsTrigger>
          </TabsList>
          <TabsContent value="a">Account panel</TabsContent>
          <TabsContent value="b">Password panel</TabsContent>
          <TabsContent value="c">Billing panel</TabsContent>
        </Tabs>
      ),
    })
  })

  it(`SegmentedControl with icons, fill, a disabled option`, async () => {
    await expectElementParity({
      react: () =>
        h(RC.SegmentedControl, {
          value: `list`,
          onValueChange: () => {},
          fill: true,
          options: [
            { value: `list`, label: `List`, icon: lucide.SearchIcon },
            { value: `board`, label: `Board`, icon: lucide.SettingsIcon },
            { value: `off`, label: `Off`, disabled: true },
          ],
        }),
      solid: () => (
        <SegmentedControl
          value="list"
          onValueChange={() => {}}
          fill
          options={[
            { value: `list`, label: `List`, icon: SolidSearch },
            { value: `board`, label: `Board`, icon: SolidSettings },
            { value: `off`, label: `Off`, disabled: true },
          ]}
        />
      ),
    })
  })

  for (const open of [true, false]) {
    it(`Collapsible ${open ? `open` : `closed`}`, async () => {
      await expectElementParity({
        react: () => h(RC.Collapsible, { defaultOpen: open }, h(RC.CollapsibleTrigger, null, `Toggle`), h(RC.CollapsibleContent, null, `Hidden body`)),
        solid: () => (
          <Collapsible defaultOpen={open}>
            <CollapsibleTrigger>Toggle</CollapsibleTrigger>
            <CollapsibleContent>Hidden body</CollapsibleContent>
          </Collapsible>
        ),
      })
    })
  }

  it(`ScrollArea with overflowing content`, async () => {
    const rows = Array.from({ length: 30 }, (_, i) => `Row ${i + 1}`)
    await expectElementParity({
      react: () => h(RC.ScrollArea, { className: `h-40 w-48`, style: { height: 160 } }, ...rows.map((r) => h(`div`, { key: r }, r))),
      solid: () => (
        <ScrollArea class="h-40 w-48" style={{ height: `160px` }}>
          {rows.map((r) => (
            <div>{r}</div>
          ))}
        </ScrollArea>
      ),
    })
  })

  it(`Avatar: fallback (muted, and with a userId hue)`, async () => {
    await expectElementParity({
      react: () => h(Fragment, null, h(RC.Avatar, null, h(RC.AvatarFallback, null, `DS`)), h(RC.Avatar, null, h(RC.AvatarFallback, { userId: `user-42` }, `AB`))),
      solid: () => (
        <>
          <Avatar>
            <AvatarFallback>DS</AvatarFallback>
          </Avatar>
          <Avatar>
            <AvatarFallback userId="user-42">AB</AvatarFallback>
          </Avatar>
        </>
      ),
    })
  })

  it(`Avatar: an image that never loads in jsdom → the fallback after delayMs 0`, async () => {
    // The 0 ms timer fires after the render's tick: one more tick on each side
    // (inside act for React) before the bodies are compared.
    const r = await renderReactElement(() => h(RC.Avatar, null, h(RC.AvatarImage, { src: `/avatar.png`, alt: `Dana` }), h(RC.AvatarFallback, { delayMs: 0, userId: `dana` }, `DS`)))
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 5))
    })
    const react = serializeBody()
    r.cleanup()
    const s = await renderSolidElement(() => (
      <Avatar>
        <AvatarImage src="/avatar.png" alt="Dana" />
        <AvatarFallback delayMs={0} userId="dana">
          DS
        </AvatarFallback>
      </Avatar>
    ))
    await new Promise((resolve) => setTimeout(resolve, 5))
    const solid = serializeBody()
    s.cleanup()
    expect(react).toContain(`data-slot="avatar-fallback"`)
    expect(solid).toBe(react)
  })
})

// ── select ──

import * as SolidSelect from "./select"

describe(`Select parity`, () => {
  let R: Record<string, any>
  beforeAll(async () => {
    R = await reactModule(`select`)
  })
  const S = SolidSelect
  const reactItems = (disabledLast = false) => [
    h(R.SelectGroup, { key: `g1` }, h(R.SelectLabel, null, `Fruits`), h(R.SelectItem, { value: `apple` }, `Apple`), h(R.SelectItem, { value: `banana` }, `Banana`)),
    h(R.SelectSeparator, { key: `sep` }),
    h(R.SelectGroup, { key: `g2` }, h(R.SelectLabel, null, `Vegetables`), h(R.SelectItem, { value: `carrot` }, `Carrot`), h(R.SelectItem, { value: `leek`, disabled: disabledLast }, `Leek`)),
  ]
  const solidItems = (disabledLast = false) => (
    <>
      <S.SelectGroup>
        <S.SelectLabel>Fruits</S.SelectLabel>
        <S.SelectItem value="apple">Apple</S.SelectItem>
        <S.SelectItem value="banana">Banana</S.SelectItem>
      </S.SelectGroup>
      <S.SelectSeparator />
      <S.SelectGroup>
        <S.SelectLabel>Vegetables</S.SelectLabel>
        <S.SelectItem value="carrot">Carrot</S.SelectItem>
        <S.SelectItem value="leek" disabled={disabledLast}>
          Leek
        </S.SelectItem>
      </S.SelectGroup>
    </>
  )
  const reactSelect = (root: Record<string, unknown>, content: Record<string, unknown> = {}) =>
    h(R.Select, root, h(R.SelectTrigger, { className: `w-44` }, h(R.SelectValue, { placeholder: `Pick a fruit` })), h(R.SelectContent, content, ...reactItems(true)))
  const solidSelect = (root: Record<string, unknown>, content: Record<string, unknown> = {}) => (
    <S.Select {...root}>
      <S.SelectTrigger class="w-44">
        <S.SelectValue placeholder="Pick a fruit" />
      </S.SelectTrigger>
      <S.SelectContent {...content}>{solidItems(true)}</S.SelectContent>
    </S.Select>
  )

  it(`closed with a placeholder`, async () => {
    await expectElementParity({ react: () => reactSelect({}), solid: () => solidSelect({}) })
  })
  it(`closed with a value`, async () => {
    await expectElementParity({ react: () => reactSelect({ defaultValue: `banana` }), solid: () => solidSelect({ defaultValue: `banana` }) })
  })
  it(`closed inside a <form> (the hidden native select)`, async () => {
    await expectElementParity({ react: () => h(`form`, null, reactSelect({ defaultValue: `carrot`, name: `veg` })), solid: () => <form>{solidSelect({ defaultValue: `carrot`, name: `veg` })}</form> })
  })
  it(`a small trigger, disabled`, async () => {
    await expectElementParity({
      react: () => h(R.Select, { disabled: true }, h(R.SelectTrigger, { size: `sm` }, h(R.SelectValue, { placeholder: `Off` })), h(R.SelectContent, null, h(R.SelectItem, { value: `a` }, `A`))),
      solid: () => (
        <S.Select disabled>
          <S.SelectTrigger size="sm">
            <S.SelectValue placeholder="Off" />
          </S.SelectTrigger>
          <S.SelectContent>
            <S.SelectItem value="a">A</S.SelectItem>
          </S.SelectContent>
        </S.Select>
      ),
    })
  })
  it(`defaultOpen, item-aligned: groups, labels, separator, a disabled item`, async () => {
    await expectElementParity({ react: () => reactSelect({ defaultOpen: true, defaultValue: `banana` }), solid: () => solidSelect({ defaultOpen: true, defaultValue: `banana` }) })
  })
  it(`defaultOpen, item-aligned, no value`, async () => {
    await expectElementParity({ react: () => reactSelect({ defaultOpen: true }), solid: () => solidSelect({ defaultOpen: true }) })
  })
  it(`defaultOpen, position="popper"`, async () => {
    await expectElementParity({ react: () => reactSelect({ defaultOpen: true, defaultValue: `carrot` }, { position: `popper` }), solid: () => solidSelect({ defaultOpen: true, defaultValue: `carrot` }, { position: `popper` }) })
  })
})

// ── command ──

import * as SolidCommand from "./command"

type CommandCase = { query?: string; empty?: boolean; label?: string }

const commandReact = (R: Record<string, any>, c: CommandCase) => () =>
  h(
    R.Command,
    { label: c.label ?? `Commands`, className: `w-80` },
    h(R.CommandInput, { defaultValue: c.query, placeholder: `Search` }),
    h(
      R.CommandList,
      null,
      h(R.CommandEmpty, null, `No results.`),
      h(R.CommandGroup, { heading: `Fruit` }, h(R.CommandItem, { value: `Apple` }, `Apple`), h(R.CommandItem, { value: `Banana` }, `Banana`, h(R.CommandShortcut, null, `⌘B`)), h(R.CommandItem, { value: `Pineapple`, disabled: true }, `Pineapple`)),
      h(R.CommandSeparator, null),
      c.empty ? null : h(R.CommandGroup, { heading: `Tools` }, h(R.CommandItem, { value: `Hammer`, keywords: [`nail`] }, `Hammer`), h(R.CommandItem, { value: `Apple pie tool` }, `Apple pie tool`))
    )
  )

const commandSolid = (c: CommandCase) => () => {
  const S = SolidCommand
  return (
    <S.Command label={c.label ?? `Commands`} class="w-80">
      <S.CommandInput defaultValue={c.query} placeholder="Search" />
      <S.CommandList>
        <S.CommandEmpty>No results.</S.CommandEmpty>
        <S.CommandGroup heading="Fruit">
          <S.CommandItem value="Apple">Apple</S.CommandItem>
          <S.CommandItem value="Banana">
            Banana
            <S.CommandShortcut>⌘B</S.CommandShortcut>
          </S.CommandItem>
          <S.CommandItem value="Pineapple" disabled>
            Pineapple
          </S.CommandItem>
        </S.CommandGroup>
        <S.CommandSeparator />
        {c.empty ? null : (
          <S.CommandGroup heading="Tools">
            <S.CommandItem value="Hammer" keywords={[`nail`]}>
              Hammer
            </S.CommandItem>
            <S.CommandItem value="Apple pie tool">Apple pie tool</S.CommandItem>
          </S.CommandGroup>
        )}
      </S.CommandList>
    </S.Command>
  )
}

describe(`command parity`, () => {
  const cases: [string, CommandCase][] = [
    [`two groups, no query`, {}],
    [`a query that filters and reorders`, { query: `apple` }],
    [`a query matching a keyword (one group hidden)`, { query: `nail` }],
    [`a query matching nothing: the empty state`, { query: `zzz` }],
  ]
  for (const [name, c] of cases) {
    it(name, async () => {
      const R = await reactModule(`command`)
      await expectElementParity({ react: commandReact(R, c), solid: commandSolid(c) })
    })
  }
})
