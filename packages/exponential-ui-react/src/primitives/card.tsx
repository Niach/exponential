import * as React from "react"
import { CARD_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

function Card({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card"
      className={cn(CARD_CLASSES.root, className)}
      {...props}
    />
  )
}

function CardHeader({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card-header"
      className={cn(CARD_CLASSES.header, className)}
      {...props}
    />
  )
}

function CardTitle({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card-title"
      className={cn(CARD_CLASSES.title, className)}
      {...props}
    />
  )
}

function CardDescription({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card-description"
      className={cn(CARD_CLASSES.description, className)}
      {...props}
    />
  )
}

function CardAction({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card-action"
      className={cn(CARD_CLASSES.action, className)}
      {...props}
    />
  )
}

function CardContent({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card-content"
      className={cn(CARD_CLASSES.content, className)}
      {...props}
    />
  )
}

function CardFooter({ className, ...props }: React.ComponentProps<`div`>) {
  return (
    <div
      data-slot="card-footer"
      className={cn(CARD_CLASSES.footer, className)}
      {...props}
    />
  )
}

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardAction,
  CardDescription,
  CardContent,
}
