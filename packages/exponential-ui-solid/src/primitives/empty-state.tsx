import { Show, children, splitProps, type ComponentProps, type JSX } from "solid-js"
import { EMPTY_STATE_CLASSES, type EmptyStateSize } from "@exponential-at/ui/web/primitives"

import type { IconComponent } from "../host"
import { PaintIcon } from "../icons"
import { cn } from "./cn"
import { IconDisc } from "./icon-disc"

/** lucide-react's `focusable="false"` (not in Solid's SVG attribute types). */
const UNFOCUSABLE = { focusable: `false` } as JSX.SvgSVGAttributes<SVGSVGElement>

export { EMPTY_STATE_SIZES } from "@exponential-at/ui/web/primitives"
export type { EmptyStateSize } from "@exponential-at/ui/web/primitives"

export function EmptyState(props: { icon?: IconComponent; title: string; description?: string; size?: EmptyStateSize; class?: string; children?: JSX.Element }) {
  const actions = children(() => props.children)
  return (
    <Show
      when={props.size !== `list`}
      fallback={
        <div data-slot="empty-state" data-size="list" role="status" class={cn(EMPTY_STATE_CLASSES.list, props.class)}>
          <Show when={props.icon}>{(icon) => <PaintIcon icon={icon()} aria-hidden="true" {...UNFOCUSABLE} class={EMPTY_STATE_CLASSES.listIcon} />}</Show>
          <span>{props.title}</span>
        </div>
      }
    >
      <div data-slot="empty-state" data-size="page" role="status" class={cn(EMPTY_STATE_CLASSES.page, props.class)}>
        <Show when={props.icon}>{(icon) => <IconDisc icon={icon()} />}</Show>
        <h2 class={EMPTY_STATE_CLASSES.pageTitle}>{props.title}</h2>
        <Show when={props.description}>
          <p class={EMPTY_STATE_CLASSES.pageDescription}>{props.description}</p>
        </Show>
        <Show when={actions()}>
          <div class={EMPTY_STATE_CLASSES.pageActions}>{actions()}</div>
        </Show>
      </div>
    </Show>
  )
}

export function ListEmpty(props: Omit<ComponentProps<`p`>, `title`>) {
  const [local, rest] = splitProps(props, [`class`, `children`])
  return (
    <p data-slot="list-empty" role="status" class={cn(EMPTY_STATE_CLASSES.listEmpty, local.class)} {...rest}>
      {local.children}
    </p>
  )
}

export function EmptyCta(props: Omit<ComponentProps<`button`>, `children` | `title`> & { icon: IconComponent; title: string; description: string }) {
  const [local, rest] = splitProps(props, [`icon`, `title`, `description`, `class`, `type`])
  return (
    <button type={local.type ?? `button`} data-slot="empty-cta" class={cn(EMPTY_STATE_CLASSES.cta, local.class)} {...rest}>
      <span class={EMPTY_STATE_CLASSES.ctaTitle}>
        <PaintIcon icon={local.icon} class={EMPTY_STATE_CLASSES.ctaIcon} />
        {local.title}
      </span>
      <span class={EMPTY_STATE_CLASSES.ctaDescription}>{local.description}</span>
    </button>
  )
}
