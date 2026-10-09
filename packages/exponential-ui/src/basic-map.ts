// VAPP-85 (D14): the A2UI basic catalog → core catalog map, applied by the
// reducer. The declarative part is catalog/basic-map.json; the NAMED
// transforms listed under its `transforms` are implemented here, one function
// per name, in the words of the table.

import basicMapJson from "../catalog/basic-map.json" with { type: "json" }
import type { Action, ChildTemplate, FlatComponent } from "./types"

interface PropRule {
  from?: string
  map?: Record<string, unknown>
  default?: unknown
  const?: unknown
}
interface ComponentRule {
  to: string
  props: Record<string, string | PropRule>
  children?: `children` | `child`
  transform?: string
}

export const basicMap = basicMapJson as unknown as {
  from: string
  to: string
  version: string
  placeholderIcon: string
  components: Record<string, ComponentRule>
  transforms: Record<string, string>
  icons: Record<string, string | null>
  functions: Record<string, string>
}

/** What a basic component becomes before the reducer nests its children. */
export interface MappedComponent {
  component: string
  props: Record<string, unknown>
  childrenIds: string[]
  template?: ChildTemplate
  slots: Record<string, string>
  on?: Record<string, Action>
  style?: Record<string, unknown>
}

export function mapBasicIcon(name: unknown): string {
  if (typeof name !== `string`) return basicMap.placeholderIcon
  return basicMap.icons[name] ?? basicMap.placeholderIcon
}

function isBinding(value: unknown): value is { path: string } {
  return typeof value === `object` && value !== null && typeof (value as { path?: unknown }).path === `string`
}

type Transform = (out: MappedComponent, flat: FlatComponent, lookup: (id: string) => FlatComponent | undefined) => void

const transforms: Record<string, Transform> = {
  textVariant(out, flat) {
    const variant = flat.variant
    if (variant === `h1` || variant === `h2` || variant === `h3` || variant === `h4`) {
      out.component = `Heading`
      out.props = { text: out.props.text, level: variant }
      return
    }
    out.props.variant = variant === `h5` ? `label` : variant === `caption` ? `caption` : `body`
  },
  imageVariant(out, flat) {
    const variant = (flat.variant as string | undefined) ?? `mediumFeature`
    if (variant === `avatar`) {
      out.component = `Avatar`
      out.props = { src: out.props.src, name: flat.description ?? `` }
      return
    }
    const style: Record<string, unknown> =
      variant === `icon`
        ? { width: 24, height: 24 }
        : variant === `smallFeature`
          ? { width: 120, height: 120 }
          : variant === `header`
            ? { width: `100%`, aspectRatio: 3 }
            : { width: `100%`, aspectRatio: 1.7777778 }
    out.style = { ...(out.style ?? {}), ...style }
  },
  iconName(out, flat) {
    out.props.name = mapBasicIcon(flat.name)
  },
  tabs(out, flat) {
    const tabs = Array.isArray(flat.tabs) ? (flat.tabs as { title: unknown; child: string }[]) : []
    out.props.tabs = tabs.map((tab) => ({ label: tab.title, value: tab.child }))
    out.childrenIds = tabs.map((tab) => tab.child)
    if (tabs.length > 0) out.props.value = tabs[0].child
  },
  modal(out, flat) {
    if (typeof flat.trigger === `string`) out.slots.trigger = flat.trigger
    if (typeof flat.content === `string`) out.childrenIds = [flat.content]
    out.props.open = false
  },
  buttonChild(out, flat, lookup) {
    const childId = typeof flat.child === `string` ? flat.child : undefined
    const child = childId ? lookup(childId) : undefined
    if (child?.component === `Text`) out.props.label = child.text
    else if (child?.component === `Icon`) out.props.icon = mapBasicIcon(child.name)
    else if (childId) out.childrenIds = [childId]
    if (out.props.icon !== undefined && out.props.label === undefined) out.props.size = `icon`
    if (flat.action !== undefined) out.on = { press: flat.action as Action }
  },
  textField(out, flat) {
    out.props.name = flat.id
    const variant = (flat.variant as string | undefined) ?? `shortText`
    if (variant === `longText`) out.component = `Textarea`
    else out.props.type = variant === `number` ? `number` : variant === `obscured` ? `password` : `text`
    if (typeof flat.validationRegexp === `string`) {
      const value = isBinding(flat.value) ? flat.value.path : flat.id
      const checks = Array.isArray(out.props.checks) ? [...(out.props.checks as unknown[])] : []
      checks.push({
        condition: { call: `regex`, args: { value: { path: value }, pattern: flat.validationRegexp } },
        message: `Invalid value`,
      })
      out.props.checks = checks
    }
  },
  fieldName(out, flat) {
    out.props.name = flat.id
  },
  choicePicker(out, flat) {
    out.props.name = flat.id
    const multiple = flat.variant === `multipleSelection`
    const value = flat.value
    const joined = Array.isArray(value)
      ? multiple
        ? (value as string[]).join(`,`)
        : ((value as string[])[0] ?? ``)
      : value
    if (joined !== undefined) out.props.value = joined
    if (flat.filterable === true) {
      out.component = `Select`
      out.props.searchable = true
      out.props.multiple = multiple
    } else if (flat.displayStyle === `chips`) {
      out.component = `ToggleGroup`
      out.props = { items: out.props.options, type: multiple ? `multiple` : `single`, variant: `outline`, value: out.props.value }
    } else if (multiple) {
      out.component = `Select`
      out.props.multiple = true
    }
  },
}

/** Map one basic component; `null` when the table has no row (→ Unknown). */
export function mapBasicComponent(
  flat: FlatComponent,
  lookup: (id: string) => FlatComponent | undefined
): MappedComponent | null {
  const rule = basicMap.components[flat.component]
  if (!rule) return null
  const props: Record<string, unknown> = {}
  for (const [coreProp, spec] of Object.entries(rule.props)) {
    if (typeof spec === `string`) {
      if (flat[spec] !== undefined) props[coreProp] = flat[spec]
      continue
    }
    if (`const` in spec) {
      props[coreProp] = spec.const
      continue
    }
    let value = spec.from !== undefined ? flat[spec.from] : undefined
    if (spec.map && typeof value === `string` && value in spec.map) value = spec.map[value]
    if (value === undefined && spec.default !== undefined) value = spec.default
    if (value !== undefined) props[coreProp] = value
  }
  const out: MappedComponent = { component: rule.to, props, childrenIds: [], slots: {} }
  if (rule.children === `children`) {
    const children = flat.children
    if (Array.isArray(children)) out.childrenIds = children
    else if (children && typeof children === `object`)
      out.template = { component: children.componentId, path: children.path, ...(typeof children.key === `string` ? { key: children.key } : {}) }
  } else if (rule.children === `child` && typeof flat.child === `string`) {
    out.childrenIds = [flat.child]
  }
  if (rule.transform) {
    const fn = transforms[rule.transform]
    if (!fn) throw new Error(`basic-map.json names an unimplemented transform: ${rule.transform}`)
    fn(out, flat, lookup)
  }
  return out
}

export const BASIC_TRANSFORM_NAMES: readonly string[] = Object.keys(transforms)
