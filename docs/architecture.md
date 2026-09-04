# Architecture

Kalendar keeps Apple-specific code at the outer edge:

```text
crossterm input → App transition → CalendarBackend → App state → ratatui renderer
                                      │
                         MockBackend or MacOsBackend
                                      │
                                  JSON Lines
                                      │
                              Swift EventKit helper
```

## Workspace

- `kalendar-core` owns calendar/event models, date helpers, the async backend contract, and the deterministic mutable demo backend.
- `kalendar-tui` owns state transitions, configuration, input, terminal lifecycle, geometry, and rendering. It imports no EventKit types.
- `kalendar-macos` translates backend calls to correlated JSON requests over a persistent helper process. Process isolation keeps Objective-C/Swift concerns out of Rust and makes bridge failures recoverable.
- `kalendar` chooses a backend, parses command-line options, configures file logging, and launches the UI.
- `native/macos-calendar-bridge` is a presentation-free Swift executable using Apple's supported EventKit API.

## State and data loading

`App` is the single owner of view, mode, cursor, selection, calendars, visibility, events, editor, and search state. Normal-mode input maps to typed `Action` values; the reducer applies synchronous state changes and emits typed `Command` values for backend-loading transitions. Modal handlers use the same explicit state methods, and rendering is read-only. Backend ranges are selected per view:

- Week: visible week plus seven days on each side.
- Month: the complete six-week grid.
- Agenda: selected day through the following 14 days.

Results are cached in memory by timestamp range. Exact hits and containing cached ranges render without another backend call; a week can also reuse the adjacent data loaded as padding. Explicit refresh and every mutation invalidate the cache.

## Time and overlap geometry

Time projection and overlap allocation are independent of Ratatui. Events are sorted, transitively overlapping intervals are grouped, and each event is assigned the first free horizontal slot. A group uses its peak slot count for stable widths. Projection clips events outside configured hours and guarantees at least one row for a visible short event.

All local-time construction goes through `local_at`, which handles ambiguous and nonexistent wall-clock times around daylight-saving transitions without panicking.

## Native permissions

The helper reports `granted`, `not_determined`, or `denied`. Kalendar requests full access only for `not_determined`; denied access becomes an actionable in-TUI error. The helper embeds the required Calendar usage descriptions in its Mach-O information property-list section.

## Failure and terminal safety

The alternate screen and raw mode are owned by a drop guard. Both ordinary errors and panics restore raw mode, leave the alternate screen, and show the cursor. User-facing backend and validation failures render as overlays; diagnostic details are written outside the terminal UI to the log file.
