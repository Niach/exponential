import * as React from "react"
import { TABLE_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Table({ className, ...props }: React.ComponentProps<`table`>) {
  return (
    <div data-slot="table-container" className={TABLE_CLASSES.container}>
      <table data-slot="table" className={cn(TABLE_CLASSES.root, className)} {...props} />
    </div>
  )
}

function TableHeader({ className, ...props }: React.ComponentProps<`thead`>) {
  return <thead data-slot="table-header" className={cn(TABLE_CLASSES.header, className)} {...props} />
}

function TableBody({ className, ...props }: React.ComponentProps<`tbody`>) {
  return <tbody data-slot="table-body" className={cn(TABLE_CLASSES.body, className)} {...props} />
}

function TableFooter({ className, ...props }: React.ComponentProps<`tfoot`>) {
  return <tfoot data-slot="table-footer" className={cn(TABLE_CLASSES.footer, className)} {...props} />
}

/** A selected row says so with `data-state="selected"` (shadcn's convention). */
function TableRow({ className, ...props }: React.ComponentProps<`tr`>) {
  return <tr data-slot="table-row" className={cn(TABLE_CLASSES.row, className)} {...props} />
}

function TableHead({ className, ...props }: React.ComponentProps<`th`>) {
  return <th data-slot="table-head" className={cn(TABLE_CLASSES.head, className)} {...props} />
}

function TableCell({ className, ...props }: React.ComponentProps<`td`>) {
  return <td data-slot="table-cell" className={cn(TABLE_CLASSES.cell, className)} {...props} />
}

function TableCaption({ className, ...props }: React.ComponentProps<`caption`>) {
  return <caption data-slot="table-caption" className={cn(TABLE_CLASSES.caption, className)} {...props} />
}

export { Table, TableHeader, TableBody, TableFooter, TableRow, TableHead, TableCell, TableCaption }
