// FEED-131: the `overlays` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "overlays";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/overlays.tsx.
// Each overlay opens on mount (`defaultOpen`); layout is inline style; the
// toast stack is a fixed store (fixed ids and heights, sticky toasts).
import { useMemo, useState, type CSSProperties, type ReactElement } from "react"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
  Button,
  Dialog,
  DialogBody,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
  Toaster,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  createToastStore,
} from "../../src/primitives"

const ROW: CSSProperties = { display: `flex`, flexWrap: `wrap`, alignItems: `center`, gap: 8 }
const SPACED: CSSProperties = { display: `flex`, justifyContent: `center`, paddingTop: 96 }
const noop = () => {}

function DialogBodyCopy() {
  return (
    <>
      <DialogHeader>
        <DialogTitle>Edit profile</DialogTitle>
        <DialogDescription>Make changes to your profile here.</DialogDescription>
      </DialogHeader>
      <DialogBody>
        <p>Your name and email show on every issue you create.</p>
      </DialogBody>
      <DialogFooter>
        <DialogCancel />
        <Button>Save changes</Button>
      </DialogFooter>
    </>
  )
}

function SheetBodyCopy({ title }: { title: string }) {
  return (
    <>
      <SheetHeader>
        <SheetTitle>{title}</SheetTitle>
        <SheetDescription>Pick the boards this view shows.</SheetDescription>
      </SheetHeader>
      <SheetFooter>
        <Button>Apply</Button>
      </SheetFooter>
    </>
  )
}

function MenuItems() {
  const [status, setStatus] = useState(true)
  const [activity, setActivity] = useState(false)
  const [position, setPosition] = useState(`bottom`)
  const keepOpen = (event: Event) => event.preventDefault()
  return (
    <>
      <DropdownMenuLabel>My account</DropdownMenuLabel>
      <DropdownMenuGroup>
        <DropdownMenuItem>
          Profile
          <DropdownMenuShortcut>⇧⌘P</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem inset>Billing</DropdownMenuItem>
        <DropdownMenuItem disabled>Team</DropdownMenuItem>
        <DropdownMenuItem variant="destructive">Delete</DropdownMenuItem>
      </DropdownMenuGroup>
      <DropdownMenuSeparator />
      <DropdownMenuCheckboxItem checked={status} onCheckedChange={setStatus} onSelect={keepOpen}>
        Status bar
      </DropdownMenuCheckboxItem>
      <DropdownMenuCheckboxItem checked={activity} onCheckedChange={setActivity} onSelect={keepOpen}>
        Activity bar
      </DropdownMenuCheckboxItem>
      <DropdownMenuSeparator />
      <DropdownMenuLabel inset>Panel position</DropdownMenuLabel>
      <DropdownMenuRadioGroup value={position} onValueChange={setPosition}>
        <DropdownMenuRadioItem value="top">Top</DropdownMenuRadioItem>
        <DropdownMenuRadioItem value="bottom">Bottom</DropdownMenuRadioItem>
        <DropdownMenuRadioItem value="right">Right</DropdownMenuRadioItem>
      </DropdownMenuRadioGroup>
    </>
  )
}

function ToastStack() {
  const store = useMemo(() => {
    const s = createToastStore()
    s.toast.info(`Session started on studio-mac`, { id: `t1`, duration: Infinity, height: 54 })
    s.toast.error(`Could not merge the pull request`, { id: `t2`, duration: Infinity, height: 74, action: { label: `Retry` } })
    s.toast.success(`Issue EXP-1031 created`, { id: `t3`, duration: Infinity, height: 76, description: `Added to the Inbox board.` })
    return s
  }, [])
  return <Toaster store={store} touch={false} data-xui-target="" />
}

function ToastLive() {
  const store = useMemo(() => createToastStore(), [])
  return (
    <>
      <Button data-xui-target="" onClick={() => store.toast.success(`Saved`, { id: `live`, duration: 600, height: 54 })}>
        Notify
      </Button>
      <Toaster store={store} touch={false} />
    </>
  )
}

export const SPECIMENS: Record<string, () => ReactElement> = {
  "overlays/dialog-open": () => (
    <Dialog defaultOpen>
      <DialogTrigger asChild>
        <Button variant="outline">Edit profile</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogBodyCopy />
      </DialogContent>
    </Dialog>
  ),
  "overlays/dialog-sheet-phone": () => (
    <Dialog defaultOpen>
      <DialogTrigger asChild>
        <Button variant="outline">Edit profile</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogBodyCopy />
      </DialogContent>
    </Dialog>
  ),
  "overlays/sheet-right": () => (
    <Sheet defaultOpen>
      <SheetTrigger asChild>
        <Button variant="outline">Filters</Button>
      </SheetTrigger>
      <SheetContent side="right">
        <SheetBodyCopy title="Filters" />
      </SheetContent>
    </Sheet>
  ),
  "overlays/sheet-bottom": () => (
    <Sheet defaultOpen>
      <SheetTrigger asChild>
        <Button variant="outline">Boards</Button>
      </SheetTrigger>
      <SheetContent side="bottom">
        <SheetBodyCopy title="Boards" />
      </SheetContent>
    </Sheet>
  ),
  "overlays/alert-dialog": () => (
    <AlertDialog defaultOpen>
      <AlertDialogTrigger asChild>
        <Button variant="destructive">Delete board</Button>
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete this board?</AlertDialogTitle>
          <AlertDialogDescription>Its issues move to the trash for 48 hours.</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction>Delete</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  ),
  "overlays/popover": () => (
    <div style={SPACED}>
      <Popover defaultOpen>
        <PopoverTrigger asChild>
          <Button variant="outline">Dimensions</Button>
        </PopoverTrigger>
        <PopoverContent>
          <p>Set the dimensions for the layer.</p>
        </PopoverContent>
      </Popover>
    </div>
  ),
  "overlays/tooltip": () => (
    <div style={SPACED}>
      <TooltipProvider>
        <Tooltip defaultOpen>
          <TooltipTrigger asChild>
            <Button variant="outline">Add to library</Button>
          </TooltipTrigger>
          <TooltipContent>Add to library</TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </div>
  ),
  "overlays/hover-card": () => (
    <div style={SPACED}>
      <HoverCard defaultOpen>
        <HoverCardTrigger asChild>
          <Button variant="link">@exponential</Button>
        </HoverCardTrigger>
        <HoverCardContent>
          <p>The real-time issue tracker. Joined October 2026.</p>
        </HoverCardContent>
      </HoverCard>
    </div>
  ),
  "overlays/dropdown-menu": () => (
    <DropdownMenu defaultOpen>
      <DropdownMenuTrigger asChild>
        <Button variant="outline">Open menu</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <MenuItems />
      </DropdownMenuContent>
    </DropdownMenu>
  ),
  "overlays/dropdown-menu-sub": () => (
    <DropdownMenu defaultOpen>
      <DropdownMenuTrigger asChild>
        <Button variant="outline">Open menu</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuItem>New team</DropdownMenuItem>
        <DropdownMenuSub open onOpenChange={noop}>
          <DropdownMenuSubTrigger>Invite users</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuItem>Email</DropdownMenuItem>
            <DropdownMenuItem>Message</DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem>More…</DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger variant="destructive">Danger zone</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuItem variant="destructive">Delete team</DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
  ),
  "overlays/toast-stack": () => <ToastStack />,
  /** Harness-only (the browser interaction suite): a 600 ms toast per press. */
  "overlays/toast-live": () => <ToastLive />,
  "overlays/triggers": () => (
    <TooltipProvider>
      <div style={ROW}>
        <Dialog>
          <DialogTrigger asChild>
            <Button variant="outline">Dialog</Button>
          </DialogTrigger>
          <DialogContent>
            <DialogBodyCopy />
          </DialogContent>
        </Dialog>
        <Sheet>
          <SheetTrigger asChild>
            <Button variant="outline">Sheet</Button>
          </SheetTrigger>
          <SheetContent>
            <SheetBodyCopy title="Sheet" />
          </SheetContent>
        </Sheet>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="outline">Alert</Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogTitle>Sure?</AlertDialogTitle>
          </AlertDialogContent>
        </AlertDialog>
        <Popover>
          <PopoverTrigger asChild>
            <Button variant="outline">Popover</Button>
          </PopoverTrigger>
          <PopoverContent>Popover</PopoverContent>
        </Popover>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" data-xui-target="">
              Tooltip
            </Button>
          </TooltipTrigger>
          <TooltipContent>Shown on hover</TooltipContent>
        </Tooltip>
        <HoverCard>
          <HoverCardTrigger asChild>
            <Button variant="link">Hover card</Button>
          </HoverCardTrigger>
          <HoverCardContent>Card</HoverCardContent>
        </HoverCard>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline">Menu</Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuItem>Item</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        <PlainDialogTrigger />
      </div>
    </TooltipProvider>
  ),
}

/** A plain (non-`asChild`) trigger: Radix's own `<button>`. */
function PlainDialogTrigger() {
  return (
    <Dialog>
      <DialogTrigger>Plain trigger</DialogTrigger>
      <DialogContent>
        <DialogBodyCopy />
      </DialogContent>
    </Dialog>
  )
}
