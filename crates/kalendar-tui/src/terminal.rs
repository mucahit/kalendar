use crate::action::Action;
use crate::app::{App, EditorField, Mode, View};
use crate::event_details::meeting_url;
use crate::reducer::dispatch;
use crate::ui;
use anyhow::{Context, Result};
use chrono::{Local, Timelike};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use kalendar_core::{DeleteScope, RecurrenceScope};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::Duration;

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enabling terminal raw mode")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error).context("entering alternate screen");
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
                return Err(error).context("initializing terminal");
            }
        };
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub async fn run(mut app: App) -> Result<()> {
    install_panic_cleanup();
    let mut guard = TerminalGuard::enter()?;
    let mut dirty = true;
    let mut minute = Local::now().minute();
    while !app.should_quit {
        if dirty {
            guard
                .terminal
                .draw(|frame| ui::render(frame, &app))
                .context("drawing terminal")?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(500)).context("polling terminal input")? {
            match event::read().context("reading terminal input")? {
                TerminalEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('z')
                    {
                        suspend(&mut guard.terminal)?;
                    } else {
                        handle_key(&mut app, key).await;
                    }
                    dirty = true;
                }
                TerminalEvent::Resize(_, _) => dirty = true,
                _ => {}
            }
        }
        let current_minute = Local::now().minute();
        if current_minute != minute {
            minute = current_minute;
            dirty = true;
        }
    }
    Ok(())
}

async fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }
    if app.error.is_some() {
        match key.code {
            KeyCode::Esc => app.error = None,
            KeyCode::Char('r') => {
                app.error = None;
                let result = if app.calendars.is_empty() {
                    app.initialize().await
                } else {
                    app.refresh(true).await
                };
                if let Err(error) = result {
                    app.set_error(error);
                }
            }
            _ => {}
        }
        return;
    }
    app.status = None;
    match app.mode {
        Mode::Normal => handle_normal(app, key).await,
        Mode::EventDetail => handle_detail(app, key),
        Mode::EventEditor => handle_editor(app, key).await,
        Mode::CalendarPicker => handle_calendar_picker(app, key),
        Mode::Search => handle_search(app, key).await,
        Mode::ConfirmDelete => handle_delete(app, key).await,
        Mode::ConfirmUpdate => handle_update_scope(app, key).await,
        Mode::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                app.mode = Mode::Normal;
            }
        }
    }
}

async fn handle_normal(app: &mut App, key: KeyEvent) {
    let action = if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('j') => Some(Action::ScrollHours(1)),
            KeyCode::Char('k') => Some(Action::ScrollHours(-1)),
            _ => None,
        }
    } else {
        match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Char('1' | 'a') => Some(Action::SetView(View::Agenda)),
            KeyCode::Char('2' | 'w') => Some(Action::SetView(View::Week)),
            KeyCode::Char('3' | 'm') => Some(Action::SetView(View::Month)),
            KeyCode::Char('t') => Some(Action::Today),
            KeyCode::Char('H') => Some(Action::MoveDays(-7)),
            KeyCode::Char('L') => Some(Action::MoveDays(7)),
            KeyCode::PageUp => Some(Action::MoveMonth(-1)),
            KeyCode::PageDown => Some(Action::MoveMonth(1)),
            KeyCode::Char('h') | KeyCode::Left => Some(Action::MoveDays(-1)),
            KeyCode::Char('l') | KeyCode::Right => Some(Action::MoveDays(1)),
            KeyCode::Char('j') | KeyCode::Down => Some(match app.view {
                View::Week => Action::MoveTime(30),
                View::Month => Action::MoveDays(7),
                View::Agenda => Action::CycleEvent(1),
            }),
            KeyCode::Char('k') | KeyCode::Up => Some(match app.view {
                View::Week => Action::MoveTime(-30),
                View::Month => Action::MoveDays(-7),
                View::Agenda => Action::CycleEvent(-1),
            }),
            KeyCode::Tab => Some(Action::CycleEvent(1)),
            KeyCode::BackTab => Some(Action::CycleEvent(-1)),
            KeyCode::Enter => Some(Action::OpenSelected),
            KeyCode::Char('o') => {
                open_selected_meeting(app);
                None
            }
            KeyCode::Char('n') => Some(Action::NewEvent),
            KeyCode::Char('e') => Some(Action::EditSelected),
            KeyCode::Char('d') => Some(Action::DeleteSelected),
            KeyCode::Char('c') => Some(Action::OpenCalendars),
            KeyCode::Char('/') => Some(Action::OpenSearch),
            KeyCode::Char('?') => Some(Action::OpenHelp),
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Esc => Some(Action::ClearSelection),
            _ => None,
        }
    };
    if let Some(action) = action
        && let Err(error) = dispatch(app, action).await
    {
        app.set_error(error);
    }
}

fn handle_detail(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.mode = Mode::Normal,
        KeyCode::Char('o') => open_selected_meeting(app),
        KeyCode::Char('e') => app.start_edit_event(),
        KeyCode::Char('d') => app.start_delete_confirmation(),
        _ => {}
    }
}

fn open_selected_meeting(app: &mut App) {
    let Some(url) = app.selected().and_then(meeting_url) else {
        return;
    };
    match std::process::Command::new("open").arg(&url).status() {
        Ok(status) if status.success() => app.status = Some("Opening meeting link".into()),
        Ok(status) => app.set_error(anyhow::anyhow!("opening the meeting link failed: {status}")),
        Err(error) => app.set_error(anyhow::anyhow!(error).context("opening the meeting link")),
    }
}

async fn handle_editor(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        if app.editor_is_recurring() {
            app.mode = Mode::ConfirmUpdate;
        } else if let Err(error) = app.save_editor(RecurrenceScope::ThisEvent).await {
            app.set_error(error);
        }
        return;
    }
    match key.code {
        KeyCode::Esc => {
            app.editor = None;
            app.mode = Mode::Normal;
        }
        KeyCode::Tab => {
            if let Some(editor) = app.editor.as_mut() {
                editor.field = editor.field.next();
            }
        }
        KeyCode::BackTab => {
            if let Some(editor) = app.editor.as_mut() {
                editor.field = editor.field.previous();
            }
        }
        KeyCode::Backspace => {
            if let Some(editor) = app.editor.as_mut() {
                editor.backspace();
            }
        }
        KeyCode::Left => match app.editor.as_ref().map(|editor| editor.field) {
            Some(EditorField::Calendar) => app.cycle_editor_calendar(-1),
            Some(EditorField::AllDay) => {
                if let Some(editor) = app.editor.as_mut() {
                    editor.all_day = !editor.all_day;
                }
            }
            _ => {}
        },
        KeyCode::Right => match app.editor.as_ref().map(|editor| editor.field) {
            Some(EditorField::Calendar) => app.cycle_editor_calendar(1),
            Some(EditorField::AllDay) => {
                if let Some(editor) = app.editor.as_mut() {
                    editor.all_day = !editor.all_day;
                }
            }
            _ => {}
        },
        KeyCode::Char(' ')
            if app
                .editor
                .as_ref()
                .is_some_and(|editor| editor.field == EditorField::AllDay) =>
        {
            if let Some(editor) = app.editor.as_mut() {
                editor.all_day = !editor.all_day;
            }
        }
        KeyCode::Char(character)
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
        {
            if let Some(editor) = app.editor.as_mut() {
                editor.insert(character);
            }
        }
        _ => {}
    }
}

fn handle_calendar_picker(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_calendar_picker(),
        KeyCode::Up | KeyCode::Char('k') => {
            app.calendar_picker_index = app.calendar_picker_index.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.calendar_picker_index =
                (app.calendar_picker_index + 1).min(app.calendars.len().saturating_sub(1))
        }
        KeyCode::Char(' ') => app.toggle_calendar(),
        KeyCode::Enter => {
            if let Err(error) = app.apply_calendar_visibility() {
                app.set_error(error);
            }
        }
        _ => {}
    }
}

async fn handle_search(app: &mut App, key: KeyEvent) {
    let mut update = false;
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            if let Err(error) = app.accept_search().await {
                app.set_error(error);
            }
        }
        KeyCode::Up => app.search.selected = app.search.selected.saturating_sub(1),
        KeyCode::Down => {
            app.search.selected =
                (app.search.selected + 1).min(app.search.results.len().saturating_sub(1))
        }
        KeyCode::Backspace => {
            app.search.query.pop();
            update = true;
        }
        KeyCode::Char(character)
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
        {
            app.search.query.push(character);
            update = true;
        }
        _ => {}
    }
    if update && let Err(error) = app.update_search().await {
        app.set_error(error);
    }
}

async fn handle_delete(app: &mut App, key: KeyEvent) {
    let recurring = app
        .selected()
        .is_some_and(|event| event.recurrence.is_some());
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => app.cancel_delete_confirmation(),
        KeyCode::Char('y' | '1') => {
            if let Err(error) = app.delete_selected(DeleteScope::ThisEvent).await {
                app.set_error(error);
            }
        }
        KeyCode::Char('2') if recurring => {
            if let Err(error) = app.delete_selected(DeleteScope::ThisAndFuture).await {
                app.set_error(error);
            }
        }
        _ => {}
    }
}

async fn handle_update_scope(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.mode = Mode::EventEditor,
        KeyCode::Char('1' | 'y') => {
            if let Err(error) = app.save_editor(RecurrenceScope::ThisEvent).await {
                app.set_error(error);
            }
        }
        KeyCode::Char('2') => {
            if let Err(error) = app.save_editor(RecurrenceScope::ThisAndFuture).await {
                app.set_error(error);
            }
        }
        _ => {}
    }
}

fn install_panic_cleanup() {
    std::panic::set_hook(Box::new(|info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        eprintln!("kalendar stopped unexpectedly: {info}");
    }));
}

fn suspend(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode().context("leaving raw mode before suspend")?;
    execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)
        .context("restoring terminal before suspend")?;
    let status = std::process::Command::new("kill")
        .args(["-TSTP", &std::process::id().to_string()])
        .status()
        .context("suspending kalendar")?;
    if !status.success() {
        return Err(anyhow::anyhow!("could not suspend kalendar: {status}"));
    }
    enable_raw_mode().context("restoring raw mode after resume")?;
    execute!(terminal.backend_mut(), EnterAlternateScreen, Hide)
        .context("restoring alternate screen after resume")?;
    terminal.clear().context("redrawing after resume")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use chrono::NaiveDate;
    use kalendar_core::MockBackend;
    use std::sync::Arc;

    #[tokio::test]
    async fn control_c_quits_from_a_modal_mode() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        app.mode = Mode::Help;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        )
        .await;
        assert!(app.should_quit);
    }
}
