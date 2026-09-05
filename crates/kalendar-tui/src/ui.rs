use crate::app::{App, EditorField, Mode, View, event_occurs_on};
use crate::event_details::{description, meeting_url};
use crate::geometry::layout_day;
use chrono::{Datelike, Days, Local, Timelike};
use kalendar_core::{CalendarId, Event};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area);
        return;
    }
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    render_header(frame, chunks[0], app);
    match app.view {
        View::Week => render_week(frame, chunks[1], app),
        View::Month => render_month(frame, chunks[1], app),
        View::Agenda => render_agenda(frame, chunks[1], app),
    }
    render_footer(frame, chunks[2], app);
    match app.mode {
        Mode::EventDetail => render_detail(frame, area, app),
        Mode::EventEditor => render_editor(frame, area, app),
        Mode::CalendarPicker => render_calendar_picker(frame, area, app),
        Mode::Search => render_search(frame, area, app),
        Mode::ConfirmDelete => render_delete(frame, area, app),
        Mode::ConfirmUpdate => render_update_scope(frame, area, app),
        Mode::Help => render_help(frame, area, app),
        Mode::Normal => {}
    }
    if let Some(error) = &app.error {
        render_error(frame, area, app, error);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let view = match app.view {
        View::Week => "WEEK",
        View::Month => "MONTH",
        View::Agenda => "AGENDA",
    };
    let period = match app.view {
        View::Week => {
            let start = app.week_start();
            format!(
                "{} {}  ·  W{:02}",
                app.cursor_date.format("%B"),
                app.cursor_date.year(),
                start.iso_week().week()
            )
        }
        View::Month => app.cursor_date.format("%B %Y").to_string(),
        View::Agenda => format!(
            "{} → 14 days",
            app.cursor_date.format(&app.config.date_format)
        ),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " KALENDAR ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {period}")),
            Span::styled(
                format!("   {view}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(border_color(app))),
        ),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let help = match app.mode {
        Mode::Normal if app.selected().and_then(meeting_url).is_some() => {
            "o join   Enter details   n new   / search   c calendars   ? help   q quit"
        }
        Mode::Normal => {
            "1 agenda  2 week  3 month   n new   / search   c calendars   ? help   q quit"
        }
        Mode::EventDetail if app.selected().and_then(meeting_url).is_some() => {
            "o join   e edit   d delete   Esc close"
        }
        Mode::EventDetail => "e edit   d delete   Esc close",
        Mode::EventEditor => "Tab next   ←/→ choose/toggle   Ctrl+S save   Esc cancel",
        Mode::CalendarPicker => "↑/↓ move   Space toggle   Enter apply   Esc cancel",
        Mode::Search => "type to search   ↑/↓ results   Enter jump   Esc close",
        Mode::ConfirmDelete
            if app
                .selected()
                .is_some_and(|event| event.recurrence.is_some()) =>
        {
            "y/1 this event   2 this and future   Esc cancel"
        }
        Mode::ConfirmDelete => "y confirm   Esc cancel",
        Mode::ConfirmUpdate => "y/1 this event   2 this and future   Esc back",
        Mode::Help => "Esc close",
    };
    let mut line = Line::from(Span::styled(help, Style::default().fg(muted_color(app))));
    if let Some(status) = &app.status {
        line = Line::from(vec![
            Span::styled(
                format!(" {status} "),
                Style::default().fg(Color::Black).bg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled(help, Style::default().fg(muted_color(app))),
        ]);
    }
    frame.render_widget(Paragraph::new(line), area);
}

fn render_week(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let header_height = 3_u16;
    let gutter_width = 7_u16;
    let grid_top = area.y + header_height;
    let grid_height = area.height.saturating_sub(header_height);
    let usable_width = area.width.saturating_sub(gutter_width);
    let day_offsets: &[u16] = if app.config.show_weekends {
        &[0, 1, 2, 3, 4, 5, 6]
    } else if app.config.week_starts_on.eq_ignore_ascii_case("sunday") {
        &[1, 2, 3, 4, 5]
    } else {
        &[0, 1, 2, 3, 4]
    };
    let day_count = u16::try_from(day_offsets.len()).unwrap_or(7);
    let base_day_width = usable_width / day_count;
    let week = app.week_start();
    let now = Local::now();
    let (visible_start, visible_end) = app.config.day_minutes();
    let visible = app.visible_events().collect::<Vec<_>>();
    let span = u32::from(visible_end.saturating_sub(visible_start)).max(1);

    let today_is_visible = day_offsets.iter().any(|offset| {
        week.checked_add_days(Days::new(u64::from(*offset))) == Some(now.date_naive())
    });
    if app.config.show_current_time && today_is_visible {
        let minutes = u16::try_from(now.hour() * 60 + now.minute()).unwrap_or_default();
        if minutes >= visible_start && minutes < visible_end {
            let offset = u32::from(minutes - visible_start) * u32::from(grid_height) / span;
            let y = grid_top + u16::try_from(offset).unwrap_or(0);
            let label = format!(
                " NOW {}",
                "─".repeat(usize::from(usable_width.saturating_sub(6)))
            );
            frame.render_widget(
                Paragraph::new(label).style(Style::default().fg(Color::Red)),
                Rect::new(area.x + gutter_width, y, usable_width, 1),
            );
        }
    }

    let cursor_day_offset = (app.cursor_date - week).num_days();
    if let Some(column) = day_offsets
        .iter()
        .position(|offset| i64::from(*offset) == cursor_day_offset)
        && app.cursor_minutes >= visible_start
        && app.cursor_minutes < visible_end
    {
        let column = u16::try_from(column).unwrap_or_default();
        let x = area.x + gutter_width + column * base_day_width;
        let width = if column + 1 == day_count {
            area.right().saturating_sub(x)
        } else {
            base_day_width
        };
        let offset = u32::from(app.cursor_minutes - visible_start) * u32::from(grid_height) / span;
        let y = grid_top + u16::try_from(offset).unwrap_or_default();
        frame.render_widget(
            Paragraph::new(format!(
                "▶{}",
                "·".repeat(usize::from(width.saturating_sub(2)))
            ))
            .style(Style::default().fg(config_color(&app.config.theme.selected, Color::Blue))),
            Rect::new(x + 1, y, width.saturating_sub(1), 1),
        );
    }

    frame.render_widget(
        Paragraph::new("all-day").style(Style::default().fg(muted_color(app))),
        Rect::new(area.x, area.y + 1, gutter_width, 2),
    );
    for (column, day_offset) in day_offsets.iter().copied().enumerate() {
        let date = week
            .checked_add_days(Days::new(u64::from(day_offset)))
            .expect("week day is representable");
        let column = u16::try_from(column).unwrap_or_default();
        let x = area.x + gutter_width + column * base_day_width;
        let width = if column + 1 == day_count {
            area.right().saturating_sub(x)
        } else {
            base_day_width
        };
        let selected = date == app.cursor_date;
        let today = date == now.date_naive();
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(config_color(&app.config.theme.selected, Color::Blue))
                .add_modifier(Modifier::BOLD)
        } else if today {
            Style::default()
                .fg(config_color(&app.config.theme.today, Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let label = format!(
            " {} {:02} ",
            date.format("%a").to_string().to_uppercase(),
            date.day()
        );
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style),
            Rect::new(x, area.y, width, 1),
        );
        frame.render_widget(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(border_color(app))),
            Rect::new(x, area.y, width, area.height),
        );

        let all_day: Vec<_> = visible
            .iter()
            .copied()
            .filter(|event| event.all_day && event_occurs_on(event, date))
            .collect();
        let mut displayed_all_day: Vec<_> = all_day.iter().copied().take(2).collect();
        if let Some(selected) = app.selected_event.as_ref()
            && let Some(selected_event) =
                all_day.iter().copied().find(|event| &event.id == selected)
            && !displayed_all_day
                .iter()
                .any(|event| event.id == selected_event.id)
            && displayed_all_day.len() == 2
        {
            displayed_all_day[1] = selected_event;
        }
        for (row, event) in displayed_all_day.into_iter().enumerate() {
            let overflow = if row == 1 && all_day.len() > 2 {
                format!(" +{}", all_day.len() - 2)
            } else {
                String::new()
            };
            let title_width = usize::from(width.saturating_sub(2)).saturating_sub(overflow.width());
            let all_day_text = format!("{}{}", truncate(&event.title, title_width), overflow);
            let style = event_style(app, event, app.selected_event.as_ref() == Some(&event.id));
            frame.render_widget(
                Paragraph::new(all_day_text).style(style),
                Rect::new(
                    x + 1,
                    area.y + 1 + u16::try_from(row).unwrap_or_default(),
                    width.saturating_sub(1),
                    1,
                ),
            );
        }

        let day_events: Vec<_> = visible
            .iter()
            .copied()
            .filter(|event| !event.all_day)
            .collect();
        for layout in layout_day(&day_events, date, visible_start, visible_end, grid_height) {
            let Some(event) = day_events
                .iter()
                .find(|event| event.id == layout.event_id)
                .copied()
            else {
                continue;
            };
            let inner_width = width.saturating_sub(1);
            let slot_count = u16::try_from(layout.slot_count).unwrap_or(1).max(1);
            let slot_width = (inner_width / slot_count).max(1);
            let slot = u16::try_from(layout.x_slot).unwrap_or(0);
            let slot_offset = slot
                .saturating_mul(slot_width)
                .min(inner_width.saturating_sub(1));
            let event_x = x + 1 + slot_offset;
            let available_width = x.saturating_add(width).saturating_sub(event_x).max(1);
            let event_width = if usize::from(slot + 1) == layout.slot_count {
                available_width
            } else {
                slot_width.min(available_width)
            };
            let event_area = Rect::new(
                event_x,
                grid_top + layout.top,
                event_width.max(1),
                layout.height.max(1),
            );
            let selected = app.selected_event.as_ref() == Some(&event.id);
            let prefix = if selected { "▶" } else { "▌" };
            let label = if event_area.height > 1 {
                format!(
                    "{prefix}{}\n {}",
                    event.title,
                    format_time(app, event.start)
                )
            } else {
                format!("{prefix}{}", event.title)
            };
            frame.render_widget(
                Paragraph::new(label)
                    .style(event_style(app, event, selected))
                    .wrap(Wrap { trim: true }),
                event_area,
            );
        }
    }

    for hour in ((visible_start / 60)..=(visible_end / 60)).step_by(2) {
        let minutes = hour * 60;
        if minutes < visible_start || minutes > visible_end {
            continue;
        }
        let offset = u32::from(minutes - visible_start) * u32::from(grid_height) / span;
        let y = grid_top
            + u16::try_from(offset)
                .unwrap_or(0)
                .min(grid_height.saturating_sub(1));
        frame.render_widget(
            Paragraph::new(format!(" {}", format_minutes(app, hour * 60)))
                .style(Style::default().fg(muted_color(app))),
            Rect::new(area.x, y, gutter_width.saturating_sub(1), 1),
        );
    }
    if app.cursor_minutes >= visible_start && app.cursor_minutes < visible_end {
        let offset = u32::from(app.cursor_minutes - visible_start) * u32::from(grid_height) / span;
        let y = grid_top + u16::try_from(offset).unwrap_or_default();
        frame.render_widget(
            Paragraph::new(format!(">{}", format_minutes(app, app.cursor_minutes))).style(
                Style::default()
                    .fg(config_color(&app.config.theme.selected, Color::Blue))
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(area.x, y, gutter_width.saturating_sub(1), 1),
        );
    }
}

fn render_month(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
    let (labels, day_offsets): (&[&str], &[usize]) =
        if !app.config.show_weekends && app.config.week_starts_on.eq_ignore_ascii_case("sunday") {
            (&["MON", "TUE", "WED", "THU", "FRI"], &[1, 2, 3, 4, 5])
        } else if !app.config.show_weekends {
            (&["MON", "TUE", "WED", "THU", "FRI"], &[0, 1, 2, 3, 4])
        } else if app.config.week_starts_on.eq_ignore_ascii_case("sunday") {
            (
                &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
                &[0, 1, 2, 3, 4, 5, 6],
            )
        } else {
            (
                &["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"],
                &[0, 1, 2, 3, 4, 5, 6],
            )
        };
    let column_count = u32::try_from(labels.len()).unwrap_or(7);
    let headers =
        Layout::horizontal(vec![Constraint::Ratio(1, column_count); labels.len()]).split(chunks[0]);
    for (index, label) in labels.iter().enumerate() {
        frame.render_widget(
            Paragraph::new(*label).alignment(Alignment::Center).style(
                Style::default()
                    .fg(muted_color(app))
                    .add_modifier(Modifier::BOLD),
            ),
            headers[index],
        );
    }
    let rows = Layout::vertical([Constraint::Ratio(1, 6); 6]).split(chunks[1]);
    let start = app.month_grid_start();
    let today = Local::now().date_naive();
    let visible: Vec<_> = app.visible_events().collect();
    for (week_index, row) in rows.iter().enumerate() {
        let cells =
            Layout::horizontal(vec![Constraint::Ratio(1, column_count); labels.len()]).split(*row);
        for (day_index, cell) in cells.iter().enumerate() {
            let offset = u64::try_from(week_index * 7 + day_offsets[day_index]).unwrap_or(0);
            let date = start
                .checked_add_days(Days::new(offset))
                .expect("month grid day is representable");
            let selected = date == app.cursor_date;
            let outside = date.month() != app.cursor_date.month();
            let mut style = Style::default().fg(if outside {
                muted_color(app)
            } else {
                Color::Gray
            });
            if date == today {
                style = style
                    .fg(config_color(&app.config.theme.today, Color::Cyan))
                    .add_modifier(Modifier::BOLD);
            }
            if selected {
                style = style
                    .bg(config_color(&app.config.theme.selected, Color::Blue))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }
            let events: Vec<_> = visible
                .iter()
                .copied()
                .filter(|event| event_occurs_on(event, date))
                .collect();
            let capacity = usize::from(cell.height.saturating_sub(2));
            let mut lines = vec![Line::styled(format!(" {}", date.day()), style)];
            for event in events.iter().take(capacity) {
                let marker = if event.all_day { "◆" } else { "•" };
                lines.push(Line::styled(
                    format!("{marker} {}", event.title),
                    event_style(app, event, app.selected_event.as_ref() == Some(&event.id)),
                ));
            }
            if events.len() > capacity && capacity > 0 {
                let last = lines.len().saturating_sub(1);
                lines[last] = Line::styled(
                    format!("+{} more", events.len() - capacity + 1),
                    Style::default().fg(muted_color(app)),
                );
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::TOP | Borders::LEFT)
                            .border_style(Style::default().fg(border_color(app))),
                    )
                    .wrap(Wrap { trim: true }),
                *cell,
            );
        }
    }
}

fn render_agenda(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = Vec::new();
    let mut selected_line = 0_usize;
    let visible: Vec<_> = app.visible_events().collect();
    for offset in 0..15_u64 {
        let date = app
            .cursor_date
            .checked_add_days(Days::new(offset))
            .expect("agenda date is representable");
        lines.push(Line::styled(
            date.format("%A %-d %B").to_string().to_uppercase(),
            Style::default()
                .fg(if date == Local::now().date_naive() {
                    config_color(&app.config.theme.today, Color::Cyan)
                } else {
                    Color::Gray
                })
                .add_modifier(Modifier::BOLD),
        ));
        let events: Vec<_> = visible
            .iter()
            .copied()
            .filter(|event| event_occurs_on(event, date))
            .collect();
        if events.is_empty() {
            lines.push(Line::styled(
                "        No events",
                Style::default().fg(muted_color(app)),
            ));
        } else {
            for event in events {
                let time = if event.all_day {
                    "all-day".to_owned()
                } else {
                    format_time(app, event.start)
                };
                let selected = app.selected_event.as_ref() == Some(&event.id);
                if selected {
                    selected_line = lines.len();
                }
                lines.push(Line::from(vec![
                    Span::styled(
                        if selected { "▶ " } else { "  " },
                        event_style(app, event, selected),
                    ),
                    Span::styled(
                        format!("{time:>7}  "),
                        Style::default().fg(muted_color(app)),
                    ),
                    Span::styled(event.title.clone(), event_style(app, event, selected)),
                ]));
            }
        }
        lines.push(Line::raw(""));
    }
    let available = usize::from(area.height.saturating_sub(2)).max(1);
    let scroll = selected_line.saturating_sub(available.saturating_sub(3));
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(border_color(app)))
                    .title(" Upcoming "),
            ),
        area,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(event) = app.selected() else {
        return;
    };
    let calendar = app
        .calendar(&event.calendar_id)
        .map(|calendar| calendar.name.as_str())
        .unwrap_or("Unknown");
    let when = format_event_when(app, event);
    let mut lines = vec![
        Line::raw(when),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Calendar   ", Style::default().fg(muted_color(app))),
            Span::raw(calendar),
        ]),
    ];
    if let Some(location) = &event.location {
        lines.push(Line::from(vec![
            Span::styled("Location   ", Style::default().fg(muted_color(app))),
            Span::raw(location),
        ]));
    }
    if let Some(recurrence) = &event.recurrence {
        lines.push(Line::from(vec![
            Span::styled("Repeats    ", Style::default().fg(muted_color(app))),
            Span::raw(&recurrence.description),
        ]));
    }
    let meeting = meeting_url(event);
    if let Some(url) = &event.url {
        lines.push(Line::from(vec![
            Span::styled(
                if meeting.as_deref() == Some(url) {
                    "Meeting    "
                } else {
                    "URL        "
                },
                Style::default().fg(muted_color(app)),
            ),
            Span::raw(url),
        ]));
    }
    if let Some(url) = meeting
        .as_deref()
        .filter(|url| event.url.as_deref() != Some(*url))
    {
        lines.push(Line::from(vec![
            Span::styled("Meeting    ", Style::default().fg(muted_color(app))),
            Span::raw(url),
        ]));
    }
    if let Some(description) = description(event) {
        lines.push(Line::raw(""));
        lines.extend(description.lines().map(|line| Line::raw(line.to_owned())));
    }
    let detail_height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .max(18);
    let popup = centered(area, 64, detail_height);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(event_style(app, event, false))
                .title(format!(" {} ", event.title)),
        ),
        popup,
    );
}

fn render_editor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(editor) = &app.editor else {
        return;
    };
    let calendar = app
        .calendar(&editor.calendar_id)
        .map(|calendar| calendar.name.as_str())
        .unwrap_or("Unknown");
    let value = |field: EditorField, label: &str, text: String| {
        let style = if editor.field == field {
            Style::default().fg(Color::Black).bg(Color::Cyan)
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::styled(
                format!("{label:<10}"),
                Style::default().fg(muted_color(app)),
            ),
            Span::styled(format!(" {text} "), style),
        ])
    };
    let lines = vec![
        value(
            EditorField::Title,
            "Title",
            format!(
                "{}{}",
                editor.title,
                if editor.field == EditorField::Title {
                    "█"
                } else {
                    ""
                }
            ),
        ),
        value(EditorField::Calendar, "Calendar", calendar.to_owned()),
        value(EditorField::Date, "Date", editor.date.clone()),
        value(EditorField::Start, "Start", editor.start.clone()),
        value(EditorField::End, "End", editor.end.clone()),
        value(
            EditorField::AllDay,
            "All day",
            if editor.all_day {
                "[x]".into()
            } else {
                "[ ]".into()
            },
        ),
        value(EditorField::Location, "Location", editor.location.clone()),
        value(EditorField::Notes, "Notes", editor.notes.clone()),
    ];
    let popup = centered(area, 62, 16);
    frame.render_widget(Clear, popup);
    let title = if editor.editing.is_some() {
        " Edit event "
    } else {
        " New event "
    };
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        ),
        popup,
    );
}

fn render_calendar_picker(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let start = app.calendar_picker_index.saturating_sub(12);
    let lines: Vec<_> = app
        .calendars
        .iter()
        .enumerate()
        .skip(start)
        .take(14)
        .map(|(index, calendar)| {
            let checked = if app.visible_calendars.contains(&calendar.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let cursor = if index == app.calendar_picker_index {
                "▶"
            } else {
                " "
            };
            Line::styled(
                format!(
                    "{cursor} {checked} {}{}",
                    calendar.name,
                    if calendar.writable {
                        ""
                    } else {
                        "  (read-only)"
                    }
                ),
                if index == app.calendar_picker_index {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    color_for_calendar(app, &calendar.id)
                },
            )
        })
        .collect();
    let height = u16::try_from(lines.len())
        .unwrap_or(10)
        .saturating_add(2)
        .min(18);
    let popup = centered(area, 52, height);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Calendars "),
        ),
        popup,
    );
}

fn render_search(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.search.query),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ]),
        Line::raw(""),
    ];
    if app.search.query.is_empty() {
        lines.push(Line::styled(
            "Type to search titles, locations, and notes.",
            Style::default().fg(muted_color(app)),
        ));
    }
    let start = app.search.selected.saturating_sub(12);
    for (index, event) in app.search.results.iter().enumerate().skip(start).take(14) {
        let selected = index == app.search.selected;
        lines.push(Line::styled(
            format!(
                "{}  {}  {}",
                event.start.format("%-d %b"),
                if event.all_day {
                    "all-day".into()
                } else {
                    format_time(app, event.start)
                },
                event.title
            ),
            if selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                event_style(app, event, false)
            },
        ));
    }
    let popup = centered(area, 72, 20);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(
                    " Search · {} result{} ",
                    app.search.results.len(),
                    if app.search.results.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                )),
        ),
        popup,
    );
}

fn render_delete(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let recurring = app
        .selected()
        .is_some_and(|event| event.recurrence.is_some());
    let text = if recurring {
        Text::from(vec![
            Line::raw("Delete this recurring event?"),
            Line::raw(""),
            Line::raw("1 / y   This event"),
            Line::raw("2       This and future events"),
            Line::styled(
                "All occurrences is unavailable because EventKit cannot safely identify earlier instances.",
                Style::default().fg(muted_color(app)),
            ),
        ])
    } else {
        Text::from(vec![
            Line::raw("Delete this event?"),
            Line::raw(""),
            Line::raw("y confirm      Esc cancel"),
        ])
    };
    let popup = centered(area, 66, if recurring { 10 } else { 7 });
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .title(" Confirm delete "),
        ),
        popup,
    );
}

fn render_update_scope(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = Text::from(vec![
        Line::raw("Update this recurring event?"),
        Line::raw(""),
        Line::raw("1 / y   This event"),
        Line::raw("2       This and future events"),
        Line::styled(
            "All occurrences is unavailable because EventKit cannot safely identify earlier instances.",
            Style::default().fg(muted_color(app)),
        ),
    ]);
    let popup = centered(area, 66, 10);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: true }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Recurring update "),
        ),
        popup,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = vec![
        Line::styled(
            "Navigation",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw("h/l or ←/→     previous / next day"),
        Line::raw("j/k or ↓/↑     time, week row, or event"),
        Line::raw("Tab / Shift+Tab cycle selectable events"),
        Line::raw("H/L             previous / next week"),
        Line::raw("PageUp/PageDown previous / next month"),
        Line::raw("t               today"),
        Line::raw("Ctrl+j / Ctrl+k scroll visible hours"),
        Line::raw("Ctrl+z          suspend; terminal restores on resume"),
        Line::raw(""),
        Line::styled(
            "Views",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw("1 or a  agenda     2 or w  week     3 or m  month"),
        Line::raw(""),
        Line::styled(
            "Events",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw("n new   Enter details   o join   e edit   d delete"),
        Line::raw(""),
        Line::styled(
            "Other",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw("c calendars    / search    r refresh    q quit    Esc back"),
    ];
    let popup = centered(area, 68, 24);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color(app)))
                .title(" Keyboard shortcuts "),
        ),
        popup,
    );
}

fn render_error(frame: &mut Frame<'_>, area: Rect, app: &App, error: &str) {
    let popup = centered(area, 68, 8);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(error),
            Line::raw(""),
            Line::styled(
                "Press Esc to dismiss · r to retry",
                Style::default().fg(muted_color(app)),
            ),
        ])
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .title(" Error "),
        ),
        popup,
    );
}

fn render_too_small(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(format!(
            "Terminal too small.\nMinimum: {MIN_WIDTH}x{MIN_HEIGHT}\nCurrent: {}x{}",
            area.width, area.height
        ))
        .alignment(Alignment::Center),
        area,
    );
}

fn event_style(app: &App, event: &Event, selected: bool) -> Style {
    let color = color_for_calendar(app, &event.calendar_id);
    if selected {
        color
            .bg(config_color(&app.config.theme.selected, Color::Blue))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        color
    }
}

fn format_time(app: &App, value: chrono::DateTime<Local>) -> String {
    if app.config.time_format == "12h" {
        value.format("%-I:%M %p").to_string()
    } else {
        value.format("%H:%M").to_string()
    }
}

fn format_event_when(app: &App, event: &Event) -> String {
    let start_date = event.start.date_naive();
    let end_date = event.end.date_naive();
    if event.all_day {
        let last_date = if end_date > start_date {
            end_date.pred_opt().unwrap_or(start_date)
        } else {
            start_date
        };
        if last_date > start_date {
            format!(
                "{} → {}\nall day",
                start_date.format("%A, %-d %B %Y"),
                last_date.format("%A, %-d %B %Y")
            )
        } else {
            format!("{} · all day", start_date.format("%A, %-d %B %Y"))
        }
    } else if end_date != start_date {
        format!(
            "{} {}\n→ {} {}",
            start_date.format("%A, %-d %B %Y"),
            format_time(app, event.start),
            end_date.format("%A, %-d %B %Y"),
            format_time(app, event.end)
        )
    } else {
        format!(
            "{}\n{} → {}",
            start_date.format("%A, %-d %B %Y"),
            format_time(app, event.start),
            format_time(app, event.end)
        )
    }
}

fn format_minutes(app: &App, minutes: u16) -> String {
    let hour = minutes / 60;
    let minute = minutes % 60;
    if app.config.time_format == "12h" {
        let clock_hour = hour % 24;
        let suffix = if clock_hour < 12 { "a" } else { "p" };
        let display = match clock_hour % 12 {
            0 => 12,
            value => value,
        };
        format!("{display}:{minute:02}{suffix}")
    } else {
        format!("{hour:02}:{minute:02}")
    }
}

fn config_color(value: &str, fallback: Color) -> Color {
    match value.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark_gray" | "dark_grey" => Color::DarkGray,
        "white" => Color::White,
        _ => fallback,
    }
}

fn muted_color(app: &App) -> Color {
    config_color(&app.config.theme.muted, Color::DarkGray)
}

fn border_color(app: &App) -> Color {
    config_color(&app.config.theme.border, Color::DarkGray)
}

fn color_for_calendar(app: &App, calendar_id: &CalendarId) -> Style {
    app.calendar(calendar_id)
        .and_then(|calendar| calendar.color)
        .map_or_else(
            || Style::default().fg(Color::Gray),
            |color| Style::default().fg(Color::Rgb(color.red, color.green, color.blue)),
        )
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn truncate(value: &str, width: usize) -> String {
    if value.width() <= width {
        return value.to_owned();
    }
    if width <= 1 {
        return "…".into();
    }
    let mut result = String::new();
    for character in value.chars() {
        if result.width() + character.to_string().width() + 1 > width {
            break;
        }
        result.push(character);
    }
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use chrono::NaiveDate;
    use kalendar_core::MockBackend;
    use ratatui::{Terminal, backend::TestBackend};
    use std::sync::Arc;

    #[tokio::test]
    async fn renders_week_and_small_terminal_message() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("KALENDAR"));
        assert!(content.contains("Design"));

        app.config.show_weekends = false;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!content.contains("SAT 05"));
        assert!(!content.contains("SUN 06"));

        app.config.show_weekends = true;
        app.view = View::Month;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("September 2026"));
        assert!(content.contains("+2 more"));

        app.view = View::Agenda;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("FRIDAY 4 SEPTEMBER"));

        app.view = View::Week;
        app.selected_event = Some(kalendar_core::EventId("design".into()));
        app.mode = Mode::Normal;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("o join"));

        app.mode = Mode::EventDetail;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Studio 2"));
        assert!(content.contains("calendar interaction prototypes"));

        let selected = app
            .events
            .iter_mut()
            .find(|event| event.id.0 == "design")
            .unwrap();
        selected.url = None;
        selected.notes =
            Some("<p>Review agenda</p><p>Join via https://meet.google.com/abc-defg-hij</p>".into());
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Review agenda"));
        assert!(content.contains("Join via"));
        assert!(content.contains("o join"));
        assert!(!content.contains("<p>"));

        app.mode = Mode::Normal;
        app.visible_calendars.clear();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!content.contains("Design"));

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Terminal too small"));
    }

    #[tokio::test]
    async fn renders_crowded_and_multi_day_week_at_minimum_size() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let mut app = App::new(
            Arc::new(MockBackend::demo(date)),
            Config::default(),
            None,
            date,
            Some(View::Week),
        );
        app.initialize().await.unwrap();

        let overlapping = app
            .events
            .iter()
            .find(|event| event.id.0 == "design")
            .unwrap()
            .clone();
        for index in 0..20 {
            let mut event = overlapping.clone();
            event.id = kalendar_core::EventId(format!("crowded-{index}"));
            event.title = format!("Overlap {index}");
            app.events.push(event);
        }

        let mut multi_day = app
            .events
            .iter()
            .find(|event| event.id.0 == "all-day-release")
            .unwrap()
            .clone();
        multi_day.id = kalendar_core::EventId("conference".into());
        multi_day.title = "Conference".into();
        multi_day.start = kalendar_core::local_at(date - chrono::Duration::days(2), 0, 0);
        multi_day.end = kalendar_core::local_at(date + chrono::Duration::days(1), 0, 0);
        app.events.push(multi_day);

        let conference = app
            .events
            .iter()
            .find(|event| event.id.0 == "conference")
            .unwrap();
        let when = format_event_when(&app, conference);
        assert!(when.contains("Wednesday, 2 September 2026"));
        assert!(when.contains("Friday, 4 September 2026"));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.matches("Confere").count() >= 2);
    }

    #[test]
    fn formats_end_of_day_as_midnight_in_twelve_hour_mode() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let config = Config {
            time_format: "12h".into(),
            ..Config::default()
        };
        let app = App::new(
            Arc::new(MockBackend::demo(date)),
            config,
            None,
            date,
            Some(View::Week),
        );
        assert_eq!(format_minutes(&app, 24 * 60), "12:00a");
    }
}
