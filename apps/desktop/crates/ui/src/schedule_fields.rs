//! NEW-2: THE schedule picker's rows on the IDE — nobody speaks cron. The
//! twin of web `@exp/ui` `ScheduleFields` (`packages/ui/src/schedule-fields.tsx`),
//! iOS ExpUI `ScheduleFields.swift`, Android `ScheduleFields.kt`; the rules
//! and the copy are `fixtures/schedule-picker.json` (`domain::schedule_picker`).
//!
//! A schedule trigger STORES a 5-field cron; these rows edit it. They are
//! pushed into the trigger editor's glass group under its Schedule · On
//! event strip, top to bottom: Repeat · Days (weekly) | Day (monthly) ·
//! Every (minutes) | Minute (hourly) | Time · Time zone (EXP-1363: not for
//! minutes / hourly) · Cron (a custom, MCP-written expression only — the one
//! place cron is ever visible). Every option list scrolls.

use crate::controls::PointerMenuButtonExt as _;
use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Context, Div, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    ActiveTheme as _,
};

use domain::schedule_picker::{
    cron_from_schedule, default_schedule, every_label, schedule_from_cron, weekdays_label, Schedule,
    ScheduleRepeat, SCHEDULE_PICKER,
};

use crate::controls::glass_input;
use crate::picker::{Picker, PickerItem};
use crate::surface;
use crate::trigger_editor::{picker_trigger, picker_trigger_bare};

/// The Repeat menu's index of `custom` (after the four repeat values).
const CUSTOM_INDEX: u32 = ScheduleRepeat::ALL.len() as u32;

/// What the rows edit: the structured schedule, or — for a cron the rows
/// cannot express — `None` with the expression in `cron`; plus the zone.
pub(crate) struct ScheduleFieldsState {
    pub(crate) schedule: Option<Schedule>,
    /// The raw expression while `schedule` is `None` (custom).
    cron: Entity<InputState>,
    /// IANA zone, never blank (a read without one is `UTC`).
    pub(crate) tz: String,
}

/// The machine's IANA zone, else `UTC`.
fn machine_time_zone() -> String {
    iana_time_zone::get_timezone()
        .ok()
        .map(|zone| zone.trim().to_string())
        .filter(|zone| !zone.is_empty())
        .unwrap_or_else(|| "UTC".to_string())
}

/// Whitespace collapsed to single spaces, trimmed.
fn collapse(cron: &str) -> String {
    cron.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl ScheduleFieldsState {
    /// A fresh draft: the fixture's defaults (daily at 09:00) in the
    /// machine's zone.
    pub(crate) fn new<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Self {
        let placeholder = SCHEDULE_PICKER.copy.cron_placeholder.clone();
        let cron = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        // A custom cron's validity gates the host's Save: re-render on typing.
        cx.observe(&cron, |_, _, cx| cx.notify()).detach();
        Self {
            schedule: Some(default_schedule()),
            cron,
            tz: machine_time_zone(),
        }
    }

    /// Seed from a stored trigger's `cron` + `tz` (a blank zone reads `UTC`).
    pub(crate) fn seed(&mut self, cron: &str, tz: &str, window: &mut Window, cx: &mut App) {
        self.schedule = schedule_from_cron(cron);
        let collapsed = collapse(cron);
        self.cron.update(cx, |input, cx| input.set_value(collapsed, window, cx));
        self.tz = match tz.trim() {
            "" => "UTC".to_string(),
            tz => tz.to_string(),
        };
    }

    /// The wire `(cron, tz)`: the structured schedule's canonical cron, else
    /// the custom expression (whitespace collapsed); the zone always.
    pub(crate) fn wire(&self, cx: &App) -> (String, String) {
        let cron = match &self.schedule {
            Some(schedule) => cron_from_schedule(schedule),
            None => collapse(self.cron.read(cx).value().as_ref()),
        };
        let tz = match self.tz.trim() {
            "" => "UTC".to_string(),
            tz => tz.to_string(),
        };
        (cron, tz)
    }

    /// Inline errors; empty = savable. A weekly needs a day; a custom cron
    /// five fields.
    pub(crate) fn errors(&self, cx: &App) -> Vec<String> {
        let errors = &SCHEDULE_PICKER.copy.errors;
        match &self.schedule {
            Some(schedule) if schedule.repeat == ScheduleRepeat::Weekly && schedule.weekdays.is_empty() => {
                vec![errors.no_days.clone()]
            }
            Some(_) => Vec::new(),
            None if self.cron.read(cx).value().split_whitespace().count() == 5 => Vec::new(),
            None => vec![errors.cron.clone()],
        }
    }

    /// Pick a Repeat. While custom, this REPLACES the custom cron (from the
    /// defaults); weekly with no days seeds Monday–Friday, monthly from
    /// custom seeds day 1.
    pub(crate) fn pick_repeat(&mut self, repeat: ScheduleRepeat) {
        self.schedule = Some(with_repeat(self.schedule.as_ref(), repeat));
    }

    // -- render ---------------------------------------------------------------

    /// The rows, top to bottom, for the host's `surface::glass_group_rows`.
    /// `prefix` namespaces the element ids; `access` reaches the state on the
    /// host view.
    pub(crate) fn render_rows<V: Render>(
        &self,
        prefix: &'static str,
        access: impl Fn(&mut V) -> &mut Self + Copy + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Vec<Div> {
        let copy = &SCHEDULE_PICKER.copy;
        let mut rows = Vec::new();

        // Repeat: the four values, plus `Custom` ONLY while custom.
        let mut repeat_options: Vec<(SharedString, u32)> = ScheduleRepeat::ALL
            .iter()
            .enumerate()
            .map(|(ix, repeat)| (SharedString::from(repeat.label()), ix as u32))
            .collect();
        let current_repeat = match &self.schedule {
            Some(schedule) => ScheduleRepeat::ALL.iter().position(|r| *r == schedule.repeat).map(|ix| ix as u32),
            None => {
                repeat_options.push((copy.repeat.custom.clone().into(), CUSTOM_INDEX));
                Some(CUSTOM_INDEX)
            }
        };
        let repeat_label: SharedString = match &self.schedule {
            Some(schedule) => schedule.repeat.label().into(),
            None => copy.repeat.custom.clone().into(),
        };
        let repeat = menu_control(
            format!("{prefix}-repeat"),
            repeat_label,
            repeat_options,
            current_repeat,
            |state, ix| {
                if let Some(repeat) = ScheduleRepeat::ALL.get(ix as usize) {
                    state.pick_repeat(*repeat);
                }
            },
            access,
            true,
            cx,
        );
        rows.push(surface::glass_picker_row(copy.rows.repeat.clone(), None, repeat, cx));

        if let Some(schedule) = &self.schedule {
            match schedule.repeat {
                ScheduleRepeat::Weekly => rows.push(self.days_row(prefix, schedule, access, window, cx)),
                ScheduleRepeat::Monthly => {
                    let options = (1..=SCHEDULE_PICKER.day_of_month_max)
                        .map(|day| (day_label(day), day))
                        .collect();
                    let control = menu_control(
                        format!("{prefix}-day"),
                        day_label(schedule.day_of_month),
                        options,
                        Some(schedule.day_of_month),
                        |state, day| state.patch(|schedule| schedule.day_of_month = day),
                        access,
                        true,
                        cx,
                    );
                    rows.push(surface::glass_picker_row(copy.rows.day.clone(), None, control, cx));
                }
                ScheduleRepeat::Minutes => {
                    // EXP-1363: Every 5/10/15/20/30 minutes.
                    let options = SCHEDULE_PICKER
                        .every_values
                        .iter()
                        .map(|every| (SharedString::from(every_label(*every)), *every))
                        .collect();
                    let control = menu_control(
                        format!("{prefix}-every"),
                        every_label(schedule.every).into(),
                        options,
                        Some(schedule.every),
                        |state, every| state.patch(|schedule| schedule.every = every),
                        access,
                        true,
                        cx,
                    );
                    rows.push(surface::glass_picker_row(copy.rows.every.clone(), None, control, cx));
                }
                ScheduleRepeat::Hourly | ScheduleRepeat::Daily => {}
            }
            if schedule.repeat == ScheduleRepeat::Minutes {
                // No Minute / Time row: the interval is the whole schedule.
            } else if schedule.repeat == ScheduleRepeat::Hourly {
                let options = (0..60).map(|minute| (minute_of_hour_label(minute), minute)).collect();
                let control = menu_control(
                    format!("{prefix}-minute"),
                    minute_of_hour_label(schedule.minute),
                    options,
                    Some(schedule.minute),
                    |state, minute| state.patch(|schedule| schedule.minute = minute),
                    access,
                    true,
                    cx,
                );
                rows.push(surface::glass_picker_row(copy.rows.minute.clone(), None, control, cx));
            } else {
                rows.push(surface::glass_picker_row(
                    copy.rows.time.clone(),
                    None,
                    time_picker(prefix, schedule, access, cx),
                    cx,
                ));
            }
        }

        // EXP-1363: a minutes or hourly schedule shows no zone row (the
        // stored zone is kept as is).
        if self.schedule.as_ref().is_none_or(|schedule| schedule.repeat.zoned()) {
            rows.push(self.time_zone_row(prefix, access, window, cx));
        }

        if self.schedule.is_none() {
            let input = surface::glass_row_input(glass_input(&self.cron, window, cx));
            rows.push(surface::glass_input_row(
                copy.rows.cron.clone(),
                div()
                    .id(SharedString::from(format!("{prefix}-cron")))
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(input)
                    .into_any_element(),
                cx,
            ));
        }
        rows
    }

    /// Apply `edit` to the structured schedule (a no-op while custom).
    fn patch(&mut self, edit: impl FnOnce(&mut Schedule)) {
        if let Some(schedule) = self.schedule.as_mut() {
            edit(schedule);
        }
    }

    /// Days (weekly): the shared multi picker over the seven full names; the
    /// row reads [`weekdays_label`], never the picker's own summary.
    fn days_row<V: Render>(
        &self,
        prefix: &'static str,
        schedule: &Schedule,
        access: impl Fn(&mut V) -> &mut Self + Copy + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Div {
        let copy = &SCHEDULE_PICKER.copy;
        let label = weekdays_label(&schedule.weekdays);
        let empty = label.is_empty();
        let trigger = Button::new(SharedString::from(format!("{prefix}-days")))
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(if empty {
                cx.theme().muted_foreground
            } else {
                cx.theme().foreground.opacity(0.7)
            })
            .child(surface::picker_value_label(if empty { copy.errors.no_days.clone() } else { label }))
            .child(surface::picker_row_chevron(cx));
        let items: Vec<PickerItem<u32>> = copy
            .weekdays
            .iter()
            .enumerate()
            .map(|(ix, name)| PickerItem::new(ix as u32 + 1, name.clone()))
            .collect();
        let view = cx.entity().downgrade();
        let control = Picker::multi(
            items,
            schedule.weekdays.clone(),
            trigger.into_any_element(),
            Rc::new(move |mut next: Vec<u32>, _window, cx: &mut App| {
                next.sort_unstable();
                next.dedup();
                let _ = view.update(cx, |view, cx| {
                    access(view).patch(|schedule| schedule.weekdays = next);
                    cx.notify();
                });
            }),
        )
        .id(SharedString::from(format!("{prefix}-days-picker")))
        .render(window, cx);
        surface::glass_picker_row(copy.rows.days.clone(), None, control, cx)
    }

    /// Time zone: a searchable single picker over the contract's zones (a
    /// stored zone off the list first); `_` and `/` search as spaces.
    fn time_zone_row<V: Render>(
        &self,
        prefix: &'static str,
        access: impl Fn(&mut V) -> &mut Self + Copy + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Div {
        let copy = &SCHEDULE_PICKER.copy;
        let trigger = Button::new(SharedString::from(format!("{prefix}-time-zone")))
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(cx.theme().foreground.opacity(0.7))
            .child(surface::picker_value_label(self.tz.clone()))
            .child(surface::picker_row_chevron(cx));
        let view = cx.entity().downgrade();
        let control = Picker::single(
            time_zone_items(&self.tz),
            Some(self.tz.clone()),
            trigger.into_any_element(),
            Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                let Some(zone) = next.into_iter().next() else {
                    return;
                };
                let _ = view.update(cx, |view, cx| {
                    access(view).tz = zone;
                    cx.notify();
                });
            }),
        )
        .search(true)
        .placeholder(copy.time_zone_placeholder.clone())
        .id(SharedString::from(format!("{prefix}-time-zone-picker")))
        .render(window, cx);
        surface::glass_picker_row(copy.rows.time_zone.clone(), None, control, cx)
    }
}

/// The schedule a Repeat pick leaves (`current` `None` = custom, replaced
/// from the defaults): weekly with no days seeds Monday–Friday, monthly from
/// custom seeds day 1; every other field carries over.
fn with_repeat(current: Option<&Schedule>, repeat: ScheduleRepeat) -> Schedule {
    let seed = default_schedule();
    let base = current.cloned().unwrap_or_else(|| seed.clone());
    let weekdays = if repeat == ScheduleRepeat::Weekly && base.weekdays.is_empty() {
        seed.weekdays.clone()
    } else {
        base.weekdays.clone()
    };
    let day_of_month = if repeat == ScheduleRepeat::Monthly && current.is_none() {
        seed.day_of_month
    } else {
        base.day_of_month
    };
    // EXP-1363: a minutes read carries no time (00:00): leaving it seeds
    // the defaults' time.
    let (hour, minute) = if base.repeat == ScheduleRepeat::Minutes && repeat != ScheduleRepeat::Minutes {
        (seed.hour, seed.minute)
    } else {
        (base.hour, base.minute)
    };
    Schedule {
        repeat,
        weekdays,
        day_of_month,
        hour,
        minute,
        ..base
    }
}

/// `Day {day}`.
fn day_label(day: u32) -> SharedString {
    SCHEDULE_PICKER.copy.day_of_month.replacen("{day}", &day.to_string(), 1).into()
}

/// `:{mm}`.
fn minute_of_hour_label(minute: u32) -> SharedString {
    SCHEDULE_PICKER
        .copy
        .minute_of_hour
        .replacen("{minute}", &format!("{minute:02}"), 1)
        .into()
}

/// The zone rows: the contract's list, a stored zone off it first; each
/// searchable by its name with `_` and `/` read as spaces.
pub(crate) fn time_zone_items(current: &str) -> Vec<PickerItem<String>> {
    let item = |zone: &str| {
        PickerItem::new(zone.to_string(), zone.to_string()).keywords(vec![
            SharedString::from(zone.to_string()),
            SharedString::from(zone.replace(['_', '/'], " ")),
        ])
    };
    let names = domain::contract::TIME_ZONE_NAMES;
    let mut items = Vec::with_capacity(names.len() + 1);
    if !current.is_empty() && !names.contains(&current) {
        items.push(item(current));
    }
    items.extend(names.iter().map(|zone| item(zone)));
    items
}

/// A closed-vocabulary dropdown over `options` (label, value); a pick runs
/// `apply` on the host's state. `chevron` = the row's trailing chevron.
#[allow(clippy::too_many_arguments)]
fn menu_control<V: Render>(
    id: String,
    label: SharedString,
    options: Vec<(SharedString, u32)>,
    current: Option<u32>,
    apply: fn(&mut ScheduleFieldsState, u32),
    access: impl Fn(&mut V) -> &mut ScheduleFieldsState + Copy + 'static,
    chevron: bool,
    cx: &mut Context<V>,
) -> gpui::AnyElement {
    let view = cx.entity().downgrade();
    let trigger = if chevron {
        picker_trigger(id.into(), label, cx)
    } else {
        picker_trigger_bare(id.into(), label, cx)
    };
    trigger
        .pointer_menu(move |menu, _window, _cx| {
            let mut menu = crate::pickers::single_select_menu(menu.scrollable(true).max_h(px(320.)));
            for (text, value) in options.iter().cloned() {
                let view = view.clone();
                menu = menu.item(
                    crate::controls::pointer_label_item(text, false)
                        .checked(current == Some(value))
                        .on_click(move |_, _, cx| {
                            if let Some(view) = view.upgrade() {
                                view.update(cx, |view, cx| {
                                    apply(access(view), value);
                                    cx.notify();
                                });
                            }
                        }),
                );
            }
            menu
        })
        .into_any_element()
}

/// The Time row's trailing value ×4: the 24h `HH:mm`, its hour and its
/// minute each a picker (never the platform's locale-bound time control),
/// then the row's chevron.
fn time_picker<V: Render>(
    prefix: &'static str,
    schedule: &Schedule,
    access: impl Fn(&mut V) -> &mut ScheduleFieldsState + Copy + 'static,
    cx: &mut Context<V>,
) -> gpui::AnyElement {
    let two = |value: u32| SharedString::from(format!("{value:02}"));
    let hour = menu_control(
        format!("{prefix}-time-hour"),
        two(schedule.hour),
        (0..24).map(|hour| (two(hour), hour)).collect(),
        Some(schedule.hour),
        |state, hour| state.patch(|schedule| schedule.hour = hour),
        access,
        false,
        cx,
    );
    let minute = menu_control(
        format!("{prefix}-time-minute"),
        two(schedule.minute),
        (0..60).map(|minute| (two(minute), minute)).collect(),
        Some(schedule.minute),
        |state, minute| state.patch(|schedule| schedule.minute = minute),
        access,
        false,
        cx,
    );
    h_flex()
        .items_center()
        .child(hour)
        .child(":")
        .child(minute)
        .child(surface::picker_row_chevron(cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored zone off the contract list is offered first; a listed one
    /// is not duplicated; `_` and `/` search as spaces.
    #[test]
    fn time_zone_items_put_an_unlisted_zone_first() {
        let listed = time_zone_items("Europe/Vienna");
        assert_eq!(listed.len(), domain::contract::TIME_ZONE_NAMES.len());
        assert_eq!(listed[0].value, "UTC");
        let unlisted = time_zone_items("Mars/Olympus_Mons");
        assert_eq!(unlisted[0].value, "Mars/Olympus_Mons");
        assert_eq!(unlisted.len(), listed.len() + 1);
        let new_york = listed.iter().find(|item| item.value == "America/New_York").unwrap();
        assert!(new_york.keywords.iter().any(|keyword| keyword.as_ref() == "America New York"));
    }

    /// Picking a Repeat: weekly seeds Monday–Friday only when no day is set,
    /// monthly seeds day 1 only from custom, a custom cron is replaced from
    /// the defaults; the time carries over.
    #[test]
    fn a_repeat_pick_seeds_the_defaults() {
        let daily = schedule_from_cron("30 7 * * *").unwrap();
        let weekly = with_repeat(Some(&daily), ScheduleRepeat::Weekly);
        assert_eq!(cron_from_schedule(&weekly), "30 7 * * 1,2,3,4,5");
        let sundays = schedule_from_cron("30 7 * * 0").unwrap();
        assert_eq!(cron_from_schedule(&with_repeat(Some(&sundays), ScheduleRepeat::Weekly)), "30 7 * * 0");
        assert_eq!(cron_from_schedule(&with_repeat(Some(&daily), ScheduleRepeat::Monthly)), "30 7 1 * *");
        assert_eq!(cron_from_schedule(&with_repeat(Some(&daily), ScheduleRepeat::Hourly)), "30 * * * *");
        // From custom: the defaults (09:00), day 1.
        assert_eq!(cron_from_schedule(&with_repeat(None, ScheduleRepeat::Monthly)), "0 9 1 * *");
        assert_eq!(cron_from_schedule(&with_repeat(None, ScheduleRepeat::Weekly)), "0 9 * * 1,2,3,4,5");
        // EXP-1363: minutes keeps the default interval, leaving it seeds 09:00.
        assert_eq!(cron_from_schedule(&with_repeat(Some(&daily), ScheduleRepeat::Minutes)), "*/10 * * * *");
        let minutes = schedule_from_cron("*/15 * * * *").unwrap();
        assert_eq!(cron_from_schedule(&with_repeat(Some(&minutes), ScheduleRepeat::Daily)), "0 9 * * *");
    }
}
