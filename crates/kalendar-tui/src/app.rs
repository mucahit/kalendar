use crate::Config;
use anyhow::{Result, anyhow};
use chrono::{Datelike, Days, Duration, Local, NaiveDate, NaiveTime, Timelike, Weekday};
use kalendar_core::{
    Calendar, CalendarBackend, CalendarId, DateRange, DeleteScope, Event, EventId, EventPatch,
    NewEvent, PermissionStatus, RecurrenceScope, local_at,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum View {
    Agenda,
    Week,
    Month,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Normal,
    EventDetail,
    EventEditor,
    CalendarPicker,
    Search,
    ConfirmDelete,
    ConfirmUpdate,
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorField {
    Title,
    Calendar,
    Date,
    Start,
    End,
    AllDay,
    Location,
    Notes,
}

impl EditorField {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::Calendar,
            Self::Calendar => Self::Date,
            Self::Date => Self::Start,
            Self::Start => Self::End,
            Self::End => Self::AllDay,
            Self::AllDay => Self::Location,
            Self::Location => Self::Notes,
            Self::Notes => Self::Title,
        }
    }

    #[must_use]
    pub fn previous(self) -> Self {
        match self {
            Self::Title => Self::Notes,
            Self::Calendar => Self::Title,
            Self::Date => Self::Calendar,
            Self::Start => Self::Date,
            Self::End => Self::Start,
            Self::AllDay => Self::End,
            Self::Location => Self::AllDay,
            Self::Notes => Self::Location,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventEditor {
    pub editing: Option<EventId>,
    pub field: EditorField,
    pub title: String,
    pub calendar_id: CalendarId,
    pub date: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: String,
    pub notes: String,
    all_day_duration_days: u64,
}

impl EventEditor {
    fn new(app: &App) -> Result<Self> {
        let calendar_id = app
            .last_used_calendar
            .clone()
            .or_else(|| {
                app.calendars
                    .iter()
                    .find(|calendar| calendar.writable)
                    .map(|calendar| calendar.id.clone())
            })
            .ok_or_else(|| anyhow!("No writable calendar is available."))?;
        let cursor_minutes = if app.view == View::Month {
            parse_time_minutes(&app.config.default_event_start).unwrap_or(app.cursor_minutes)
        } else {
            app.cursor_minutes
        };
        let (hour, minute) = (
            u32::from(cursor_minutes / 60),
            u32::from(cursor_minutes % 60),
        );
        let start = NaiveTime::from_hms_opt(hour, minute, 0)
            .unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let end = start + Duration::minutes(app.config.default_event_duration_minutes.max(1));
        Ok(Self {
            editing: None,
            field: EditorField::Title,
            title: String::new(),
            calendar_id,
            date: app.cursor_date.format("%Y-%m-%d").to_string(),
            start: start.format("%H:%M").to_string(),
            end: end.format("%H:%M").to_string(),
            all_day: false,
            location: String::new(),
            notes: String::new(),
            all_day_duration_days: 1,
        })
    }

    fn from_event(event: &Event) -> Self {
        let all_day_duration_days = u64::try_from(
            (event.end.date_naive() - event.start.date_naive())
                .num_days()
                .max(1),
        )
        .unwrap_or(1);
        Self {
            editing: Some(event.id.clone()),
            field: EditorField::Title,
            title: event.title.clone(),
            calendar_id: event.calendar_id.clone(),
            date: event.start.format("%Y-%m-%d").to_string(),
            start: event.start.format("%H:%M").to_string(),
            end: event.end.format("%H:%M").to_string(),
            all_day: event.all_day,
            location: event.location.clone().unwrap_or_default(),
            notes: event.notes.clone().unwrap_or_default(),
            all_day_duration_days,
        }
    }

    pub fn insert(&mut self, character: char) {
        match self.field {
            EditorField::Title => self.title.push(character),
            EditorField::Date => self.date.push(character),
            EditorField::Start => self.start.push(character),
            EditorField::End => self.end.push(character),
            EditorField::Location => self.location.push(character),
            EditorField::Notes => self.notes.push(character),
            EditorField::Calendar | EditorField::AllDay => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.field {
            EditorField::Title => {
                self.title.pop();
            }
            EditorField::Date => {
                self.date.pop();
            }
            EditorField::Start => {
                self.start.pop();
            }
            EditorField::End => {
                self.end.pop();
            }
            EditorField::Location => {
                self.location.pop();
            }
            EditorField::Notes => {
                self.notes.pop();
            }
            EditorField::Calendar | EditorField::AllDay => {}
        }
    }

    fn values(&self) -> Result<(chrono::DateTime<Local>, chrono::DateTime<Local>)> {
        if self.title.trim().is_empty() {
            return Err(anyhow!("Title cannot be empty."));
        }
        let date = NaiveDate::parse_from_str(&self.date, "%Y-%m-%d")
            .map_err(|_| anyhow!("Date must use YYYY-MM-DD."))?;
        if self.all_day {
            let next = date
                .checked_add_days(Days::new(self.all_day_duration_days.max(1)))
                .ok_or_else(|| anyhow!("Date is out of range."))?;
            return Ok((local_at(date, 0, 0), local_at(next, 0, 0)));
        }
        let start = NaiveTime::parse_from_str(&self.start, "%H:%M")
            .map_err(|_| anyhow!("Start must use HH:MM."))?;
        let end = NaiveTime::parse_from_str(&self.end, "%H:%M")
            .map_err(|_| anyhow!("End must use HH:MM."))?;
        if end == start {
            return Err(anyhow!("End must differ from start."));
        }
        let end_date = if end < start {
            date.checked_add_days(Days::new(1))
                .ok_or_else(|| anyhow!("Date is out of range."))?
        } else {
            date
        };
        let start = local_at(date, start.hour(), start.minute());
        let end = local_at(end_date, end.hour(), end.minute());
        Ok((start, end))
    }
}

#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<Event>,
    pub selected: usize,
}

pub struct App {
    pub backend: Arc<dyn CalendarBackend>,
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub view: View,
    pub mode: Mode,
    pub cursor_date: NaiveDate,
    pub cursor_minutes: u16,
    pub selected_event: Option<EventId>,
    pub calendars: Vec<Calendar>,
    pub visible_calendars: HashSet<CalendarId>,
    pub events: Vec<Event>,
    pub error: Option<String>,
    pub status: Option<String>,
    pub editor: Option<EventEditor>,
    pub search: SearchState,
    pub calendar_picker_index: usize,
    pub should_quit: bool,
    pub last_used_calendar: Option<CalendarId>,
    cache: HashMap<(i64, i64), Vec<Event>>,
    picker_original: Option<HashSet<CalendarId>>,
    preferred_month_day: u32,
    confirm_return_mode: Mode,
}

impl App {
    #[must_use]
    pub fn new(
        backend: Arc<dyn CalendarBackend>,
        config: Config,
        config_path: Option<PathBuf>,
        initial_date: NaiveDate,
        initial_view: Option<View>,
    ) -> Self {
        let view = initial_view.unwrap_or(match config.default_view.as_str() {
            "agenda" => View::Agenda,
            "month" => View::Month,
            _ => View::Week,
        });
        let (day_start, _) = config.day_minutes();
        let cursor_date =
            if !config.show_weekends && view != View::Agenda && is_weekend(initial_date) {
                previous_weekday(initial_date)
            } else {
                initial_date
            };
        Self {
            backend,
            config,
            config_path,
            view,
            mode: Mode::Normal,
            cursor_date,
            cursor_minutes: day_start.saturating_add(60),
            selected_event: None,
            calendars: Vec::new(),
            visible_calendars: HashSet::new(),
            events: Vec::new(),
            error: None,
            status: None,
            editor: None,
            search: SearchState::default(),
            calendar_picker_index: 0,
            should_quit: false,
            last_used_calendar: None,
            cache: HashMap::new(),
            picker_original: None,
            preferred_month_day: cursor_date.day(),
            confirm_return_mode: Mode::Normal,
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        match self.backend.permissions().await? {
            PermissionStatus::Granted => {}
            PermissionStatus::NotDetermined => {
                if !self.backend.request_permissions().await? {
                    return Err(anyhow!(
                        "Calendar access was not granted. Open System Settings → Privacy & Security → Calendars."
                    ));
                }
            }
            PermissionStatus::Denied => {
                return Err(anyhow!(
                    "Calendar permission denied. Open System Settings → Privacy & Security → Calendars."
                ));
            }
        }
        self.calendars = self.backend.calendars().await?;
        self.visible_calendars = self
            .calendars
            .iter()
            .filter(|calendar| {
                !self
                    .config
                    .hidden_calendars
                    .iter()
                    .any(|hidden| hidden == &calendar.name || hidden == &calendar.id.0)
            })
            .map(|calendar| calendar.id.clone())
            .collect();
        self.last_used_calendar = self
            .calendars
            .iter()
            .find(|calendar| calendar.writable)
            .map(|calendar| calendar.id.clone());
        self.refresh(false).await
    }

    #[must_use]
    pub fn requested_range(&self) -> DateRange {
        match self.view {
            View::Week => {
                let start_date = self.week_start();
                let end_date = start_date
                    .checked_add_days(Days::new(7))
                    .expect("week end is representable");
                let padded_start = start_date
                    .checked_sub_days(Days::new(7))
                    .expect("padded week start is representable");
                let padded_end = end_date
                    .checked_add_days(Days::new(7))
                    .expect("padded week end is representable");
                DateRange::new(local_at(padded_start, 0, 0), local_at(padded_end, 0, 0))
            }
            View::Month => {
                let start_date = self.month_grid_start();
                let end_date = start_date
                    .checked_add_days(Days::new(42))
                    .expect("month grid end is representable");
                DateRange::new(local_at(start_date, 0, 0), local_at(end_date, 0, 0))
            }
            View::Agenda => {
                let from = local_at(self.cursor_date, 0, 0);
                let to_date = self
                    .cursor_date
                    .checked_add_days(Days::new(15))
                    .expect("agenda date remains in range");
                DateRange::new(from, local_at(to_date, 0, 0))
            }
        }
    }

    #[must_use]
    fn visible_range(&self) -> DateRange {
        if self.view != View::Week {
            return self.requested_range();
        }
        let start = self.week_start();
        let end = start
            .checked_add_days(Days::new(7))
            .expect("week end is representable");
        DateRange::new(local_at(start, 0, 0), local_at(end, 0, 0))
    }

    #[must_use]
    pub fn week_start(&self) -> NaiveDate {
        let days = if self.config.week_starts_on.eq_ignore_ascii_case("sunday") {
            i64::from(self.cursor_date.weekday().num_days_from_sunday())
        } else {
            i64::from(self.cursor_date.weekday().num_days_from_monday())
        };
        self.cursor_date - Duration::days(days)
    }

    #[must_use]
    pub fn month_grid_start(&self) -> NaiveDate {
        let first = self
            .cursor_date
            .with_day(1)
            .expect("every month has a first day");
        let days = if self.config.week_starts_on.eq_ignore_ascii_case("sunday") {
            i64::from(first.weekday().num_days_from_sunday())
        } else {
            i64::from(first.weekday().num_days_from_monday())
        };
        first - Duration::days(days)
    }

    pub async fn refresh(&mut self, force: bool) -> Result<()> {
        let range = self.requested_range();
        let visible_range = self.visible_range();
        let key = (range.from.timestamp(), range.to.timestamp());
        if !force && let Some(events) = self.cache.get(&key) {
            self.events.clone_from(events);
        } else if !force
            && let Some(events) = self
                .cache
                .iter()
                .find(|((from, to), _)| *from <= key.0 && *to >= key.1)
                .map(|(_, events)| events)
        {
            self.events = events
                .iter()
                .filter(|event| event.end > range.from && event.start < range.to)
                .cloned()
                .collect();
        } else if !force
            && let Some(events) = self
                .cache
                .iter()
                .find(|((from, to), _)| {
                    *from <= visible_range.from.timestamp() && *to >= visible_range.to.timestamp()
                })
                .map(|(_, events)| events)
        {
            self.events = events
                .iter()
                .filter(|event| event.end > visible_range.from && event.start < visible_range.to)
                .cloned()
                .collect();
        } else {
            let events = self.backend.events(range).await?;
            if self.cache.len() >= 24 {
                self.cache.clear();
            }
            self.cache.insert(key, events.clone());
            self.events = events;
        }
        self.ensure_selection();
        Ok(())
    }

    pub async fn set_view(&mut self, view: View) -> Result<()> {
        self.view = view;
        self.mode = Mode::Normal;
        self.refresh(false).await
    }

    pub async fn move_cursor_days(&mut self, days: i64) -> Result<()> {
        let mut target = self
            .cursor_date
            .checked_add_signed(Duration::days(days))
            .ok_or_else(|| anyhow!("Date is out of range."))?;
        if !self.config.show_weekends && self.view != View::Agenda && days != 0 {
            let step = days.signum();
            while is_weekend(target) {
                target = target
                    .checked_add_signed(Duration::days(step))
                    .ok_or_else(|| anyhow!("Date is out of range."))?;
            }
        }
        self.cursor_date = target;
        self.preferred_month_day = self.cursor_date.day();
        self.selected_event = None;
        self.refresh(false).await
    }

    pub async fn move_month(&mut self, months: i32) -> Result<()> {
        let month_index = self.cursor_date.year() * 12
            + i32::try_from(self.cursor_date.month0()).unwrap_or(0)
            + months;
        let year = month_index.div_euclid(12);
        let month = u32::try_from(month_index.rem_euclid(12) + 1).unwrap_or(1);
        let last_day = last_day_of_month(year, month);
        self.cursor_date =
            NaiveDate::from_ymd_opt(year, month, self.preferred_month_day.min(last_day))
                .ok_or_else(|| anyhow!("Date is out of range."))?;
        if !self.config.show_weekends && self.view != View::Agenda && is_weekend(self.cursor_date) {
            self.cursor_date = previous_weekday(self.cursor_date);
        }
        self.selected_event = None;
        self.refresh(false).await
    }

    pub async fn today(&mut self) -> Result<()> {
        self.cursor_date = Local::now().date_naive();
        if !self.config.show_weekends && self.view != View::Agenda && is_weekend(self.cursor_date) {
            self.view = View::Agenda;
        }
        self.preferred_month_day = self.cursor_date.day();
        let now = Local::now();
        self.cursor_minutes = u16::try_from(now.hour() * 60 + now.minute()).unwrap_or(9 * 60);
        self.selected_event = None;
        self.refresh(false).await
    }

    pub fn move_time(&mut self, minutes: i16) {
        let (start, end) = self.config.day_minutes();
        self.cursor_minutes = self
            .cursor_minutes
            .saturating_add_signed(minutes)
            .clamp(start, end.saturating_sub(1));
        self.selected_event = None;
        self.ensure_selection();
    }

    pub fn scroll_hours(&mut self, hours: i16) {
        let (start, end) = self.config.day_minutes();
        let span = end - start;
        let max_start = 24 * 60 - span;
        let new_start = start
            .saturating_add_signed(hours.saturating_mul(60))
            .min(max_start);
        let new_end = new_start + span;
        self.config.day_start = format!("{:02}:{:02}", new_start / 60, new_start % 60);
        self.config.day_end = format!("{:02}:{:02}", new_end / 60, new_end % 60);
        self.cursor_minutes = self
            .cursor_minutes
            .clamp(new_start, new_end.saturating_sub(1));
    }

    pub fn visible_events(&self) -> impl Iterator<Item = &Event> {
        self.events
            .iter()
            .filter(|event| self.visible_calendars.contains(&event.calendar_id))
    }

    pub fn selected(&self) -> Option<&Event> {
        self.selected_event
            .as_ref()
            .and_then(|id| self.events.iter().find(|event| &event.id == id))
    }

    pub fn ensure_selection(&mut self) {
        if self.selected_event.as_ref().is_some_and(|id| {
            self.events
                .iter()
                .any(|event| &event.id == id && self.visible_calendars.contains(&event.calendar_id))
        }) {
            return;
        }
        let cursor = i64::from(self.cursor_minutes);
        self.selected_event = self
            .visible_events()
            .filter(|event| event_occurs_on(event, self.cursor_date))
            .min_by_key(|event| {
                if event.all_day {
                    i64::MAX / 2
                } else {
                    (i64::from(event.start.hour() * 60 + event.start.minute()) - cursor).abs()
                }
            })
            .map(|event| event.id.clone());
    }

    pub fn cycle_event(&mut self, delta: i32) {
        let mut ids: Vec<_> = self
            .visible_events()
            .filter(|event| match self.view {
                View::Agenda => true,
                View::Week | View::Month => event_occurs_on(event, self.cursor_date),
            })
            .map(|event| event.id.clone())
            .collect();
        ids.sort_by_key(|id| {
            self.events
                .iter()
                .find(|event| &event.id == id)
                .map(|event| event.start)
        });
        if ids.is_empty() {
            self.selected_event = None;
            return;
        }
        let current = self
            .selected_event
            .as_ref()
            .and_then(|id| ids.iter().position(|candidate| candidate == id))
            .unwrap_or(0);
        let next = (i32::try_from(current).unwrap_or(0) + delta)
            .rem_euclid(i32::try_from(ids.len()).unwrap_or(1)) as usize;
        self.selected_event = Some(ids[next].clone());
        if self.view != View::Agenda
            && let Some((date, minutes)) = self.selected().map(|event| {
                (
                    event.start.date_naive(),
                    (!event.all_day).then(|| event.start.hour() * 60 + event.start.minute()),
                )
            })
        {
            self.cursor_date = date;
            self.preferred_month_day = self.cursor_date.day();
            if let Some(minutes) = minutes {
                self.cursor_minutes = u16::try_from(minutes).unwrap_or(self.cursor_minutes);
            }
        }
    }

    pub fn open_selected(&mut self) {
        self.ensure_selection();
        if self.selected_event.is_some() {
            self.mode = Mode::EventDetail;
        }
    }

    pub fn start_new_event(&mut self) {
        match EventEditor::new(self) {
            Ok(editor) => {
                self.editor = Some(editor);
                self.mode = Mode::EventEditor;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn start_edit_event(&mut self) {
        if let Some(event) = self.selected().cloned() {
            if self
                .calendar(&event.calendar_id)
                .is_some_and(|calendar| !calendar.writable)
            {
                self.error = Some("The selected calendar is read-only.".into());
                return;
            }
            self.editor = Some(EventEditor::from_event(&event));
            self.mode = Mode::EventEditor;
        }
    }

    pub fn start_delete_confirmation(&mut self) {
        let Some(event) = self.selected() else {
            return;
        };
        if self
            .calendar(&event.calendar_id)
            .is_some_and(|calendar| !calendar.writable)
        {
            self.error = Some("The selected calendar is read-only.".into());
            return;
        }
        self.confirm_return_mode = self.mode;
        self.mode = Mode::ConfirmDelete;
    }

    pub fn cancel_delete_confirmation(&mut self) {
        self.mode = self.confirm_return_mode;
    }

    pub fn cycle_editor_calendar(&mut self, direction: i32) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        if editor.editing.is_some() {
            return;
        }
        let writable: Vec<_> = self
            .calendars
            .iter()
            .filter(|calendar| calendar.writable)
            .map(|calendar| calendar.id.clone())
            .collect();
        if writable.is_empty() {
            return;
        }
        let current = writable
            .iter()
            .position(|id| id == &editor.calendar_id)
            .unwrap_or(0);
        let next = (i32::try_from(current).unwrap_or(0) + direction)
            .rem_euclid(i32::try_from(writable.len()).unwrap_or(1)) as usize;
        if let Some(editor) = self.editor.as_mut() {
            editor.calendar_id = writable[next].clone();
        }
    }

    #[must_use]
    pub fn editor_is_recurring(&self) -> bool {
        self.editor
            .as_ref()
            .and_then(|editor| editor.editing.as_ref())
            .and_then(|id| self.events.iter().find(|event| &event.id == id))
            .is_some_and(|event| event.recurrence.is_some())
    }

    pub async fn save_editor(&mut self, scope: RecurrenceScope) -> Result<()> {
        let editor = self
            .editor
            .clone()
            .ok_or_else(|| anyhow!("No event is being edited."))?;
        let (start, end) = editor.values()?;
        let location =
            (!editor.location.trim().is_empty()).then(|| editor.location.trim().to_owned());
        let notes = (!editor.notes.trim().is_empty()).then(|| editor.notes.trim().to_owned());
        let event = if let Some(id) = &editor.editing {
            self.backend
                .update_event_scoped(
                    id,
                    EventPatch {
                        title: Some(editor.title.trim().to_owned()),
                        start: Some(start),
                        end: Some(end),
                        all_day: Some(editor.all_day),
                        location: Some(location),
                        notes: Some(notes),
                    },
                    scope,
                )
                .await?
        } else {
            self.backend
                .create_event(NewEvent {
                    calendar_id: editor.calendar_id.clone(),
                    title: editor.title.trim().to_owned(),
                    start,
                    end,
                    all_day: editor.all_day,
                    location,
                    notes,
                })
                .await?
        };
        self.last_used_calendar = Some(event.calendar_id.clone());
        self.cursor_date = event.start.date_naive();
        self.preferred_month_day = self.cursor_date.day();
        self.selected_event = Some(event.id);
        self.editor = None;
        self.mode = Mode::Normal;
        self.cache.clear();
        self.refresh(true).await?;
        self.status = Some(
            if editor.editing.is_some() {
                "Event updated"
            } else {
                "Event created"
            }
            .into(),
        );
        Ok(())
    }

    pub async fn delete_selected(&mut self, scope: DeleteScope) -> Result<()> {
        let id = self
            .selected_event
            .clone()
            .ok_or_else(|| anyhow!("No event is selected."))?;
        self.backend.delete_event(&id, scope).await?;
        self.selected_event = None;
        self.mode = Mode::Normal;
        self.cache.clear();
        self.refresh(true).await?;
        self.status = Some("Event deleted".into());
        Ok(())
    }

    pub fn open_calendar_picker(&mut self) {
        self.calendar_picker_index = self
            .calendar_picker_index
            .min(self.calendars.len().saturating_sub(1));
        self.picker_original = Some(self.visible_calendars.clone());
        self.mode = Mode::CalendarPicker;
    }

    pub fn cancel_calendar_picker(&mut self) {
        if let Some(original) = self.picker_original.take() {
            self.visible_calendars = original;
        }
        self.mode = Mode::Normal;
        self.ensure_selection();
    }

    pub fn toggle_calendar(&mut self) {
        if let Some(calendar) = self.calendars.get(self.calendar_picker_index)
            && !self.visible_calendars.remove(&calendar.id)
        {
            self.visible_calendars.insert(calendar.id.clone());
        }
    }

    pub fn apply_calendar_visibility(&mut self) -> Result<()> {
        self.config.hidden_calendars = self
            .calendars
            .iter()
            .filter(|calendar| !self.visible_calendars.contains(&calendar.id))
            .map(|calendar| calendar.id.0.clone())
            .collect();
        self.config.save(self.config_path.as_deref())?;
        self.picker_original = None;
        self.mode = Mode::Normal;
        self.ensure_selection();
        self.status = Some("Calendar visibility saved".into());
        Ok(())
    }

    pub async fn update_search(&mut self) -> Result<()> {
        if self.search.query.trim().is_empty() {
            self.search.results.clear();
        } else {
            self.search.results = self
                .backend
                .search(self.search.query.trim(), None)
                .await?
                .into_iter()
                .filter(|event| self.visible_calendars.contains(&event.calendar_id))
                .collect();
        }
        self.search.selected = self
            .search
            .selected
            .min(self.search.results.len().saturating_sub(1));
        Ok(())
    }

    pub async fn accept_search(&mut self) -> Result<()> {
        let Some(event) = self.search.results.get(self.search.selected).cloned() else {
            return Ok(());
        };
        self.cursor_date = event.start.date_naive();
        self.preferred_month_day = self.cursor_date.day();
        self.cursor_minutes =
            u16::try_from(event.start.hour() * 60 + event.start.minute()).unwrap_or(9 * 60);
        self.selected_event = Some(event.id.clone());
        self.view = if !self.config.show_weekends && is_weekend(self.cursor_date) {
            View::Agenda
        } else {
            View::Week
        };
        self.mode = Mode::Normal;
        self.refresh(false).await?;
        self.selected_event = Some(event.id);
        Ok(())
    }

    #[must_use]
    pub fn calendar(&self, id: &CalendarId) -> Option<&Calendar> {
        self.calendars.iter().find(|calendar| &calendar.id == id)
    }

    pub fn set_error(&mut self, error: impl std::fmt::Display) {
        tracing::error!(error = %error, "application operation failed");
        self.error = Some(error.to_string());
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid next month")
        .pred_opt()
        .expect("month has a previous day")
        .day()
}

fn parse_time_minutes(value: &str) -> Option<u16> {
    let time = NaiveTime::parse_from_str(value, "%H:%M").ok()?;
    u16::try_from(time.hour() * 60 + time.minute()).ok()
}

fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

fn previous_weekday(date: NaiveDate) -> NaiveDate {
    match date.weekday() {
        Weekday::Sat => date - Duration::days(1),
        Weekday::Sun => date - Duration::days(2),
        _ => date,
    }
}

pub(crate) fn event_occurs_on(event: &Event, date: NaiveDate) -> bool {
    if !event.all_day {
        return event.start.date_naive() == date;
    }
    let start = event.start.date_naive();
    let end = if event.end.date_naive() > start {
        event.end.date_naive()
    } else {
        start
            .checked_add_days(Days::new(1))
            .expect("the day after an event is representable")
    };
    start <= date && date < end
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalendar_core::MockBackend;

    #[tokio::test]
    async fn app_create_edit_search_and_delete_flow() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let backend = Arc::new(MockBackend::demo(date));
        let mut app = App::new(
            backend.clone(),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();

        app.start_new_event();
        app.editor.as_mut().unwrap().title = "A brand new event".into();
        app.save_editor(RecurrenceScope::ThisEvent).await.unwrap();
        let created = app.selected_event.clone().unwrap();
        assert_eq!(
            backend.event(&created).await.unwrap().unwrap().title,
            "A brand new event"
        );

        app.start_edit_event();
        app.editor.as_mut().unwrap().title = "A renamed event".into();
        app.save_editor(RecurrenceScope::ThisEvent).await.unwrap();
        assert_eq!(
            backend.event(&created).await.unwrap().unwrap().title,
            "A renamed event"
        );

        app.search.query = "renamed".into();
        app.update_search().await.unwrap();
        assert_eq!(app.search.results.len(), 1);

        app.delete_selected(DeleteScope::ThisEvent).await.unwrap();
        assert!(backend.event(&created).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn calendar_picker_cancel_restores_visibility() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        let before = app.visible_calendars.clone();
        app.open_calendar_picker();
        app.toggle_calendar();
        assert_ne!(app.visible_calendars, before);
        app.cancel_calendar_picker();
        assert_eq!(app.visible_calendars, before);
    }

    #[tokio::test]
    async fn recurring_edit_accepts_future_scope() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let backend = Arc::new(MockBackend::demo(date));
        let mut app = App::new(
            backend.clone(),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        app.selected_event = Some(EventId("daily-1".into()));
        app.start_edit_event();
        assert!(app.editor_is_recurring());
        app.editor.as_mut().unwrap().title = "Updated recurring event".into();
        app.save_editor(RecurrenceScope::ThisAndFuture)
            .await
            .unwrap();
        assert_eq!(
            backend
                .event(&EventId("daily-1".into()))
                .await
                .unwrap()
                .unwrap()
                .title,
            "Updated recurring event"
        );
    }

    #[tokio::test]
    async fn delete_confirmation_returns_to_its_origin_and_blocks_read_only_events() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();

        app.selected_event = Some(EventId("design".into()));
        app.mode = Mode::Normal;
        app.start_delete_confirmation();
        assert_eq!(app.mode, Mode::ConfirmDelete);
        app.cancel_delete_confirmation();
        assert_eq!(app.mode, Mode::Normal);

        app.mode = Mode::EventDetail;
        app.start_delete_confirmation();
        app.cancel_delete_confirmation();
        assert_eq!(app.mode, Mode::EventDetail);

        app.selected_event = Some(EventId("birthday".into()));
        app.start_delete_confirmation();
        assert_eq!(app.mode, Mode::EventDetail);
        assert_eq!(
            app.error.as_deref(),
            Some("The selected calendar is read-only.")
        );
    }

    #[test]
    fn sunday_week_start_is_honored() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let config = Config {
            week_starts_on: "sunday".into(),
            ..Config::default()
        };
        let app = App::new(
            Arc::new(MockBackend::demo(date)),
            config,
            None,
            date,
            Some(View::Week),
        );
        assert_eq!(
            app.week_start(),
            NaiveDate::from_ymd_opt(2026, 8, 30).unwrap()
        );
    }

    #[test]
    fn editor_interprets_an_earlier_end_time_as_next_day() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.calendars = vec![Calendar {
            id: CalendarId("write".into()),
            name: "Writable".into(),
            color: None,
            writable: true,
            source: kalendar_core::CalendarSource::Local,
        }];
        app.start_new_event();
        let editor = app.editor.as_mut().unwrap();
        editor.title = "Overnight".into();
        editor.start = "23:30".into();
        editor.end = "00:30".into();
        let (start, end) = editor.values().unwrap();
        assert_eq!((end - start).num_minutes(), 60);
        assert_ne!(start.date_naive(), end.date_naive());
    }

    #[tokio::test]
    async fn editor_preserves_multi_day_all_day_duration() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut event = MockBackend::demo(date)
            .events(DateRange::new(
                local_at(date - Duration::days(14), 0, 0),
                local_at(date + Duration::days(14), 0, 0),
            ))
            .await
            .unwrap()
            .into_iter()
            .find(|event| event.all_day)
            .unwrap();
        event.start = local_at(date, 0, 0);
        event.end = local_at(date + Duration::days(3), 0, 0);
        let editor = EventEditor::from_event(&event);
        let (start, end) = editor.values().unwrap();
        assert_eq!((end.date_naive() - start.date_naive()).num_days(), 3);
    }

    #[tokio::test]
    async fn cached_superset_serves_a_smaller_view_range() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Month),
        );
        app.initialize().await.unwrap();
        assert_eq!(app.cache.len(), 1);
        app.set_view(View::Agenda).await.unwrap();
        assert_eq!(app.cache.len(), 1);
        assert!(
            app.events
                .iter()
                .all(|event| event.start.date_naive() >= date)
        );
    }

    #[tokio::test]
    async fn padded_week_cache_serves_the_adjacent_week() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        assert_eq!(app.cache.len(), 1);
        app.move_cursor_days(7).await.unwrap();
        assert_eq!(app.cache.len(), 1);
    }

    #[tokio::test]
    async fn month_navigation_clamps_at_short_month_boundary() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Month),
        );
        app.initialize().await.unwrap();
        app.move_month(1).await.unwrap();
        assert_eq!(
            app.cursor_date,
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
        app.move_month(-1).await.unwrap();
        assert_eq!(
            app.cursor_date,
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()
        );
    }

    #[tokio::test]
    async fn hidden_weekends_are_skipped_during_grid_navigation() {
        let friday = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let config = Config {
            show_weekends: false,
            ..Config::default()
        };
        let mut app = App::new(
            Arc::new(MockBackend::demo(friday)),
            config,
            None,
            friday,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        app.move_cursor_days(1).await.unwrap();
        assert_eq!(
            app.cursor_date,
            NaiveDate::from_ymd_opt(2026, 9, 7).unwrap()
        );
        app.move_cursor_days(-1).await.unwrap();
        assert_eq!(app.cursor_date, friday);
    }

    #[test]
    fn visible_hours_can_scroll_to_midnight() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.scroll_hours(4);
        assert_eq!(app.config.day_minutes(), (12 * 60, 24 * 60));
    }
}
