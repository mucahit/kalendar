use chrono::{DateTime, Days, Duration, Local, NaiveDate, Timelike};
use kalendar_core::{Event, EventId, local_at};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventLayout {
    pub event_id: EventId,
    pub x_slot: usize,
    pub slot_count: usize,
    pub top: u16,
    pub height: u16,
}

#[must_use]
pub fn layout_day(
    events: &[&Event],
    date: NaiveDate,
    visible_start: u16,
    visible_end: u16,
    rows: u16,
) -> Vec<EventLayout> {
    if rows == 0 || visible_end <= visible_start {
        return Vec::new();
    }
    let day_start = local_at(date, 0, 0);
    let next_date = date
        .checked_add_days(Days::new(1))
        .expect("next day is representable");
    let day_end = local_at(next_date, 0, 0);
    let mut timed: Vec<_> = events
        .iter()
        .copied()
        .filter(|event| !event.all_day && effective_end(event) > day_start && event.start < day_end)
        .collect();
    timed.sort_by_key(|event| (event.start, event.end));
    let mut groups: Vec<Vec<&Event>> = Vec::new();
    let mut group_end: Option<DateTime<Local>> = None;
    for event in timed {
        if group_end.is_none_or(|end| event.start >= end) {
            groups.push(Vec::new());
            group_end = Some(effective_end(event));
        } else if group_end.is_some_and(|end| effective_end(event) > end) {
            group_end = Some(effective_end(event));
        }
        groups
            .last_mut()
            .expect("a group was created above")
            .push(event);
    }

    let visible_minutes = u32::from(visible_end - visible_start);
    let mut result = Vec::new();
    for group in groups {
        let mut slot_ends: Vec<DateTime<Local>> = Vec::new();
        let mut assigned = Vec::new();
        for event in &group {
            let slot = slot_ends
                .iter()
                .position(|end| *end <= event.start)
                .unwrap_or(slot_ends.len());
            if slot == slot_ends.len() {
                slot_ends.push(effective_end(event));
            } else {
                slot_ends[slot] = effective_end(event);
            }
            assigned.push((*event, slot));
        }
        let slot_count = slot_ends.len().max(1);
        for (event, slot) in assigned {
            let start = if event.start < day_start {
                0
            } else {
                minutes_into_day(event.start)
            }
            .max(u32::from(visible_start));
            let event_end = effective_end(event);
            let end = if event_end >= day_end {
                24 * 60
            } else {
                minutes_into_day(event_end)
            }
            .min(u32::from(visible_end));
            if end <= u32::from(visible_start) || start >= u32::from(visible_end) {
                continue;
            }
            let top =
                ((start - u32::from(visible_start)) * u32::from(rows) / visible_minutes) as u16;
            let bottom =
                ((end - u32::from(visible_start)) * u32::from(rows) / visible_minutes) as u16;
            result.push(EventLayout {
                event_id: event.id.clone(),
                x_slot: slot,
                slot_count,
                top: top.min(rows.saturating_sub(1)),
                height: bottom
                    .saturating_sub(top)
                    .max(1)
                    .min(rows.saturating_sub(top)),
            });
        }
    }
    result
}

fn minutes_into_day(value: DateTime<Local>) -> u32 {
    value.hour() * 60 + value.minute()
}

fn effective_end(event: &Event) -> DateTime<Local> {
    if event.end <= event.start {
        event.start + Duration::minutes(1)
    } else {
        event.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use kalendar_core::{Availability, CalendarId, Event, local_at};

    fn event(id: &str, date: NaiveDate, hour: u32, minute: u32, duration: i64) -> Event {
        let start = local_at(date, hour, minute);
        Event {
            id: EventId(id.into()),
            calendar_id: CalendarId("calendar".into()),
            title: id.into(),
            start,
            end: start + Duration::minutes(duration),
            all_day: false,
            location: None,
            notes: None,
            url: None,
            recurrence: None,
            availability: Availability::Busy,
        }
    }

    #[test]
    fn transitive_overlaps_share_group_width() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let events = [
            event("a", date, 9, 0, 60),
            event("b", date, 9, 30, 60),
            event("c", date, 10, 0, 60),
        ];
        let refs: Vec<_> = events.iter().collect();
        let layout = layout_day(&refs, date, 8 * 60, 20 * 60, 36);
        assert!(layout.iter().all(|item| item.slot_count == 2));
        assert_eq!(
            layout.iter().map(|item| item.x_slot).collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
    }

    #[test]
    fn short_event_occupies_at_least_one_row() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let event = event("short", date, 9, 1, 1);
        let layout = layout_day(&[&event], date, 8 * 60, 20 * 60, 12);
        assert_eq!(layout[0].height, 1);
    }

    #[test]
    fn clips_events_to_visible_hours() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let early = event("early", date, 6, 0, 180);
        let late = event("late", date, 19, 0, 180);
        let layout = layout_day(&[&early, &late], date, 8 * 60, 20 * 60, 24);
        assert_eq!(layout[0].top, 0);
        assert_eq!(layout[1].top, 22);
        assert_eq!(layout[1].height, 2);
    }

    #[test]
    fn renders_zero_duration_and_midnight_spanning_events() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let zero = event("zero", date, 9, 0, 0);
        let mut spanning = event("spanning", date, 23, 0, 180);
        spanning.start = local_at(date - Duration::days(1), 23, 0);
        spanning.end = local_at(date, 9, 0);
        let layout = layout_day(&[&zero, &spanning], date, 8 * 60, 20 * 60, 24);
        assert_eq!(layout.len(), 2);
        assert!(layout.iter().all(|item| item.height >= 1));
        assert!(
            layout
                .iter()
                .any(|item| item.event_id.0 == "spanning" && item.top == 0)
        );
    }
}
