// FEED-131: the `overlays` lane's jsdom parity. Every specimen of
// harness/primitives/overlays.tsx (the browser gate's twins) renders on both
// sides, whole <body> compared (portals, focus guards, `aria-hidden`
// siblings, the body's scroll lock and pointer-events), plus the arms and
// states a single specimen does not show.

import { createElement as h, type ReactElement } from "react"
import type { JSX } from "solid-js"
import { SPECIMENS as SOLID } from "../../harness/primitives/overlays"
import { expectElementParity, renderReactElement, renderSolidElement, serializeBody } from "../test/parity"
import * as S from "./index"

// The React side's JSX stays out of this package's Solid transform: loaded
// at run time (see src/test/parity.ts).
const REACT_SPECIMENS = `${import.meta.dirname}/../../../exponential-ui-react/harness/primitives/overlays.tsx`
const REACT_PRIMITIVES = `@exponential_app/eui-react/primitives`
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyComponent = (props: any) => ReactElement
let react: Record<string, () => ReactElement>
let R: Record<string, AnyComponent>

beforeAll(async () => {
  react = ((await import(/* @vite-ignore */ REACT_SPECIMENS)) as { SPECIMENS: Record<string, () => ReactElement> }).SPECIMENS
  R = (await import(/* @vite-ignore */ REACT_PRIMITIVES)) as Record<string, AnyComponent>
})

const parity = (react: () => ReactElement, solid: () => JSX.Element) => expectElementParity({ react, solid })

/** `DIALOG_PHONE_QUERY` (and every other max-width query) matches. */
function phone(on: boolean) {
  const original = window.matchMedia
  if (on)
    window.matchMedia = ((query: string) => ({ matches: query.includes(`max-width`), media: query, onchange: null, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, dispatchEvent: () => false })) as never
  return () => {
    window.matchMedia = original
  }
}

describe(`overlay primitives: DOM parity with React`, () => {
  it(`both harnesses define the same specimen ids`, () => {
    expect(Object.keys(SOLID).sort()).toEqual(Object.keys(react).sort())
  })

  for (const id of Object.keys(SOLID)) {
    it(id, async () => {
      await parity(react[id]!, SOLID[id]!)
    })
  }
})

describe(`Dialog`, () => {
  it(`closed: the trigger alone`, async () => {
    await parity(
      () => h(R.Dialog!, null, h(R.DialogTrigger!, null, `Open`), h(R.DialogContent!, null, h(R.DialogTitle!, null, `T`))),
      () => (
        <S.Dialog>
          <S.DialogTrigger>Open</S.DialogTrigger>
          <S.DialogContent>
            <S.DialogTitle>T</S.DialogTitle>
          </S.DialogContent>
        </S.Dialog>
      )
    )
  })

  for (const mobile of [`sheet`, `sheet-full`, `alert`, `page`] as const) {
    it(`open, the ${mobile} arm at desktop width`, async () => {
      await parity(
        () => h(R.Dialog!, { defaultOpen: true }, h(R.DialogTrigger!, null, `Open`), h(R.DialogContent!, { mobile }, h(R.DialogHeader!, null, h(R.DialogTitle!, null, `Title`), h(R.DialogDescription!, null, `Description`)), h(R.DialogBody!, null, `Body`), h(R.DialogFooter!, null, h(R.DialogCancel!, null), h(R.Button!, null, `Save`)))),
        () => (
          <S.Dialog defaultOpen>
            <S.DialogTrigger>Open</S.DialogTrigger>
            <S.DialogContent mobile={mobile}>
              <S.DialogHeader>
                <S.DialogTitle>Title</S.DialogTitle>
                <S.DialogDescription>Description</S.DialogDescription>
              </S.DialogHeader>
              <S.DialogBody>Body</S.DialogBody>
              <S.DialogFooter>
                <S.DialogCancel />
                <S.Button>Save</S.Button>
              </S.DialogFooter>
            </S.DialogContent>
          </S.Dialog>
        )
      )
    })
  }

  it(`open without the ✕, a custom class, a controlled open`, async () => {
    await parity(
      () => h(R.Dialog!, { open: true, onOpenChange: () => {} }, h(R.DialogContent!, { showCloseButton: false, className: `sm:max-w-md` }, h(R.DialogTitle!, null, `T`))),
      () => (
        <S.Dialog open onOpenChange={() => {}}>
          <S.DialogContent showCloseButton={false} class="sm:max-w-md">
            <S.DialogTitle>T</S.DialogTitle>
          </S.DialogContent>
        </S.Dialog>
      )
    )
  })

  for (const mobile of [`sheet`, `sheet-full`, `alert`] as const) {
    it(`the phone path (${mobile})`, async () => {
      const restore = phone(true)
      try {
        await parity(
          () => h(R.Dialog!, { defaultOpen: true }, h(R.DialogContent!, { mobile }, h(R.DialogHeader!, null, h(R.DialogTitle!, null, `Title`)), h(R.DialogFooter!, null, h(R.DialogCancel!, null)))),
          () => (
            <S.Dialog defaultOpen>
              <S.DialogContent mobile={mobile}>
                <S.DialogHeader>
                  <S.DialogTitle>Title</S.DialogTitle>
                </S.DialogHeader>
                <S.DialogFooter>
                  <S.DialogCancel />
                </S.DialogFooter>
              </S.DialogContent>
            </S.Dialog>
          )
        )
      } finally {
        restore()
      }
    })
  }
})

describe(`Sheet`, () => {
  for (const side of [`top`, `right`, `bottom`, `left`] as const) {
    it(`open on the ${side}`, async () => {
      await parity(
        () => h(R.Sheet!, { defaultOpen: true }, h(R.SheetTrigger!, null, `Open`), h(R.SheetContent!, { side }, h(R.SheetHeader!, null, h(R.SheetTitle!, null, `Title`), h(R.SheetDescription!, null, `D`)), h(R.SheetFooter!, null, `F`))),
        () => (
          <S.Sheet defaultOpen>
            <S.SheetTrigger>Open</S.SheetTrigger>
            <S.SheetContent side={side}>
              <S.SheetHeader>
                <S.SheetTitle>Title</S.SheetTitle>
                <S.SheetDescription>D</S.SheetDescription>
              </S.SheetHeader>
              <S.SheetFooter>F</S.SheetFooter>
            </S.SheetContent>
          </S.Sheet>
        )
      )
    })
  }

  it(`a bottom sheet without its grabber, a side one without its ✕`, async () => {
    await parity(
      () => h(R.Sheet!, { defaultOpen: true }, h(R.SheetContent!, { side: `bottom`, showGrabber: false }, h(R.SheetTitle!, null, `A`))),
      () => (
        <S.Sheet defaultOpen>
          <S.SheetContent side="bottom" showGrabber={false}>
            <S.SheetTitle>A</S.SheetTitle>
          </S.SheetContent>
        </S.Sheet>
      )
    )
    await parity(
      () => h(R.Sheet!, { defaultOpen: true }, h(R.SheetContent!, { side: `left`, showCloseButton: false }, h(R.SheetTitle!, null, `A`))),
      () => (
        <S.Sheet defaultOpen>
          <S.SheetContent side="left" showCloseButton={false}>
            <S.SheetTitle>A</S.SheetTitle>
          </S.SheetContent>
        </S.Sheet>
      )
    )
  })
})

describe(`Popover`, () => {
  for (const [side, align] of [
    [`bottom`, `center`],
    [`top`, `start`],
    [`right`, `end`],
    [`left`, `center`],
  ] as const) {
    it(`open, ${side} ${align}`, async () => {
      await parity(
        () => h(R.Popover!, { defaultOpen: true }, h(R.PopoverTrigger!, null, `Open`), h(R.PopoverContent!, { side, align }, `Body`)),
        () => (
          <S.Popover defaultOpen>
            <S.PopoverTrigger>Open</S.PopoverTrigger>
            <S.PopoverContent side={side} align={align}>
              Body
            </S.PopoverContent>
          </S.Popover>
        )
      )
    })
  }

  it(`modal, with an anchor`, async () => {
    await parity(
      () => h(R.Popover!, { defaultOpen: true, modal: true }, h(R.PopoverAnchor!, null, `Anchor`), h(R.PopoverTrigger!, null, `Open`), h(R.PopoverContent!, null, h(`button`, { type: `button` }, `Inside`))),
      () => (
        <S.Popover defaultOpen modal>
          <S.PopoverAnchor>Anchor</S.PopoverAnchor>
          <S.PopoverTrigger>Open</S.PopoverTrigger>
          <S.PopoverContent>
            <button type="button">Inside</button>
          </S.PopoverContent>
        </S.Popover>
      )
    )
  })
})

describe(`Tooltip, HoverCard, AlertDialog`, () => {
  it(`a tooltip defaultOpen, an aria-label`, async () => {
    await parity(
      () => h(R.TooltipProvider!, null, h(R.Tooltip!, { defaultOpen: true }, h(R.TooltipTrigger!, null, `Hover`), h(R.TooltipContent!, { "aria-label": `Spoken` }, `Shown`))),
      () => (
        <S.TooltipProvider>
          <S.Tooltip defaultOpen>
            <S.TooltipTrigger>Hover</S.TooltipTrigger>
            <S.TooltipContent aria-label="Spoken">Shown</S.TooltipContent>
          </S.Tooltip>
        </S.TooltipProvider>
      )
    )
  })

  it(`a hover card defaultOpen, its tabbables out of the tab order`, async () => {
    await parity(
      () => h(R.HoverCard!, { defaultOpen: true }, h(R.HoverCardTrigger!, { href: `#u` }, `@user`), h(R.HoverCardContent!, { side: `top` }, h(`a`, { href: `#x` }, `Profile`))),
      () => (
        <S.HoverCard defaultOpen>
          <S.HoverCardTrigger href="#u">@user</S.HoverCardTrigger>
          <S.HoverCardContent side="top">
            <a href="#x">Profile</a>
          </S.HoverCardContent>
        </S.HoverCard>
      )
    )
  })

  it(`an alert dialog closed, then open`, async () => {
    for (const open of [false, true]) {
      await parity(
        () => h(R.AlertDialog!, { defaultOpen: open }, h(R.AlertDialogTrigger!, null, `Delete`), h(R.AlertDialogContent!, null, h(R.AlertDialogHeader!, null, h(R.AlertDialogTitle!, null, `Sure?`), h(R.AlertDialogDescription!, null, `Gone.`)), h(R.AlertDialogFooter!, null, h(R.AlertDialogCancel!, null, `Cancel`), h(R.AlertDialogAction!, null, `Delete`)))),
        () => (
          <S.AlertDialog defaultOpen={open}>
            <S.AlertDialogTrigger>Delete</S.AlertDialogTrigger>
            <S.AlertDialogContent>
              <S.AlertDialogHeader>
                <S.AlertDialogTitle>Sure?</S.AlertDialogTitle>
                <S.AlertDialogDescription>Gone.</S.AlertDialogDescription>
              </S.AlertDialogHeader>
              <S.AlertDialogFooter>
                <S.AlertDialogCancel>Cancel</S.AlertDialogCancel>
                <S.AlertDialogAction>Delete</S.AlertDialogAction>
              </S.AlertDialogFooter>
            </S.AlertDialogContent>
          </S.AlertDialog>
        )
      )
    }
  })
})

describe(`DropdownMenu`, () => {
  // A `defaultOpen` sub closes at once (the root content takes the focus,
  // a focus outside the sub): the open case is a controlled one.
  for (const subOpen of [false, true]) {
    it(`every item kind, the sub menu ${subOpen ? `open` : `closed`}`, async () => {
      const noop = () => {}
      await parity(
        () =>
          h(
            R.DropdownMenu!,
            { defaultOpen: true },
            h(R.DropdownMenuTrigger!, null, `Menu`),
            h(
              R.DropdownMenuContent!,
              null,
              h(R.DropdownMenuLabel!, null, `Account`),
              h(R.DropdownMenuGroup!, null, h(R.DropdownMenuItem!, null, `Profile`, h(R.DropdownMenuShortcut!, null, `⌘P`)), h(R.DropdownMenuItem!, { inset: true }, `Billing`), h(R.DropdownMenuItem!, { disabled: true }, `Team`), h(R.DropdownMenuItem!, { variant: `destructive` }, `Delete`)),
              h(R.DropdownMenuSeparator!, null),
              h(R.DropdownMenuCheckboxItem!, { checked: true, onCheckedChange: noop }, `On`),
              h(R.DropdownMenuCheckboxItem!, { checked: false, onCheckedChange: noop }, `Off`),
              h(R.DropdownMenuRadioGroup!, { value: `b`, onValueChange: noop }, h(R.DropdownMenuRadioItem!, { value: `a` }, `A`), h(R.DropdownMenuRadioItem!, { value: `b` }, `B`)),
              h(R.DropdownMenuSub!, subOpen ? { open: true, onOpenChange: noop } : { defaultOpen: true }, h(R.DropdownMenuSubTrigger!, { inset: true }, `More`), h(R.DropdownMenuSubContent!, null, h(R.DropdownMenuItem!, null, `Nested`)))
            )
          ),
        () => (
          <S.DropdownMenu defaultOpen>
            <S.DropdownMenuTrigger>Menu</S.DropdownMenuTrigger>
            <S.DropdownMenuContent>
              <S.DropdownMenuLabel>Account</S.DropdownMenuLabel>
              <S.DropdownMenuGroup>
                <S.DropdownMenuItem>
                  Profile<S.DropdownMenuShortcut>⌘P</S.DropdownMenuShortcut>
                </S.DropdownMenuItem>
                <S.DropdownMenuItem inset>Billing</S.DropdownMenuItem>
                <S.DropdownMenuItem disabled>Team</S.DropdownMenuItem>
                <S.DropdownMenuItem variant="destructive">Delete</S.DropdownMenuItem>
              </S.DropdownMenuGroup>
              <S.DropdownMenuSeparator />
              <S.DropdownMenuCheckboxItem checked onCheckedChange={noop}>
                On
              </S.DropdownMenuCheckboxItem>
              <S.DropdownMenuCheckboxItem checked={false} onCheckedChange={noop}>
                Off
              </S.DropdownMenuCheckboxItem>
              <S.DropdownMenuRadioGroup value="b" onValueChange={noop}>
                <S.DropdownMenuRadioItem value="a">A</S.DropdownMenuRadioItem>
                <S.DropdownMenuRadioItem value="b">B</S.DropdownMenuRadioItem>
              </S.DropdownMenuRadioGroup>
              <S.DropdownMenuSub {...(subOpen ? { open: true, onOpenChange: noop } : { defaultOpen: true })}>
                <S.DropdownMenuSubTrigger inset>More</S.DropdownMenuSubTrigger>
                <S.DropdownMenuSubContent>
                  <S.DropdownMenuItem>Nested</S.DropdownMenuItem>
                </S.DropdownMenuSubContent>
              </S.DropdownMenuSub>
            </S.DropdownMenuContent>
          </S.DropdownMenu>
        )
      )
    })
  }

  it(`closed: the trigger alone`, async () => {
    await parity(
      () => h(R.DropdownMenu!, null, h(R.DropdownMenuTrigger!, null, `Menu`), h(R.DropdownMenuContent!, null, h(R.DropdownMenuItem!, null, `X`))),
      () => (
        <S.DropdownMenu>
          <S.DropdownMenuTrigger>Menu</S.DropdownMenuTrigger>
          <S.DropdownMenuContent>
            <S.DropdownMenuItem>X</S.DropdownMenuItem>
          </S.DropdownMenuContent>
        </S.DropdownMenu>
      )
    )
  })
})

describe(`Toaster`, () => {
  const fill = (store: S.ToastStore) => {
    store.toast.success(`Saved`, { id: `a`, duration: Infinity, height: 54 })
    store.toast.error(`Failed`, { id: `b`, duration: Infinity, height: 74, description: `Try again`, action: { label: `Retry` } })
    store.toast.info(`Heads up`, { id: `c`, duration: Infinity, height: 54 })
  }
  for (const touch of [false, true]) {
    it(`three toasts collapsed then expanded (${touch ? `touch` : `pointer`})`, async () => {
      const r = S.createToastStore()
      const s = S.createToastStore()
      fill(r)
      fill(s)
      const expand = async () => {
        const list = document.querySelector(`[data-slot="toast-list"]`)!
        if (touch) list.dispatchEvent(Object.assign(new Event(`pointerdown`, { bubbles: true }), { pointerType: `touch` }))
        else {
          // React's onPointerEnter listens to `pointerover`, Solid's to `pointerenter`.
          list.dispatchEvent(Object.assign(new Event(`pointerover`, { bubbles: true }), { pointerType: `mouse` }))
          list.dispatchEvent(Object.assign(new Event(`pointerenter`), { pointerType: `mouse` }))
        }
        await new Promise((res) => setTimeout(res, 0))
      }
      const { act } = await import(`react`)
      const rr = await renderReactElement(() => h(R.Toaster!, { store: r, touch }))
      const collapsedR = serializeBody()
      await act(expand)
      const expandedR = serializeBody()
      rr.cleanup()
      const ss = await renderSolidElement(() => <S.Toaster store={s} touch={touch} />)
      const collapsedS = serializeBody()
      await expand()
      const expandedS = serializeBody()
      ss.cleanup()
      expect(collapsedS).toBe(collapsedR)
      expect(expandedS).toBe(expandedR)
      expect(expandedR).not.toBe(collapsedR)
      expect(expandedR).toContain(`data-expanded="true"`)
    })
  }

  it(`a dismissed toast leaves both stacks`, async () => {
    await parity(
      () => {
        const store = S.createToastStore()
        store.toast(`Only`, { id: `x`, duration: Infinity, height: 54 })
        store.dismiss(`x`)
        return h(R.Toaster!, { store, touch: false })
      },
      () => {
        const store = S.createToastStore()
        store.toast(`Only`, { id: `x`, duration: Infinity, height: 54 })
        store.dismiss(`x`)
        return <S.Toaster store={store} touch={false} />
      }
    )
  })
})
