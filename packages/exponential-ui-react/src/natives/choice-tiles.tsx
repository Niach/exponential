// EXP-1280: ChoiceTiles — picture tiles picking one value or several. The
// headless frame (grid, radio/checkbox buttons, roving keys) is the
// primitive's (`primitives/choice-tiles.tsx`, the app's plan card uses the
// same frame); here it wears the theme's recipe parts: `tile.<i>`
// (`selected`, `disabled`, `invalid`), `picture.<i>` (through the host's
// media request, so a relative `/api/attachments/{id}` loads in the app; a
// tile without an image keeps the box with the Image fallback glyph),
// `label.<i>`, `description.<i>`. The value is bindable, comma-joined when
// `type: multiple` (like Segmented); a pick writes it and fires
// `change {value}` (fixtures/interactions-round1.json `choiceTiles`).

import { useMemo, useRef } from "react"
import { useSurfaceContext } from "../context"
import { FieldErrors, useField } from "../form"
import type { NativeProps } from "../node-view"
import { useMediaSource } from "../media"
import { ChoiceTilesFrame, choiceColumns, choiceValues, type ChoiceTileOption } from "../primitives/choice-tiles"
import { useBoundState } from "./bound"
import { bool, BuiltinIcon, objects, str } from "./shared"
import { useParts } from "./shared"

function TilePicture({ src }: { src: string | null }) {
  const ctx = useSurfaceContext()
  const media = useMediaSource(ctx.host, src ?? ``)
  if (src && media.url && !media.error) return <img src={media.url} alt="" loading="lazy" decoding="async" />
  return <BuiltinIcon slot="Image.fallback" />
}

export function ChoiceTilesNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const multiple = str(props.type, `single`) === `multiple`
  const options = useMemo(
    () =>
      objects<Record<string, unknown>>(props.options).map(
        (o): ChoiceTileOption => ({ value: str(o.value), label: str(o.label), description: o.description === undefined ? null : str(o.description), image: typeof o.image === `string` && o.image ? o.image : null, disabled: bool(o.disabled) })
      ),
    [props.options]
  )
  const [value, setValue] = useBoundState(node, scope, `value`, str(props.value))
  const rootRef = useRef<HTMLDivElement | null>(null)
  // The field's focus target: the tab stop (the picked tile, else the first).
  const focusRef = useMemo(
    () => ({
      get current(): HTMLElement | null {
        return (rootRef.current?.querySelector(`button[tabindex="0"]`) ?? rootRef.current?.querySelector(`button`)) as HTMLElement | null
      },
    }),
    []
  )
  const f = useField({ node, domId, props, value, focusRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const picked = choiceValues(value, multiple)
  const states = (s: { selected: boolean; disabled: boolean }) => [s.selected && `selected`, s.disabled && `disabled`, f.invalid && `invalid`] as const
  const { className: rootClass, ...rootRest } = rootProps as Record<string, unknown>
  return (
    <ChoiceTilesFrame
      rootRef={rootRef}
      options={options}
      picked={picked}
      multiple={multiple}
      columns={choiceColumns(props.columns)}
      disabled={disabled}
      invalid={f.invalid}
      dir={ctx.direction === `rtl` ? `rtl` : `ltr`}
      onPick={(next) => {
        const joined = next.join(`,`)
        setValue(joined)
        f.change(joined)
        f.touch()
        void emit(`change`, { value: joined }, { value: joined })
      }}
      rootProps={{ ...rootRest, className: `${str(rootClass)} xui-choice-tiles`, "aria-describedby": f.describedBy }}
      tileProps={(s) => part.at(`tile`, s.index, ...states(s))}
      pictureProps={(s) => part.at(`picture`, s.index, ...states(s))}
      labelProps={(s) => part.at(`label`, s.index, ...states(s))}
      descriptionProps={(s) => part.at(`description`, s.index, ...states(s))}
      renderPicture={(src) => <TilePicture src={src} />}
      after={<FieldErrors part={part} field={f} />}
    />
  )
}
