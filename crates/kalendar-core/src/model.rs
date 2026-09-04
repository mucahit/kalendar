use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CalendarId(pub String);

impl Display for CalendarId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub String);

impl Display for EventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum CalendarSource {
    Local,
    ICloud,
    CalDav,
    Exchange,
    Birthdays,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: CalendarId,
    pub name: String,
    pub color: Option<RgbColor>,
    pub writable: bool,
    pub source: CalendarSource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    #[default]
    Busy,
    Free,
    Tentative,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Recurrence {
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub calendar_id: CalendarId,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub url: Option<String>,
    pub recurrence: Option<Recurrence>,
    #[serde(default)]
    pub availability: Availability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NewEvent {
    pub calendar_id: CalendarId,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_day: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<Option<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteScope {
    ThisEvent,
    ThisAndFuture,
    AllEvents,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceScope {
    ThisEvent,
    ThisAndFuture,
    AllEvents,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    NotDetermined,
    Denied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DateRange {
    pub from: DateTime<Local>,
    pub to: DateTime<Local>,
}

impl DateRange {
    #[must_use]
    pub fn new(from: DateTime<Local>, to: DateTime<Local>) -> Self {
        Self { from, to }
    }

    #[must_use]
    pub fn contains(&self, moment: DateTime<Local>) -> bool {
        moment >= self.from && moment < self.to
    }
}
