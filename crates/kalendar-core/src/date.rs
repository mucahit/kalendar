use crate::DateRange;
use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveDate, TimeZone};

#[must_use]
pub fn local_at(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Local> {
    let naive = date
        .and_hms_opt(hour, minute, 0)
        .expect("hour and minute supplied by application are valid");
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(value) | LocalResult::Ambiguous(value, _) => value,
        LocalResult::None => {
            // A DST jump can make a local wall-clock time invalid. Move forward to
            // the first representable hour instead of panicking during navigation.
            let fallback = naive + chrono::Duration::hours(1);
            Local
                .from_local_datetime(&fallback)
                .earliest()
                .expect("a time one hour after a DST gap is representable")
        }
    }
}

#[must_use]
pub fn week_start(date: NaiveDate) -> NaiveDate {
    let days = i64::from(date.weekday().num_days_from_monday());
    date - chrono::Duration::days(days)
}

#[must_use]
pub fn month_grid_start(date: NaiveDate) -> NaiveDate {
    week_start(date.with_day(1).expect("every month has a first day"))
}

#[must_use]
pub fn range_for_week(date: NaiveDate) -> DateRange {
    let from_date = week_start(date);
    let to_date = from_date
        .checked_add_days(Days::new(7))
        .expect("week navigation stays inside chrono's supported range");
    DateRange::new(local_at(from_date, 0, 0), local_at(to_date, 0, 0))
}

#[must_use]
pub fn range_for_month(date: NaiveDate) -> DateRange {
    let from_date = month_grid_start(date);
    let to_date = from_date
        .checked_add_days(Days::new(42))
        .expect("month navigation stays inside chrono's supported range");
    DateRange::new(local_at(from_date, 0, 0), local_at(to_date, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe::Lisbon;

    #[test]
    fn week_begins_on_monday_across_month_boundary() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        assert_eq!(
            week_start(date),
            NaiveDate::from_ymd_opt(2026, 8, 31).unwrap()
        );
    }

    #[test]
    fn month_grid_covers_six_weeks() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let range = range_for_month(date);
        assert_eq!((range.to - range.from).num_days(), 42);
    }

    #[test]
    fn leap_day_has_correct_week_start() {
        let date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        assert_eq!(
            week_start(date),
            NaiveDate::from_ymd_opt(2024, 2, 26).unwrap()
        );
    }

    #[test]
    fn calendar_days_remain_dates_across_dst_transitions() {
        let spring_start = Lisbon
            .with_ymd_and_hms(2026, 3, 29, 0, 0, 0)
            .single()
            .unwrap();
        let spring_end = Lisbon
            .with_ymd_and_hms(2026, 3, 30, 0, 0, 0)
            .single()
            .unwrap();
        assert_eq!((spring_end - spring_start).num_hours(), 23);
        assert_eq!(
            spring_end.date_naive() - spring_start.date_naive(),
            chrono::Duration::days(1)
        );

        let autumn_start = Lisbon
            .with_ymd_and_hms(2026, 10, 25, 0, 0, 0)
            .single()
            .unwrap();
        let autumn_end = Lisbon
            .with_ymd_and_hms(2026, 10, 26, 0, 0, 0)
            .single()
            .unwrap();
        assert_eq!((autumn_end - autumn_start).num_hours(), 25);
        assert_eq!(
            autumn_end.date_naive() - autumn_start.date_naive(),
            chrono::Duration::days(1)
        );
    }
}
