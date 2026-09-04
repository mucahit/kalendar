use anyhow::{Context, Result};
use chrono::NaiveTime;
use chrono::format::{Item, StrftimeItems};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_view: String,
    pub week_starts_on: String,
    pub time_format: String,
    pub date_format: String,
    pub day_start: String,
    pub day_end: String,
    pub default_event_duration_minutes: i64,
    pub default_event_start: String,
    pub show_weekends: bool,
    pub show_current_time: bool,
    pub hidden_calendars: Vec<String>,
    pub theme: ThemeConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub today: String,
    pub selected: String,
    pub muted: String,
    pub border: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            today: "cyan".into(),
            selected: "blue".into(),
            muted: "dark_gray".into(),
            border: "dark_gray".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_view: "week".into(),
            week_starts_on: "monday".into(),
            time_format: "24h".into(),
            date_format: "%d %b".into(),
            day_start: "08:00".into(),
            day_end: "20:00".into(),
            default_event_duration_minutes: 60,
            default_event_start: "09:00".into(),
            show_weekends: true,
            show_current_time: true,
            hidden_calendars: Vec::new(),
            theme: ThemeConfig::default(),
        }
    }
}

impl Config {
    #[must_use]
    pub fn path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map_or_else(
                || {
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(".config")
                },
                PathBuf::from,
            )
            .join("kalendar/config.toml")
    }

    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path.map_or_else(Self::path, Path::to_path_buf);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let parsed: Self = toml::from_str(&contents)
                    .with_context(|| format!("parsing configuration at {}", path.display()))?;
                Ok(parsed.normalized())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self, path: Option<&Path>) -> Result<()> {
        let path = path.map_or_else(Self::path, Path::to_path_buf);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let contents = toml::to_string_pretty(self).context("serializing configuration")?;
        fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))
    }

    #[must_use]
    pub fn day_minutes(&self) -> (u16, u16) {
        let parse = |value: &str, fallback: u16| {
            let mut parts = value.split(':');
            let hour = parts.next().and_then(|part| part.parse::<u16>().ok());
            let minute = parts.next().and_then(|part| part.parse::<u16>().ok());
            match (hour, minute) {
                (Some(24), Some(0)) => 24 * 60,
                (Some(hour), Some(minute)) if hour < 24 && minute < 60 => hour * 60 + minute,
                _ => fallback,
            }
        };
        let start = parse(&self.day_start, 8 * 60);
        let end = parse(&self.day_end, 20 * 60);
        if end >= start + 60 {
            (start, end)
        } else {
            (8 * 60, 20 * 60)
        }
    }

    fn normalized(mut self) -> Self {
        if !matches!(self.default_view.as_str(), "agenda" | "week" | "month") {
            self.default_view = "week".into();
        }
        if !matches!(self.week_starts_on.as_str(), "monday" | "sunday") {
            self.week_starts_on = "monday".into();
        }
        if !matches!(self.time_format.as_str(), "12h" | "24h") {
            self.time_format = "24h".into();
        }
        if StrftimeItems::new(&self.date_format).any(|item| item == Item::Error) {
            self.date_format = "%d %b".into();
        }
        if NaiveTime::parse_from_str(&self.default_event_start, "%H:%M").is_err() {
            self.default_event_start = "09:00".into();
        }
        self.default_event_duration_minutes = self.default_event_duration_minutes.clamp(1, 24 * 60);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_working_hours_fall_back() {
        let config = Config {
            day_start: "25:70".into(),
            day_end: "oops".into(),
            ..Config::default()
        };
        assert_eq!(config.day_minutes(), (480, 1200));
    }

    #[test]
    fn midnight_is_a_valid_end_of_day_boundary() {
        let config = Config {
            day_start: "12:00".into(),
            day_end: "24:00".into(),
            ..Config::default()
        };
        assert_eq!(config.day_minutes(), (12 * 60, 24 * 60));
    }

    #[test]
    fn configuration_round_trips() {
        let directory =
            std::env::temp_dir().join(format!("kalendar-config-test-{}", std::process::id()));
        let path = directory.join("config.toml");
        let config = Config {
            default_view: "month".into(),
            hidden_calendars: vec!["work-id".into()],
            ..Config::default()
        };
        config.save(Some(&path)).unwrap();
        let loaded = Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.default_view, "month");
        assert_eq!(loaded.hidden_calendars, ["work-id"]);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn invalid_display_configuration_is_normalized() {
        let directory = std::env::temp_dir().join(format!(
            "kalendar-invalid-config-test-{}",
            std::process::id()
        ));
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            &path,
            "default_view = 'sideways'\nweek_starts_on = 'friday'\ntime_format = 'decimal'\ndate_format = '%Q'\ndefault_event_start = 'noonish'\ndefault_event_duration_minutes = -3\n",
        )
        .unwrap();
        let loaded = Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.default_view, "week");
        assert_eq!(loaded.week_starts_on, "monday");
        assert_eq!(loaded.time_format, "24h");
        assert_eq!(loaded.date_format, "%d %b");
        assert_eq!(loaded.default_event_start, "09:00");
        assert_eq!(loaded.default_event_duration_minutes, 1);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
}
