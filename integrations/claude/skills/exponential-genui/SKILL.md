---
name: exponential-genui
description: "Write genUI (A2UI v0.9) surfaces: the component model, bindings and calls, actions, layout and responsive rules, the client functions, style keys, tokens and the built-in themes. Use when composing an exponential_show card, an app template or any genUI surface; exponential_apps {op: \"catalog\"} lists the components and their props."
---

# genUI

> Generated from the genUI prompt and catalog (genUI 0.21.0) — do not edit; regenerate with packages/exponential-ui/scripts/generate-skills.ts.

genUI paints A2UI v0.9 surfaces the same on web, desktop, iOS and Android: an `exponential_show` card, an app template and every other surface use this one model.

## Rules

- Reply with A2UI v0.9 messages: createSurface{surfaceId, catalogId}, then updateComponents{surfaceId, components:[…]} (one id "root"; resend an id to change it), updateDataModel{surfaceId, path, value} (no value = remove). Keep data in the model; bind to it.
- A component = {id, component, …props, children?: [ids] | {componentId, path, key?}, slots?: {name: id}, on?: {event: action}, style?, visible?}. Children are ids, never inline; {componentId, path} repeats per item, key = its naming field.
- Include every required prop. A ~ prop takes a binding {path: "/json/pointer"} or a live call {call, args}. visible: false (or bound) hides.
- An action (on: {event: action}) = {event: {name, context?}} (sent to you; context bindings read after the component's own write) and/or {functionCall: {call, args}} (client-side: a function below or a namespaced host one like app.toast).
- Lay out with Page, Stack, Grid, Card, SplitView, Sidebar, List (past a screen); control row = Toolbar, side-by-side fields = FieldGroup, Cancel/submit row = FormActions; Box + style for the rest. Wrap fields in Form; a valid submit closes the Dialog or Drawer.
- Responsive: a ^ prop also takes {base, sm?, md?, lg?, xl?} (from that breakpoint up). Style conditions, one level, narrow first: "@media (min-width: $breakpoint.md)": {…} (also max-width, min-/max-height, in px or $breakpoint.*; orientation: portrait|landscape; hover: hover|none; prefers-reduced-motion: reduce), then ":hover", ":focus-visible", ":pressed". display: "none" hides.
- Prefer components over Markdown (feature lists = List + Row), labels and tones over colours; a colour is a $color.* token (light and dark apply), never #hex. Built-in copy (Cancel, Search…) is localized: leave it unset.
- One default Button per screen (FormActions submit or main action); others outline, secondary or ghost.

## Components

Their props are in exponential_apps {op: "catalog"} (only: [names] = just these). By group:

- layout: Box, Stack, Grid, Card, Window, Separator, List, Resizable, ScrollArea, Sidebar, SplitView, AppBar, Page, Toolbar, Embed
- list: Section, Group, Trace, TraceItem, Plan, Row, Tree, MessageBubble
- text: Text, Heading, Markdown, CodeBlock, Kbd, KbdGroup, Label
- media: Image, ZoomableImage, Icon, Video, AudioPlayer, Avatar, QrCode, Carousel, Mosaic, CallSurface, Motion, Canvas
- navigation: Tabs, Segmented, Accordion, Collapsible, Pagination, Breadcrumb, Outline, Stepper
- overlay: Dialog, Drawer, DockCard, AlertDialog, Popover, Tooltip, Menu, Toast
- feedback: Badge, Alert, Chip, EmptyState, Progress, Meter, Ring, Spinner, Skeleton, VoiceWave
- data: Table, Chart, Sparkline, StatTile, Calendar, Timeline, RecordForm, RecordTable
- action: Button, Link, Toggle
- input: Form, FieldGroup, FormActions, SettingsRow, Input, Textarea, NumberField, Checkbox, Radio, ChoiceTiles, Switch, Slider, Rating, Select, SearchList, ChipInput, KeyRecorder, IconPicker, ColorPicker, DatePicker, DateRangePicker, TimePicker, FileUpload, QrScanner, Composer

## References (read on demand)

- `references/functions.md` — the client functions a binding or press calls ({call, args}): validation, formatting, logic.
- `references/style.md` — the Box style keys, conditions and the token groups.
- `references/themes.md` — the built-in themes.

Apps add their own bindings (exp: and records: sources) and host functions: the `exponential-apps` skill.
