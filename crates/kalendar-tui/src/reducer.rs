use crate::action::Action;
use crate::app::{App, Mode, View};
use crate::command::Command;
use anyhow::Result;

pub(crate) async fn dispatch(app: &mut App, action: Action) -> Result<()> {
    let command = reduce(app, action);
    execute(app, command).await
}

fn reduce(app: &mut App, action: Action) -> Command {
    match action {
        Action::Quit => app.should_quit = true,
        Action::SetView(view) => return Command::LoadView(view),
        Action::MoveDays(days) => return Command::MoveDays(days),
        Action::MoveMonth(months) => return Command::MoveMonth(months),
        Action::MoveTime(minutes) => app.move_time(minutes),
        Action::ScrollHours(hours) => app.scroll_hours(hours),
        Action::CycleEvent(delta) => app.cycle_event(delta),
        Action::Today => return Command::Today,
        Action::OpenSelected if app.view == View::Month && app.selected_event.is_none() => {
            return Command::LoadView(View::Agenda);
        }
        Action::OpenSelected => app.open_selected(),
        Action::NewEvent => app.start_new_event(),
        Action::EditSelected => app.start_edit_event(),
        Action::DeleteSelected => app.start_delete_confirmation(),
        Action::OpenCalendars => app.open_calendar_picker(),
        Action::OpenSearch => {
            app.search = Default::default();
            app.mode = Mode::Search;
        }
        Action::OpenHelp => app.mode = Mode::Help,
        Action::Refresh => return Command::Refresh,
        Action::ClearSelection => app.selected_event = None,
    }
    Command::None
}

async fn execute(app: &mut App, command: Command) -> Result<()> {
    match command {
        Command::None => Ok(()),
        Command::LoadView(view) => app.set_view(view).await,
        Command::MoveDays(days) => app.move_cursor_days(days).await,
        Command::MoveMonth(months) => app.move_month(months).await,
        Command::Today => app.today().await,
        Command::Refresh => app.refresh(true).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use chrono::NaiveDate;
    use kalendar_core::MockBackend;
    use std::sync::Arc;

    #[tokio::test]
    async fn action_reducer_drives_navigation_and_commands() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        dispatch(&mut app, Action::MoveDays(7)).await.unwrap();
        assert_eq!(
            app.cursor_date,
            NaiveDate::from_ymd_opt(2026, 9, 11).unwrap()
        );
        dispatch(&mut app, Action::SetView(View::Month))
            .await
            .unwrap();
        assert_eq!(app.view, View::Month);
        app.selected_event = None;
        dispatch(&mut app, Action::OpenSelected).await.unwrap();
        assert_eq!(app.view, View::Agenda);
        dispatch(&mut app, Action::Quit).await.unwrap();
        assert!(app.should_quit);
    }
}
