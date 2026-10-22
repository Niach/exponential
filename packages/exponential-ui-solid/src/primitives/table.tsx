import { splitProps, type ComponentProps } from "solid-js"
import { TABLE_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Table(props: ComponentProps<`table`>) {
  const [local, rest] = splitProps(props, [`class`])
  return (
    <div data-slot="table-container" class={TABLE_CLASSES.container}>
      <table data-slot="table" class={cn(TABLE_CLASSES.root, local.class)} {...rest} />
    </div>
  )
}

function TableHeader(props: ComponentProps<`thead`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <thead data-slot="table-header" class={cn(TABLE_CLASSES.header, local.class)} {...rest} />
}

function TableBody(props: ComponentProps<`tbody`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <tbody data-slot="table-body" class={cn(TABLE_CLASSES.body, local.class)} {...rest} />
}

function TableFooter(props: ComponentProps<`tfoot`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <tfoot data-slot="table-footer" class={cn(TABLE_CLASSES.footer, local.class)} {...rest} />
}

/** A selected row says so with `data-state="selected"` (shadcn's convention). */
function TableRow(props: ComponentProps<`tr`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <tr data-slot="table-row" class={cn(TABLE_CLASSES.row, local.class)} {...rest} />
}

function TableHead(props: ComponentProps<`th`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <th data-slot="table-head" class={cn(TABLE_CLASSES.head, local.class)} {...rest} />
}

function TableCell(props: ComponentProps<`td`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <td data-slot="table-cell" class={cn(TABLE_CLASSES.cell, local.class)} {...rest} />
}

function TableCaption(props: ComponentProps<`caption`>) {
  const [local, rest] = splitProps(props, [`class`])
  return <caption data-slot="table-caption" class={cn(TABLE_CLASSES.caption, local.class)} {...rest} />
}

export { Table, TableHeader, TableBody, TableFooter, TableRow, TableHead, TableCell, TableCaption }
