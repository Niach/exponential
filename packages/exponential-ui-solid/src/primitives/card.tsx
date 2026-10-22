import { splitProps, type ComponentProps } from "solid-js"
import { CARD_CLASSES } from "@exponential-at/ui/web/primitives"

import { cn } from "./cn"

type Part = keyof typeof CARD_CLASSES

/** One `<div data-slot="card-…">` per part: the parts differ only by slot
 *  and recipe. */
const part = (slot: string, recipe: Part) =>
  function CardPart(props: ComponentProps<`div`>) {
    const [local, rest] = splitProps(props, [`class`])
    return <div data-slot={slot} class={cn(CARD_CLASSES[recipe], local.class)} {...rest} />
  }

const Card = part(`card`, `root`)
const CardHeader = part(`card-header`, `header`)
const CardTitle = part(`card-title`, `title`)
const CardDescription = part(`card-description`, `description`)
const CardAction = part(`card-action`, `action`)
const CardContent = part(`card-content`, `content`)
const CardFooter = part(`card-footer`, `footer`)

export { Card, CardHeader, CardFooter, CardTitle, CardAction, CardDescription, CardContent }
