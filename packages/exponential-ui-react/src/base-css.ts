// VAPP-87: the structural CSS every surface carries (layer `xui-base`, the
// lowest). It restates taffy's defaults on EVERY painted element — the three
// "CSS equals taffy" rules of the VAPP-4 spike:
//   1. `box-sizing: border-box; display: flex; flex-direction: row;
//      position: relative` on every node, `min-width` left at `auto`
//      (never `0` as a default: that broke 5/48 frames at 390);
//   2. control chrome is paint or lives INSIDE the measured size (borders
//      start at 0 and only a recipe/style adds one, which the core also
//      counts as layout);
//   3. `overflow: hidden` is emitted as `overflow: clip` (theme-css.ts).
// Natives then add their own structure under `[data-xui-c="…"]`; a theme
// never needs to (recipes carry only visuals, sizes and spacing).

import { ANIMATION_NAMES, ANIMATION_PROPERTIES_CSS, RTL_MIRRORED_ICONS, keyframesCss } from "@exponential-at/ui"

/** Round 2 §2: the `animation` key's keyframe sets (style.json
 *  `animations`; the individual `translate`/`rotate`/`scale` properties
 *  compose outside a node's own `transform`; opacity moves `--xui-a-opacity`,
 *  which the node's `opacity` multiplies), the animated custom properties
 *  registered so they interpolate (`ANIMATION_PROPERTIES_CSS`: that one and
 *  the shimmer band's `--xui-band`), and the glyphs that mirror
 *  in an rtl node (catalog/locale.json `rtlMirroredIcons`, by the element's
 *  `dir`, so a node's own direction counts). */
const ROUND2_CSS = `${ANIMATION_NAMES.map(keyframesCss).join(`\n`)}
:is(${RTL_MIRRORED_ICONS.map((n) => `svg[data-icon="${n}"]`).join(`,`)}):dir(rtl){scale:-1 1}`

export const BASE_CSS = `${ANIMATION_PROPERTIES_CSS}
@layer xui-base{
${ROUND2_CSS}
.xui-surface{display:block;position:relative;font-family:var(--xui-font-sans);color:var(--xui-color-foreground);font-size:var(--xui-type-size-sm);line-height:var(--xui-type-lineHeight-sm);-webkit-font-smoothing:antialiased}
.xui-surface *{box-sizing:border-box}
.xui-container{container-type:inline-size;container-name:xui;width:100%;position:relative}
:where([data-xui-part]){border:0 solid transparent}
.xui-el{box-sizing:border-box;display:flex;flex-direction:row;position:relative;margin:0;padding:0;border:0 solid transparent;min-width:auto;min-height:auto;font:inherit;color:inherit;background:transparent;text-align:start;text-decoration:none;outline:none;appearance:none;-webkit-appearance:none}
button:where([data-xui-part],.xui-calendar-nav,.xui-table-sort,.xui-carousel-nav){margin:0;padding:0;border:0 solid transparent;background:transparent;color:inherit;font:inherit;appearance:none;-webkit-appearance:none}
.xui-el:focus-visible{outline:2px solid var(--xui-color-ring);outline-offset:1px}
.xui-leaf{flex-direction:column}
.xui-leaf>.xui-measured{flex-shrink:0}
[data-xui-c="Text"].xui-el{display:block;white-space:pre-wrap;overflow-wrap:anywhere}
[data-xui-c="Text"][data-lines="1"].xui-el{white-space:nowrap;overflow:clip;text-overflow:ellipsis;min-width:0}
[data-xui-c="Text"][data-clamp].xui-el{display:-webkit-box;-webkit-box-orient:vertical;overflow:clip}
[data-xui-c="Text"][data-xui-align="start"].xui-el{text-align:start}
[data-xui-c="Text"][data-xui-align="center"].xui-el{text-align:center}
[data-xui-c="Text"][data-xui-align="end"].xui-el{text-align:end}
[data-xui-c="Button"].xui-el,[data-xui-c="Toggle"].xui-el,[data-xui-c="Link"].xui-el{display:inline-flex;align-items:center;justify-content:center;white-space:nowrap;cursor:pointer;user-select:none;flex-shrink:0;transition:background-color var(--xui-motion-fast) var(--xui-ease-standard,ease),color var(--xui-motion-fast) var(--xui-ease-standard,ease),opacity var(--xui-motion-fast) var(--xui-ease-standard,ease),border-color var(--xui-motion-fast) var(--xui-ease-standard,ease)}
[data-xui-c="Button"].xui-el:disabled,[data-xui-c="Toggle"].xui-el:disabled{cursor:default;pointer-events:none}
[data-xui-c="Link"].xui-el{text-decoration:underline;text-underline-offset:3px}
.xui-Button-icon,.xui-Button-spinner,.xui-Toggle-icon{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}
.xui-Button-icon>svg,.xui-Toggle-icon>svg,.xui-Button-spinner>svg,.xui-icon>svg{width:100%;height:100%}
.xui-icon{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}
[data-xui-c="Icon"].xui-el{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}
[data-xui-c="Icon"].xui-el>svg{width:100%;height:100%}
[data-xui-c="Image"].xui-el{overflow:clip}
[data-xui-c="Image"].xui-el>img{display:block;position:absolute;inset:0;width:100%;height:100%}
.xui-image-placeholder{display:flex;align-items:center;justify-content:center;width:100%;height:100%;min-height:24px;background:repeating-linear-gradient(45deg,color-mix(in srgb,currentColor 12%,transparent) 0 6px,color-mix(in srgb,currentColor 4%,transparent) 6px 12px);color:var(--xui-color-mutedForeground);font-size:var(--xui-type-size-xs)}
[data-xui-c="Video"].xui-el{overflow:clip}
.xui-media-sizer{display:block;max-width:100%;height:0;flex:0 1 auto;visibility:hidden;pointer-events:none}
[data-xui-c="Video"].xui-el>video{display:block;position:absolute;inset:0;width:100%;height:100%;object-fit:contain}
[data-xui-c="AudioPlayer"].xui-el>audio{width:100%;display:block;flex-shrink:0}
[data-xui-c="Avatar"].xui-el{overflow:clip;flex-shrink:0;align-items:center;justify-content:center}
.xui-Avatar-image{width:100%;height:100%;object-fit:cover;border-radius:inherit}
.xui-Avatar-fallback{display:flex;align-items:center;justify-content:center;width:100%;height:100%;border-radius:inherit;text-transform:uppercase;user-select:none}
[data-xui-c="List"].xui-el{flex-direction:column;overflow:auto;min-height:0}
[data-xui-c="List"][data-r-direction="horizontal"].xui-el{flex-direction:row}
.xui-List-divider{flex-shrink:0;align-self:stretch;border-style:solid;border-width:0}
.xui-list-window{position:relative;width:100%;flex-shrink:0}
.xui-list-window[data-axis="horizontal"]{width:auto;align-self:flex-start}
.xui-list-item{position:absolute;left:0;right:0}
.xui-list-window[data-axis="horizontal"]>.xui-list-item{left:auto;right:auto;top:0}
.xui-list-row{display:flex;flex-direction:column;position:relative}
.xui-list-row[data-axis="horizontal"]{flex-direction:row}
.xui-list-gap{position:absolute;display:flex;pointer-events:none}
.xui-list-gap[data-axis="vertical"]{left:0;right:0;flex-direction:column;transform:translateY(-50%)}
.xui-list-gap[data-axis="horizontal"]{top:0;bottom:0;flex-direction:row;transform:translateX(-50%)}
.xui-list-gap[data-axis="horizontal"]:dir(rtl){transform:translateX(50%)}
.xui-List-section{display:flex;flex-direction:row;align-items:center;flex-shrink:0}
[data-xui-c="Resizable"].xui-el{flex-direction:row;align-items:stretch;min-width:0;min-height:0}
[data-xui-c="Resizable"][data-orientation="vertical"].xui-el{flex-direction:column}
.xui-Resizable-panel{display:flex;flex-direction:column;min-width:0;min-height:0;overflow:clip}
.xui-Resizable-handle{position:relative;display:flex;align-items:center;justify-content:center;flex:0 0 var(--xui-control-hairline);min-width:0;min-height:0;touch-action:none;user-select:none;outline:none;z-index:1}
[data-orientation="horizontal"]>.xui-Resizable-handle{cursor:col-resize}
[data-orientation="vertical"]>.xui-Resizable-handle{cursor:row-resize}
.xui-Resizable-handle:focus-visible{outline:2px solid var(--xui-color-ring);outline-offset:1px}
.xui-resize-hit{position:absolute}
[data-orientation="horizontal"]>.xui-Resizable-handle>.xui-resize-hit{top:0;bottom:0;left:50%;transform:translateX(-50%)}
[data-orientation="vertical"]>.xui-Resizable-handle>.xui-resize-hit{left:0;right:0;top:50%;transform:translateY(-50%)}
.xui-Resizable-grip{flex:none;pointer-events:none}
[data-xui-c="Carousel"].xui-el{flex-direction:column;overflow:clip}
.xui-carousel-track{display:flex;flex-direction:row;overflow-x:auto;scroll-snap-type:x mandatory;scrollbar-width:none;gap:var(--xui-spacing-sm)}
.xui-carousel-track::-webkit-scrollbar{display:none}
.xui-Carousel-page{flex:0 0 100%;scroll-snap-align:start;display:flex;flex-direction:column}
.xui-carousel-dots{display:flex;justify-content:center;gap:var(--xui-spacing-xs);padding-top:var(--xui-spacing-sm)}
.xui-Carousel-indicator{width:6px;height:6px;border-radius:9999px;background:var(--xui-color-border);padding:0;cursor:pointer}
.xui-Carousel-indicator[data-xs~="selected"]{background:var(--xui-color-primary)}
[data-xui-c="Tabs"].xui-el{flex-direction:column}
.xui-Tabs-list{display:flex;flex-direction:row;align-self:flex-start;flex-shrink:0}
.xui-Tabs-list[data-fill="true"]{align-self:stretch}
.xui-Tabs-tab{display:inline-flex;flex-direction:column;align-items:center;justify-content:center;gap:var(--xui-spacing-xs);cursor:pointer;white-space:nowrap;flex:0 0 auto;position:relative}
.xui-Tabs-list[data-fill="true"]>.xui-Tabs-tab{flex:1 1 0}
.xui-Tabs-tab-body{display:inline-flex;align-items:center;gap:var(--xui-spacing-xs)}
.xui-Tabs-indicator{position:absolute;left:0;right:0;bottom:0}
.xui-Tabs-content{display:flex;flex-direction:column;outline:none}
.xui-Tabs-content[hidden]{display:none}
[data-xui-c="Segmented"].xui-el{display:inline-flex;align-items:center;align-self:flex-start;flex-wrap:nowrap}
[data-xui-c="Segmented"][data-r-fill="true"].xui-el,[data-xui-c="Segmented"][data-r-variant="bar"].xui-el{align-self:stretch}
.xui-Segmented-item{display:inline-flex;align-items:center;justify-content:center;cursor:pointer;white-space:nowrap;flex:0 0 auto;min-width:0}
[data-xui-c="Segmented"][data-r-fill="true"].xui-el>.xui-Segmented-item,[data-xui-c="Segmented"][data-r-variant="bar"].xui-el>.xui-Segmented-item{flex:1 1 0}
[data-xui-c="Segmented"][data-r-variant="bar"].xui-el{align-items:stretch;height:var(--xui-control-tabBar);width:100%}
[data-xui-c="Segmented"][data-r-variant="bar"].xui-el>.xui-Segmented-item{flex-direction:column;gap:var(--xui-spacing-xxs);padding:var(--xui-spacing-xs) 0}
[data-xui-c="Segmented"][data-r-variant="bar"].xui-el .xui-Segmented-label{font-size:var(--xui-type-size-xs);line-height:var(--xui-type-lineHeight-xs);white-space:nowrap}
.xui-Segmented-icon{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}
.xui-Segmented-label{min-width:0;overflow:hidden;text-overflow:ellipsis}
[data-xui-c="Accordion"].xui-el{flex-direction:column}
.xui-Accordion-item{display:flex;flex-direction:column}
.xui-Accordion-trigger{display:flex;align-items:center;justify-content:space-between;width:100%;cursor:pointer;text-align:start}
.xui-Accordion-trigger>svg{transition:transform var(--xui-motion-fast) var(--xui-ease-standard,ease)}
.xui-Accordion-trigger[data-state="open"]>svg{transform:rotate(180deg)}
.xui-Accordion-content{display:flex;flex-direction:column;overflow:clip}
.xui-Accordion-content[hidden]{display:none}
.xui-Dialog-overlay,.xui-Drawer-overlay{position:fixed;inset:0;z-index:50}
.xui-Dialog-content{position:fixed;left:50%;top:50%;transform:translate(-50%,-50%);z-index:51;display:flex;flex-direction:column;width:calc(100% - 32px);max-width:480px;max-height:calc(100% - 32px);overflow:auto;outline:none}
.xui-Drawer-content{position:fixed;z-index:51;display:flex;flex-direction:column;overflow:auto;outline:none}
.xui-Drawer-content[data-side="bottom"]{left:0;right:0;bottom:0;max-height:85%}
.xui-Drawer-content[data-side="top"]{left:0;right:0;top:0;max-height:85%}
.xui-Drawer-content[data-side="left"]{left:0;top:0;bottom:0;width:min(85%,360px)}
.xui-Drawer-content[data-side="right"]{right:0;top:0;bottom:0;width:min(85%,360px)}
.xui-Drawer-handle{align-self:center;width:36px;height:4px;border-radius:9999px;background:var(--xui-color-border);flex-shrink:0}
.xui-overlay-head{display:flex;flex-direction:column;gap:var(--xui-spacing-xxs)}
.xui-overlay-body{display:flex;flex-direction:column;gap:var(--xui-spacing-md);min-height:0}
.xui-Dialog-footer,.xui-Drawer-footer{display:flex;flex-direction:row;justify-content:flex-end;flex-wrap:wrap}
.xui-Dialog-close{position:absolute;top:var(--xui-spacing-sm);inset-inline-end:var(--xui-spacing-sm);display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0}
.xui-Dialog-close>svg{width:16px;height:16px}
.xui-Popover-content,.xui-Tooltip-content,.xui-Menu-content,.xui-Select-content,.xui-TimePicker-list{z-index:52;display:flex;flex-direction:column;outline:none;max-height:var(--radix-popper-available-height,320px);overflow:auto}
.xui-Tooltip-content{max-width:280px}
.xui-Menu-item,.xui-Select-item,.xui-TimePicker-item{display:flex;align-items:center;cursor:pointer;user-select:none;white-space:nowrap;outline:none}
.xui-Menu-item[data-disabled],.xui-Select-item[data-disabled],.xui-Select-item[aria-disabled="true"]{pointer-events:none}
.xui-Menu-item>svg,.xui-Select-item>svg{width:16px;height:16px;flex-shrink:0}
.xui-Menu-separator{align-self:stretch;flex-shrink:0}
.xui-Menu-label{display:flex;align-items:center}
.xui-Menu-trigger{display:inline-flex;align-items:center;justify-content:center;white-space:nowrap;flex-shrink:0;cursor:pointer}
[data-xui-overlay-root].xui-el{flex-direction:row;align-self:flex-start;align-items:stretch}
[data-xui-overlay-root="stretch"].xui-el{align-self:stretch}
[data-xui-overlay-root="none"].xui-el{display:contents}
.xui-trigger{display:flex;flex-direction:row;align-items:stretch;flex:1 1 auto;min-width:0}
.xui-Tooltip-anchor{display:flex;flex-direction:row;flex:1 1 auto;min-width:0}
.xui-field-column{display:flex;flex-direction:column;gap:var(--xui-spacing-xs);min-width:0}
.xui-field-row{display:flex;flex-direction:row;align-items:flex-start;gap:var(--xui-spacing-sm);min-width:0}
.xui-field-text{display:flex;flex-direction:column;gap:var(--xui-spacing-xxs);min-width:0}
[data-xui-c="Input"].xui-el,[data-xui-c="Textarea"].xui-el,[data-xui-c="Select"].xui-el,[data-xui-c="DatePicker"].xui-el,[data-xui-c="DateRangePicker"].xui-el,[data-xui-c="TimePicker"].xui-el,[data-xui-c="NumberField"].xui-el,[data-xui-c="ChipInput"].xui-el,[data-xui-c="FileUpload"].xui-el,[data-xui-c="Slider"].xui-el,[data-xui-c="Radio"].xui-el,[data-xui-c="Composer"].xui-el{flex-direction:column;gap:var(--xui-spacing-xs);min-width:0}
[data-xui-c="Checkbox"].xui-el{flex-direction:row;align-items:flex-start;gap:var(--xui-spacing-sm);cursor:pointer}
[data-xui-c="Checkbox"].xui-el>.xui-field-text{flex:1 1 0%}
.xui-Checkbox-box{margin-top:max(0px,calc((var(--xui-type-lineHeight-sm) - var(--xui-control-checkbox)) / 2))}
[data-xui-c="Switch"].xui-el{flex-direction:row;align-items:center;cursor:pointer}
.xui-Switch-body{display:flex;flex-direction:column;gap:var(--xui-spacing-xxs);flex:1 1 0%;min-width:0}
.xui-Input-field,.xui-Textarea-field{display:block;width:100%;min-width:0;margin:0;outline:none;appearance:none;border:0 solid transparent;background:transparent;color:inherit;font:inherit}
.xui-Input-field::placeholder,.xui-Textarea-field::placeholder,.xui-Composer-field::placeholder{color:var(--xui-color-mutedForeground)}
.xui-Textarea-field{resize:vertical;line-height:var(--xui-type-lineHeight-sm);padding-top:var(--xui-spacing-sm);padding-bottom:var(--xui-spacing-sm)}
.xui-Input-label,.xui-Textarea-label,.xui-Select-label,.xui-DatePicker-label,.xui-DateRangePicker-label,.xui-TimePicker-label,.xui-NumberField-label,.xui-ChipInput-label,.xui-FileUpload-label,.xui-Slider-label,.xui-Radio-label,.xui-Checkbox-label,.xui-Switch-label{display:block;cursor:inherit}
.xui-Select-trigger,.xui-DatePicker-trigger,.xui-DateRangePicker-trigger,.xui-TimePicker-trigger{display:flex;flex-direction:row;align-items:center;justify-content:space-between;gap:var(--xui-spacing-sm);width:100%;min-width:0;cursor:pointer;text-align:start;white-space:nowrap}
.xui-Select-trigger>span,.xui-DatePicker-trigger>span,.xui-DateRangePicker-trigger>span,.xui-TimePicker-trigger>span{overflow:clip;text-overflow:ellipsis;min-width:0;flex:1 1 auto}
.xui-Select-trigger>svg,.xui-DatePicker-trigger>svg,.xui-DateRangePicker-trigger>svg,.xui-TimePicker-trigger>svg{width:16px;height:16px;flex-shrink:0;opacity:.6}
.xui-Select-search{display:flex;width:100%;min-width:0;margin:0 0 var(--xui-spacing-xs);padding:var(--xui-spacing-xs) var(--xui-spacing-sm);outline:none;border:0;border-bottom:1px solid var(--xui-color-border);background:transparent;color:inherit;font:inherit}
.xui-Select-check{display:inline-flex;margin-inline-start:auto;flex-shrink:0}
.xui-Checkbox-box{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0;padding:0;cursor:pointer}
.xui-Checkbox-check{display:inline-flex;align-items:center;justify-content:center}
.xui-Checkbox-check>svg{width:100%;height:100%}
[data-xui-c="Radio"][data-r-orientation="horizontal"].xui-el>.xui-radio-items{flex-direction:row;flex-wrap:wrap}
.xui-radio-items{display:flex;flex-direction:column;gap:inherit}
.xui-radio-row{display:flex;align-items:center;gap:var(--xui-spacing-sm);cursor:pointer}
.xui-Radio-item{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0;padding:0;cursor:pointer}
.xui-Switch-track{display:inline-flex;align-items:center;flex-shrink:0;padding:2px;cursor:pointer;transition:background-color var(--xui-motion-fast) var(--xui-ease-standard,ease)}
.xui-Switch-thumb{display:block;transition:transform var(--xui-motion-fast) var(--xui-ease-standard,ease);transform:translateX(0)}
.xui-Switch-track[data-state="checked"]>.xui-Switch-thumb{transform:translateX(var(--xui-switch-travel,12px))}
.xui-Switch-track[data-state="checked"]>.xui-Switch-thumb:dir(rtl){transform:translateX(calc(-1 * var(--xui-switch-travel,12px)))}
.xui-slider-head{display:flex;justify-content:space-between;gap:var(--xui-spacing-sm)}
.xui-Slider-track{position:relative;display:flex;align-items:center;width:100%;flex-grow:1;overflow:visible;touch-action:none;user-select:none}
.xui-slider-root{display:flex;align-items:center;width:100%;min-height:var(--xui-control-slider);touch-action:none;user-select:none;position:relative}
.xui-Slider-range{position:absolute;height:100%;left:0;border-radius:inherit}
.xui-Slider-thumb{display:block;cursor:grab;outline:none}
.xui-DatePicker-calendar,.xui-DateRangePicker-calendar{display:flex;flex-direction:column;gap:var(--xui-spacing-sm);padding:var(--xui-spacing-sm)}
.xui-calendar-head{display:flex;align-items:center;justify-content:space-between;gap:var(--xui-spacing-sm);font-weight:var(--xui-type-weight-medium)}
.xui-calendar-nav{display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;border-radius:var(--xui-radius-sm);cursor:pointer;padding:0}
.xui-calendar-nav:hover{background:var(--xui-color-accent)}
.xui-calendar-nav>svg{width:16px;height:16px}
.xui-calendar-grid{display:grid;grid-template-columns:repeat(7,32px);gap:2px}
.xui-calendar-weekday{display:flex;align-items:center;justify-content:center;height:24px;font-size:var(--xui-type-size-xs);color:var(--xui-color-mutedForeground)}
.xui-DatePicker-day,.xui-DateRangePicker-day{display:flex;align-items:center;justify-content:center;width:32px;height:32px;border-radius:var(--xui-radius-sm);cursor:pointer;padding:0;font-size:var(--xui-type-size-sm)}
.xui-DatePicker-day:hover,.xui-DateRangePicker-day:hover{background:var(--xui-color-accent)}
.xui-DatePicker-day[data-xs~="selected"],.xui-DateRangePicker-day[data-xs~="selected"]{background:var(--xui-color-primary);color:var(--xui-color-primaryForeground)}
.xui-DatePicker-day[data-outside],.xui-DateRangePicker-day[data-outside]{opacity:.4}
.xui-DatePicker-day:disabled,.xui-DateRangePicker-day:disabled{opacity:.3;cursor:default;pointer-events:none}
.xui-Composer-field{display:block;width:100%;min-width:0;margin:0;padding:0;outline:none;border:0;background:transparent;color:inherit;font:inherit;resize:none;max-height:200px}
.xui-composer-bar{display:flex;align-items:center;justify-content:flex-end;gap:var(--xui-spacing-xs)}
.xui-Composer-attachment{display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0}
.xui-Composer-send{display:inline-flex;align-items:center;justify-content:center;cursor:pointer;padding:0}
.xui-Composer-send>svg,.xui-Composer-attachment>svg{width:16px;height:16px}
.xui-Composer-send:disabled{opacity:.5;pointer-events:none}
[data-xui-c="Ring"].xui-el,[data-xui-c="Spinner"].xui-el{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}
[data-xui-c="Ring"].xui-el>svg,[data-xui-c="Spinner"].xui-el>svg{width:100%;height:100%;display:block}
.xui-Ring-label{position:absolute;inset:0;display:flex;align-items:center;justify-content:center}
[data-xui-c="Spinner"].xui-el>svg,.xui-Button-spinner>svg{animation:xui-spin 0.8s linear infinite}
[data-xui-c="Skeleton"].xui-el{animation:xui-pulse 2s cubic-bezier(.4,0,.6,1) infinite;flex-shrink:0}
[data-xui-c="Markdown"].xui-el{display:block;min-width:0;overflow-wrap:anywhere}
[data-xui-c="Markdown"][data-lines].xui-el{display:-webkit-box;-webkit-box-orient:vertical;overflow:clip}
.xui-md>:first-child{margin-top:0}
.xui-md>:last-child{margin-bottom:0}
.xui-Markdown-paragraph{margin:0 0 var(--xui-spacing-sm)}
.xui-Markdown-heading{margin:var(--xui-spacing-md) 0 var(--xui-spacing-xs);font-weight:var(--xui-type-weight-semibold);line-height:1.25}
.xui-md ul,.xui-md ol{margin:0 0 var(--xui-spacing-sm);padding-inline-start:1.4em}
.xui-md ul{list-style:disc}
.xui-md ol{list-style:decimal}
.xui-Markdown-listItem{margin:0}
.xui-Markdown-code{font-family:var(--xui-font-mono);font-size:.925em;padding:0 .3em;border-radius:4px;background:var(--xui-color-muted)}
.xui-Markdown-codeBlock{display:block;font-family:var(--xui-font-mono);font-size:var(--xui-type-size-xs);white-space:pre;overflow:auto;padding:var(--xui-spacing-sm);margin:0 0 var(--xui-spacing-sm);border-radius:var(--xui-radius-sm);background:var(--xui-color-muted)}
.xui-Markdown-quote{margin:0 0 var(--xui-spacing-sm);padding-inline-start:var(--xui-spacing-md);border-inline-start:2px solid var(--xui-color-border);color:var(--xui-color-mutedForeground)}
.xui-Markdown-link{color:var(--xui-color-primary);text-decoration:underline;text-underline-offset:3px}
.xui-Markdown-table{border-collapse:collapse;margin:0 0 var(--xui-spacing-sm);width:100%}
.xui-Markdown-table th,.xui-Markdown-table td{border:1px solid var(--xui-color-border);padding:var(--xui-spacing-xs) var(--xui-spacing-sm);text-align:start}
.xui-md img{max-width:100%;height:auto;border-radius:var(--xui-radius-sm)}
[data-xui-c="Chart"].xui-el{flex-direction:column;gap:var(--xui-spacing-xs);min-width:0}
.xui-Chart-legend,.xui-Chart-title{flex-shrink:0}
[data-xui-c="Chart"].xui-el>svg{width:100%;display:block}
.xui-Chart-legend{display:flex;flex-wrap:wrap;gap:var(--xui-spacing-sm);font-size:var(--xui-type-size-xs);color:var(--xui-color-mutedForeground)}
.xui-Chart-legend>span{display:inline-flex;align-items:center;gap:var(--xui-spacing-xxs)}
.xui-Chart-legend i{display:inline-block;width:8px;height:8px;border-radius:2px}
.xui-Chart-title{font-weight:var(--xui-type-weight-medium)}
[data-xui-c="TreeGuides"].xui-el{flex-direction:row;align-self:stretch;flex-shrink:0}
[data-xui-c="TreeGuides"].xui-el{overflow:visible}
.xui-tree-col{position:relative;align-self:stretch;flex-shrink:0;overflow:visible}
.xui-tree-col>.xui-TreeGuides-line{position:absolute;background:currentColor;display:block;padding:0;margin:0;box-sizing:border-box}
.xui-tree-col>.xui-TreeGuides-line[data-elbow]{background:none;border-style:solid;border-color:currentColor;border-top-width:0;border-right-width:0}
[data-xui-c="Unknown"].xui-el{flex-direction:column;gap:var(--xui-spacing-xxs)}
[data-xui-c="Box"][data-pressable="true"].xui-el{cursor:pointer}
.xui-layer{position:absolute;top:0;left:0;width:0;height:0;overflow:visible}
.xui-sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}

.xui-calendar-row{display:contents}
.xui-calendar-cell{display:flex;align-items:center;justify-content:center}
.xui-calendar-day-label{position:relative}
.xui-DatePicker-day[data-today]>.xui-calendar-day-label,.xui-DateRangePicker-day[data-today]>.xui-calendar-day-label{text-decoration:underline;text-underline-offset:3px}
.xui-DateRangePicker-day{position:relative}
.xui-DateRangePicker-range{position:absolute;inset:0;border-radius:inherit}
.xui-DateRangePicker-day[data-range]{background:var(--xui-color-accent)}
.xui-calendar-nav{color:inherit}
[data-xui-c="Form"].xui-el{flex-direction:column}
.xui-form-fields{display:contents;border:0;margin:0;padding:0;min-width:0}
.xui-Form-summary{display:flex;flex-direction:column}
.xui-form-summary-list{margin:0;padding-inline-start:1.2em}
.xui-Input-error,.xui-Textarea-error,.xui-NumberField-error,.xui-ChipInput-error,.xui-FileUpload-error,.xui-Select-error,.xui-Checkbox-error,.xui-Switch-error,.xui-Radio-error,.xui-DatePicker-error,.xui-DateRangePicker-error,.xui-TimePicker-error{display:flex;flex-direction:column;gap:2px}
[data-xui-numberfield]{display:flex;flex-direction:row;align-items:center;gap:var(--xui-spacing-xxs);min-width:0}
.xui-NumberField-input{flex:1 1 auto;min-width:0;width:100%;border:0;outline:none;background:transparent;color:inherit;font:inherit;padding:0;text-align:start;font-variant-numeric:tabular-nums}
.xui-NumberField-decrement,.xui-NumberField-increment,.xui-ChipInput-remove,.xui-FileUpload-remove,.xui-Toast-close,.xui-CodeBlock-copy{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0;padding:0;cursor:pointer}
.xui-NumberField-decrement>svg,.xui-NumberField-increment>svg,.xui-ChipInput-remove>svg,.xui-FileUpload-remove>svg,.xui-Toast-close>svg,.xui-CodeBlock-copy>svg{width:16px;height:16px}
.xui-NumberField-decrement:disabled,.xui-NumberField-increment:disabled{opacity:.4;pointer-events:none}
.xui-NumberField-unit{flex-shrink:0}
[data-xui-chipfield]{display:flex;flex-direction:row;flex-wrap:wrap;align-items:center;cursor:text}
.xui-chip-list{display:contents;list-style:none;margin:0;padding:0}
.xui-ChipInput-chip{display:inline-flex;align-items:center;flex-shrink:0;outline:none}
.xui-ChipInput-input{flex:1 1 80px;min-width:80px;border:0;outline:none;background:transparent;color:inherit;font:inherit;padding:0;height:var(--xui-type-lineHeight-sm)}
.xui-ChipInput-suggestions{display:flex;flex-direction:column;border:1px solid var(--xui-color-border);border-radius:var(--xui-radius-md);background:var(--xui-color-popover);color:var(--xui-color-popoverForeground);padding:var(--xui-spacing-xs);box-shadow:var(--xui-shadow-md)}
.xui-ChipInput-suggestion{display:flex;align-items:center;padding:var(--xui-spacing-xs) var(--xui-spacing-sm);border-radius:var(--xui-radius-sm);cursor:pointer}
.xui-ChipInput-suggestion[data-active]{background:var(--xui-color-accent);color:var(--xui-color-accentForeground)}
.xui-FileUpload-dropzone{display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center;cursor:pointer;outline:none}
.xui-FileUpload-dropzone[aria-disabled="true"]{cursor:default}
.xui-FileUpload-icon{display:inline-flex}
.xui-FileUpload-icon>svg{width:100%;height:100%}
.xui-FileUpload-title{font-weight:var(--xui-type-weight-medium)}
.xui-file-list{display:flex;flex-direction:column;gap:var(--xui-spacing-xs);list-style:none;margin:0;padding:0}
.xui-FileUpload-file{display:flex;flex-direction:row;align-items:center}
.xui-FileUpload-fileIcon{display:inline-flex;flex-shrink:0;opacity:.7}
.xui-FileUpload-fileIcon>svg{width:16px;height:16px}
.xui-FileUpload-browse{display:inline-flex;align-items:center;justify-content:center;white-space:nowrap;flex-shrink:0;height:var(--xui-control-buttonSm)}
.xui-FileUpload-fileName{flex:1 1 auto;min-width:0;overflow:clip;text-overflow:ellipsis;white-space:nowrap}
[data-xui-c="CodeBlock"].xui-el{flex-direction:column;min-width:0;overflow:clip}
.xui-CodeBlock-header{display:flex;flex-direction:row;align-items:center;justify-content:space-between}
.xui-codeblock-scroll{overflow:auto;min-width:0;outline:none}
.xui-codeblock-scroll[data-max-lines]{max-height:calc(var(--xui-code-lines) * var(--xui-type-lineHeight-sm))}
.xui-codeblock-pre{margin:0;padding:0;font:inherit;white-space:pre;tab-size:2;min-width:max-content}
.xui-codeblock-scroll[data-wrap="true"]>.xui-codeblock-pre{white-space:pre-wrap;overflow-wrap:anywhere;min-width:0}
.xui-codeblock-pre>code{font:inherit;display:block}
.xui-CodeBlock-line{display:flex;flex-direction:row}
.xui-codeblock-text{flex:0 1 auto;min-width:0}
.xui-CodeBlock-gutter{display:flex;flex-direction:row;justify-content:flex-end;flex-shrink:0;user-select:none}
.xui-CodeBlock-lineNumber{display:block}
[data-xui-c="Table"].xui-el{flex-direction:column;min-width:0;overflow:auto}
.xui-Table-header{display:flex;flex-direction:column}
.xui-Table-header[data-sticky="true"]{position:sticky;top:0;z-index:1}
.xui-table-row,.xui-Table-row{display:grid;align-items:stretch;min-width:0}
.xui-table-body{display:flex;flex-direction:column;position:relative}
.xui-table-check{display:flex;align-items:center;justify-content:center;min-width:0}
.xui-Table-headerCell,.xui-Table-cell{display:flex;flex-direction:row;align-items:center;min-width:0;overflow:clip;text-overflow:ellipsis;white-space:nowrap;gap:var(--xui-spacing-xs)}
.xui-Table-headerCell[data-r-align="end"],.xui-Table-cell[data-r-align="end"]{justify-content:flex-end;text-align:end}
.xui-Table-headerCell[data-r-align="center"],.xui-Table-cell[data-r-align="center"]{justify-content:center;text-align:center}
.xui-Table-cell[data-type="number"]{font-variant-numeric:tabular-nums}
.xui-Table-cell>svg{width:16px;height:16px}
.xui-Table-row{outline:none}
.xui-Table-row:focus-visible{box-shadow:inset 0 0 0 2px var(--xui-color-ring)}
.xui-Table-row[tabindex]{cursor:pointer}
.xui-table-sort{display:inline-flex;align-items:center;gap:var(--xui-spacing-xxs);padding:0;margin:0;border:0;background:transparent;color:inherit;font:inherit;cursor:pointer;min-width:0}
.xui-Table-headerCell[data-r-align="end"]>.xui-table-sort{flex-direction:row}
.xui-Table-sortIcon{display:inline-flex;flex-shrink:0}
.xui-Table-sortIcon>svg{width:100%;height:100%}
.xui-Table-empty{display:flex;justify-content:center}
.xui-table-badge{display:inline-flex;align-items:center;height:var(--xui-control-badge);padding:0 var(--xui-spacing-sm);border-radius:var(--xui-radius-full);background:var(--xui-color-secondary);color:var(--xui-color-secondaryForeground);font-size:var(--xui-type-size-xs)}
.xui-chart-plot{position:relative;width:100%;min-width:0;flex:1 1 0%;min-height:0;overflow:visible}
.xui-chart-svg{position:absolute;top:0;left:0;display:block;overflow:visible;outline:none}
.xui-chart-svg:focus-visible{outline:2px solid var(--xui-color-ring);outline-offset:2px;border-radius:var(--xui-radius-sm)}
.xui-Chart-grid{opacity:.7}
.xui-Chart-tooltip{display:flex;flex-direction:column;gap:2px;z-index:2;min-width:120px;white-space:nowrap}
.xui-chart-tip-row{display:inline-flex;align-items:center;gap:var(--xui-spacing-xxs)}
.xui-chart-tip-row>i{display:inline-block;width:8px;height:8px;border-radius:2px}
.xui-Chart-valueLabel{fill:currentColor}
[data-xui-c="Chart"][data-kind="sparkline"].xui-el{gap:0}
.xui-Image-fallback{display:flex;align-items:center;justify-content:center;position:absolute;inset:0}
.xui-Image-fallback>svg{width:var(--xui-control-iconLg);height:var(--xui-control-iconLg)}
[data-xui-c="Image"][data-state="loading"].xui-el{background:var(--xui-color-muted)}
.xui-carousel-controls{display:flex;align-items:center;justify-content:center}
.xui-carousel-controls>.xui-carousel-dots{padding-top:0}
.xui-carousel-nav{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0;padding:0;cursor:pointer}
.xui-carousel-nav:disabled{pointer-events:none}
.xui-carousel-nav>svg{width:var(--xui-control-iconSm);height:var(--xui-control-iconSm)}
.xui-menu-label{flex:1 1 auto;min-width:0}
.xui-Menu-shortcut{margin-inline-start:auto;padding-inline-start:var(--xui-spacing-lg)}
.xui-Menu-check{display:inline-flex;align-items:center;justify-content:center;flex-shrink:0}
.xui-Menu-check svg,.xui-Menu-submenuIndicator>svg{width:100%;height:100%}
.xui-Menu-submenuIndicator{display:inline-flex;margin-inline-start:auto;flex-shrink:0}
.xui-Select-searchbox{display:flex;align-items:center;gap:var(--xui-spacing-xs);padding-inline-start:var(--xui-spacing-sm)}
.xui-Select-searchbox>svg{width:16px;height:16px;opacity:.6;flex-shrink:0}
.xui-Select-searchbox>.xui-Select-search{margin:0;border-bottom:0;padding-inline-start:0}
.xui-Select-listbox,.xui-TimePicker-listbox{display:flex;flex-direction:column;outline:none;overflow:auto;max-height:280px}
.xui-Select-empty{display:flex}
.xui-TimePicker-list{min-width:var(--radix-popover-trigger-width)}
[data-xui-c="Toast"].xui-el{flex-direction:row;align-items:flex-start;width:min(360px,calc(100vw - 32px));pointer-events:auto}
.xui-Toast-icon{display:inline-flex;flex-shrink:0}
.xui-Toast-icon>svg{width:100%;height:100%}
.xui-toast-text{display:flex;flex-direction:column;gap:2px;flex:1 1 auto;min-width:0}
.xui-Toast-action{display:inline-flex;align-items:center;flex-shrink:0;cursor:pointer;white-space:nowrap}
.xui-toast-layer{position:fixed;bottom:var(--xui-spacing-lg);left:50%;transform:translateX(-50%);z-index:60;display:flex;flex-direction:column;align-items:center;gap:var(--xui-spacing-sm);pointer-events:none;width:max-content;max-width:calc(100vw - 32px)}
.xui-toast-layer>:nth-last-child(n+4){display:none}
.xui-Popover-content[data-open-on="hover"]{pointer-events:auto}
@keyframes xui-fade-out{to{opacity:0}}
@keyframes xui-pop-in{from{opacity:0;scale:.96}}
@keyframes xui-pop-out{to{opacity:0;scale:.96}}
@keyframes xui-slide-bottom-in{from{translate:0 100%}}
@keyframes xui-slide-bottom-out{to{translate:0 100%}}
@keyframes xui-slide-top-in{from{translate:0 -100%}}
@keyframes xui-slide-top-out{to{translate:0 -100%}}
@keyframes xui-slide-left-in{from{translate:-100% 0}}
@keyframes xui-slide-left-out{to{translate:-100% 0}}
@keyframes xui-slide-right-in{from{translate:100% 0}}
@keyframes xui-slide-right-out{to{translate:100% 0}}
@keyframes xui-toast-in{from{opacity:0;translate:0 12px}}
@keyframes xui-accordion-open{from{height:0}to{height:var(--radix-accordion-content-height)}}
@keyframes xui-accordion-close{from{height:var(--radix-accordion-content-height)}to{height:0}}
.xui-Popover-content[data-state="open"],.xui-Tooltip-content[data-state$="open"],.xui-Menu-content[data-state="open"],.xui-Select-content[data-state="open"],.xui-DatePicker-calendar[data-state="open"],.xui-DateRangePicker-calendar[data-state="open"],.xui-TimePicker-list[data-state="open"],.xui-Dialog-content[data-state="open"]{animation:xui-pop-in var(--xui-motion-fast) var(--xui-ease-decelerate,ease-out)}
.xui-Popover-content[data-state="closed"],.xui-Tooltip-content[data-state="closed"],.xui-Menu-content[data-state="closed"],.xui-Select-content[data-state="closed"],.xui-DatePicker-calendar[data-state="closed"],.xui-DateRangePicker-calendar[data-state="closed"],.xui-TimePicker-list[data-state="closed"],.xui-Dialog-content[data-state="closed"]{animation:xui-pop-out var(--xui-motion-fast) var(--xui-ease-accelerate,ease-in) forwards}
.xui-Dialog-overlay[data-state="open"],.xui-Drawer-overlay[data-state="open"]{animation:xui-fade-in var(--xui-motion-standard) var(--xui-ease-decelerate,ease-out)}
.xui-Dialog-overlay[data-state="closed"],.xui-Drawer-overlay[data-state="closed"]{animation:xui-fade-out var(--xui-motion-standard) var(--xui-ease-accelerate,ease-in) forwards}
.xui-Drawer-content[data-side="bottom"][data-state="open"]{animation:xui-slide-bottom-in var(--xui-motion-slow) var(--xui-ease-decelerate,ease-out)}
.xui-Drawer-content[data-side="bottom"][data-state="closed"]{animation:xui-slide-bottom-out var(--xui-motion-standard) var(--xui-ease-accelerate,ease-in) forwards}
.xui-Drawer-content[data-side="top"][data-state="open"]{animation:xui-slide-top-in var(--xui-motion-slow) var(--xui-ease-decelerate,ease-out)}
.xui-Drawer-content[data-side="top"][data-state="closed"]{animation:xui-slide-top-out var(--xui-motion-standard) var(--xui-ease-accelerate,ease-in) forwards}
.xui-Drawer-content[data-side="left"][data-state="open"]{animation:xui-slide-left-in var(--xui-motion-slow) var(--xui-ease-decelerate,ease-out)}
.xui-Drawer-content[data-side="left"][data-state="closed"]{animation:xui-slide-left-out var(--xui-motion-standard) var(--xui-ease-accelerate,ease-in) forwards}
.xui-Drawer-content[data-side="right"][data-state="open"]{animation:xui-slide-right-in var(--xui-motion-slow) var(--xui-ease-decelerate,ease-out)}
.xui-Drawer-content[data-side="right"][data-state="closed"]{animation:xui-slide-right-out var(--xui-motion-standard) var(--xui-ease-accelerate,ease-in) forwards}
.xui-Accordion-content[data-state="open"]{animation:xui-accordion-open var(--xui-motion-standard) var(--xui-ease-standard,ease)}
.xui-Accordion-content[data-state="closed"]{animation:xui-accordion-close var(--xui-motion-standard) var(--xui-ease-standard,ease)}
.xui-Tabs-content[data-state="active"]{animation:xui-fade-in var(--xui-motion-fast) var(--xui-ease-decelerate,ease-out)}
[data-xui-c="Toast"][data-state="open"].xui-el{animation:xui-toast-in var(--xui-motion-standard) var(--xui-ease-decelerate,ease-out)}
[data-xui-c="Toast"][data-state="closed"].xui-el{animation:xui-fade-out var(--xui-motion-standard) var(--xui-ease-accelerate,ease-in) forwards}
.xui-accordion-title{display:inline-flex;align-items:baseline;gap:var(--xui-spacing-xs);min-width:0}
.xui-Collapsible-chevron.xui-el{transition:transform var(--xui-motion-fast) var(--xui-ease-standard,ease)}
}
@media (prefers-reduced-motion: reduce){@layer xui-base{.xui-surface *,.xui-surface *::before,.xui-surface *::after{animation-duration:0ms !important;animation-iteration-count:1 !important;transition-duration:0ms !important;scroll-behavior:auto !important}}}
.xui-surface[data-xui-motion="reduce"] *,.xui-surface[data-xui-motion="reduce"] *::after{animation-duration:0ms !important;animation-iteration-count:1 !important;transition-duration:0ms !important}`
