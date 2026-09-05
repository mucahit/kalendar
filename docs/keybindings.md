# Keyboard shortcuts

## Normal mode

| Key | Action |
| --- | --- |
| `1` / `a` | Agenda view |
| `2` / `w` | Week view |
| `3` / `m` | Month view |
| `h` / `←`, `l` / `→` | Previous / next day |
| `j` / `↓`, `k` / `↑` | Move time, month row, or agenda event |
| `H`, `L` | Previous / next week |
| `PageUp`, `PageDown` | Previous / next month |
| `Ctrl+j`, `Ctrl+k` | Scroll visible time range |
| `Tab`, `Shift+Tab` | Cycle selectable events |
| `t` | Today |
| `Enter` | Open selected event |
| `o` | Join the selected meeting, when available |
| `n` | New event |
| `e` | Edit selected event |
| `d` | Delete selected event |
| `c` | Calendar picker |
| `/` | Search |
| `r` | Force refresh |
| `?` | Help |
| `q` / `Ctrl+c` | Quit |
| `Ctrl+z` | Suspend and safely restore after resume |

## Event details

When an event has a meeting URL, press `o` from the calendar or its event details to join it in your
default browser. Kalendar recognizes meeting links in the event URL, location, and description
fields. Use `e` to edit, `d` to delete, and `Esc` or `Enter` to close the details.

## Event editor

`Tab` and `Shift+Tab` change fields. On Calendar and All day, use arrows; Space also toggles All day. Text fields accept direct input and Backspace. `Ctrl+s` validates and saves; `Esc` cancels.

Dates use `YYYY-MM-DD`; timed fields use 24-hour `HH:MM` even when display format is configured as 12-hour. An end time earlier than the start time is treated as the following day, allowing overnight events.

## Calendar picker

Use arrows or `j`/`k`, Space to toggle, Enter to save, and Esc to cancel all unsaved visibility changes.

## Search

Search updates as text is entered. Use arrows to choose a result and Enter to jump to it in week view. Esc returns without changing the current date.

## Deletion

For a single event, `y` confirms. For a recurring occurrence, `1`/`y` deletes only that occurrence and `2` deletes it and future occurrences. EventKit cannot safely infer occurrences before an arbitrary selected instance, so Kalendar does not offer a misleading “all occurrences” action.

Saving an edit to a recurring event presents the same `1`/`2` scope choice before changing EventKit.
