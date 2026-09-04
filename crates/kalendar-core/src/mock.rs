use crate::{
    Availability, Calendar, CalendarBackend, CalendarId, CalendarSource, DateRange, DeleteScope,
    Event, EventId, EventPatch, NewEvent, Recurrence, RecurrenceScope, RgbColor, local_at,
    week_start,
};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{Duration, Local, NaiveDate};
use std::sync::RwLock;
use uuid::Uuid;

pub struct MockBackend {
    calendars: Vec<Calendar>,
    events: RwLock<Vec<Event>>,
}

impl MockBackend {
    #[must_use]
    pub fn demo(anchor: NaiveDate) -> Self {
        let work = Calendar {
            id: CalendarId("demo-work".into()),
            name: "Work".into(),
            color: Some(RgbColor {
                red: 74,
                green: 144,
                blue: 226,
            }),
            writable: true,
            source: CalendarSource::Local,
        };
        let personal = Calendar {
            id: CalendarId("demo-personal".into()),
            name: "Personal".into(),
            color: Some(RgbColor {
                red: 80,
                green: 200,
                blue: 120,
            }),
            writable: true,
            source: CalendarSource::Local,
        };
        let birthdays = Calendar {
            id: CalendarId("demo-birthdays".into()),
            name: "Birthdays".into(),
            color: Some(RgbColor {
                red: 236,
                green: 100,
                blue: 140,
            }),
            writable: false,
            source: CalendarSource::Birthdays,
        };
        let monday = week_start(anchor);
        let at = |day: i64, hour, minute| local_at(monday + Duration::days(day), hour, minute);
        let event = |id: &str,
                     calendar_id: &CalendarId,
                     title: &str,
                     day: i64,
                     hour: u32,
                     minute: u32,
                     duration: i64| Event {
            id: EventId(id.into()),
            calendar_id: calendar_id.clone(),
            title: title.into(),
            start: at(day, hour, minute),
            end: at(day, hour, minute) + Duration::minutes(duration),
            all_day: false,
            location: None,
            notes: None,
            url: None,
            recurrence: None,
            availability: Availability::Busy,
        };
        let mut events = vec![
            event("daily-1", &work.id, "Daily stand-up", 0, 9, 0, 30),
            event("daily-2", &work.id, "Daily stand-up", 2, 9, 0, 30),
            event("daily-3", &work.id, "Daily stand-up", 4, 9, 0, 30),
            event("design", &work.id, "Design review", 1, 10, 0, 90),
            event("one-to-one", &work.id, "1:1", 3, 10, 0, 60),
            event("overlap-a", &work.id, "Planning workshop", 3, 10, 30, 120),
            event("overlap-b", &work.id, "Customer call", 3, 11, 0, 60),
            event("lunch", &personal.id, "Lunch with Sam", 2, 13, 0, 60),
            event(
                "demo",
                &work.id,
                "Release demo with a deliberately long title",
                4,
                14,
                0,
                90,
            ),
            event("early", &personal.id, "Morning run", 1, 6, 30, 45),
            event("late", &personal.id, "Cinema", 5, 20, 30, 150),
        ];
        events[3].location = Some("Studio 2".into());
        events[3].notes = Some("Review the new calendar interaction prototypes.".into());
        events[3].url = Some("https://example.com/design-review".into());
        events[0].recurrence = Some(Recurrence {
            description: "Every weekday".into(),
        });
        events.push(Event {
            id: EventId("all-day-release".into()),
            calendar_id: work.id.clone(),
            title: "Release day".into(),
            start: at(4, 0, 0),
            end: at(5, 0, 0),
            all_day: true,
            location: None,
            notes: Some("v0.1 launch".into()),
            url: None,
            recurrence: None,
            availability: Availability::Busy,
        });
        events.push(Event {
            id: EventId("birthday".into()),
            calendar_id: birthdays.id.clone(),
            title: "Ada's birthday".into(),
            start: at(6, 0, 0),
            end: at(7, 0, 0),
            all_day: true,
            location: None,
            notes: None,
            url: None,
            recurrence: Some(Recurrence {
                description: "Every year".into(),
            }),
            availability: Availability::Free,
        });

        // Agenda remains populated after the demo week.
        for offset in [7_i64, 9, 12] {
            events.push(event(
                &format!("future-{offset}"),
                &personal.id,
                if offset == 7 { "Dentist" } else { "Coffee" },
                offset,
                11,
                30,
                60,
            ));
        }

        Self {
            calendars: vec![work, personal, birthdays],
            events: RwLock::new(events),
        }
    }

    #[must_use]
    pub fn today() -> Self {
        Self::demo(Local::now().date_naive())
    }
}

#[async_trait]
impl CalendarBackend for MockBackend {
    async fn calendars(&self) -> Result<Vec<Calendar>> {
        Ok(self.calendars.clone())
    }

    async fn events(&self, range: DateRange) -> Result<Vec<Event>> {
        let mut result: Vec<_> = self
            .events
            .read()
            .expect("mock backend lock poisoned")
            .iter()
            .filter(|event| event.end > range.from && event.start < range.to)
            .cloned()
            .collect();
        result.sort_by_key(|event| event.start);
        Ok(result)
    }

    async fn event(&self, id: &EventId) -> Result<Option<Event>> {
        Ok(self
            .events
            .read()
            .expect("mock backend lock poisoned")
            .iter()
            .find(|event| &event.id == id)
            .cloned())
    }

    async fn create_event(&self, event: NewEvent) -> Result<Event> {
        let created = Event {
            id: EventId(Uuid::new_v4().to_string()),
            calendar_id: event.calendar_id,
            title: event.title,
            start: event.start,
            end: event.end,
            all_day: event.all_day,
            location: event.location,
            notes: event.notes,
            url: None,
            recurrence: None,
            availability: Availability::Busy,
        };
        self.events
            .write()
            .expect("mock backend lock poisoned")
            .push(created.clone());
        Ok(created)
    }

    async fn update_event(&self, id: &EventId, patch: EventPatch) -> Result<Event> {
        let mut events = self.events.write().expect("mock backend lock poisoned");
        let event = events
            .iter_mut()
            .find(|event| &event.id == id)
            .ok_or_else(|| anyhow!("event not found"))?;
        if let Some(value) = patch.title {
            event.title = value;
        }
        if let Some(value) = patch.start {
            event.start = value;
        }
        if let Some(value) = patch.end {
            event.end = value;
        }
        if let Some(value) = patch.all_day {
            event.all_day = value;
        }
        if let Some(value) = patch.location {
            event.location = value;
        }
        if let Some(value) = patch.notes {
            event.notes = value;
        }
        Ok(event.clone())
    }

    async fn update_event_scoped(
        &self,
        id: &EventId,
        patch: EventPatch,
        _scope: RecurrenceScope,
    ) -> Result<Event> {
        self.update_event(id, patch).await
    }

    async fn delete_event(&self, id: &EventId, _scope: DeleteScope) -> Result<()> {
        let mut events = self.events.write().expect("mock backend lock poisoned");
        let before = events.len();
        events.retain(|event| &event.id != id);
        if events.len() == before {
            return Err(anyhow!("event not found"));
        }
        Ok(())
    }

    async fn search(&self, query: &str, range: Option<DateRange>) -> Result<Vec<Event>> {
        let query = query.to_lowercase();
        let mut result: Vec<_> = self
            .events
            .read()
            .expect("mock backend lock poisoned")
            .iter()
            .filter(|event| {
                range.is_none_or(|range| event.end > range.from && event.start < range.to)
            })
            .filter(|event| {
                event.title.to_lowercase().contains(&query)
                    || event
                        .location
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
                    || event
                        .notes
                        .as_deref()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect();
        result.sort_by_key(|event| event.start);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::range_for_week;

    #[tokio::test]
    async fn backend_contract_create_update_search_delete() {
        let anchor = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let backend = MockBackend::demo(anchor);
        let calendar = backend.calendars().await.unwrap().remove(0);
        let created = backend
            .create_event(NewEvent {
                calendar_id: calendar.id,
                title: "Contract test".into(),
                start: local_at(anchor, 16, 0),
                end: local_at(anchor, 17, 0),
                all_day: false,
                location: None,
                notes: None,
            })
            .await
            .unwrap();
        let updated = backend
            .update_event(
                &created.id,
                EventPatch {
                    title: Some("Updated contract".into()),
                    ..EventPatch::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.title, "Updated contract");
        assert!(
            backend
                .search("updated", Some(range_for_week(anchor)))
                .await
                .unwrap()
                .iter()
                .any(|event| event.id == created.id)
        );
        backend
            .delete_event(&created.id, DeleteScope::ThisEvent)
            .await
            .unwrap();
        assert!(backend.event(&created.id).await.unwrap().is_none());
    }
}
