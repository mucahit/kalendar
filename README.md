# Kalendar

Kalendar is a fast, keyboard-first terminal calendar for macOS. It reads and writes the calendars already configured in Apple Calendar through a small native EventKit helper; the TUI and domain model stay backend-independent.

## Status

Version 0.1 includes week, month, and 14-day agenda views; timed and all-day events; overlap layout; current-time and calendar colors; event details; create/edit/delete; safe recurring-event update and deletion scopes supported by EventKit; calendar filtering; search; configuration; responsive terminal handling; and a deterministic demo backend.

## Requirements

- macOS 13 or newer
- Rust 1.88 or newer, with Cargo
- Xcode Command Line Tools or Xcode (for Swift/EventKit)
- A terminal at least 80×24

## Run

Try the complete UI without touching your calendars:

```bash
cargo run -- --demo
```

Use Apple Calendar data:

```bash
cargo run
```

The first build compiles both the Rust application and native Swift helper. On first real-calendar launch, macOS asks for Calendar access. Kalendar only asks while authorization is undecided; if access was denied, enable it in **System Settings → Privacy & Security → Calendars**.

Useful command-line options:

```text
kalendar --demo
kalendar --view week|month|agenda
kalendar --date 2026-09-04
kalendar --config /path/to/config.toml
kalendar --debug
kalendar doctor
```

## Install from source

```bash
make install PREFIX=/usr/local
```

This installs `kalendar` into `PREFIX/bin` and `kalendar-eventkit` into `PREFIX/libexec/kalendar`. To build a distributable archive instead:

```bash
./scripts/package-release.sh aarch64-apple-darwin
./scripts/package-release.sh x86_64-apple-darwin
```

Pushing a `v0.1.0` tag runs the release workflow. Set the repository variable `HOMEBREW_TAP_REPOSITORY` and secret `HOMEBREW_TAP_TOKEN` if the workflow should also update a tap.

## Controls

The footer shows contextual shortcuts, and `?` opens the complete help overlay.

| Key | Action |
| --- | --- |
| `1` / `a`, `2` / `w`, `3` / `m` | Agenda, week, month |
| `h j k l` or arrows | Navigate |
| `H` / `L` | Previous / next week |
| `PageUp` / `PageDown` | Previous / next month |
| `t` | Today |
| `Ctrl+j` / `Ctrl+k` | Scroll visible hours |
| `Enter` | Event details |
| `n`, `e`, `d` | New, edit, delete |
| `c` | Calendar visibility |
| `/` | Search |
| `r` | Refresh |
| `q` | Quit |

See [docs/keybindings.md](docs/keybindings.md) for mode-specific controls.

## Configuration

Kalendar reads `~/.config/kalendar/config.toml` by default:

```toml
default_view = "week"
week_starts_on = "monday"
time_format = "24h"
date_format = "%d %b"
day_start = "08:00"
day_end = "20:00"
default_event_duration_minutes = 60
default_event_start = "09:00"
show_weekends = true
show_current_time = true
hidden_calendars = []

[theme]
today = "cyan"
selected = "blue"
muted = "dark_gray"
border = "dark_gray"
```

Calendar visibility changes made with `c` are saved automatically. Logs go to `~/Library/Logs/kalendar/kalendar.log`; `--debug` adds diagnostic detail.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The architecture and native wire format are documented in [docs/architecture.md](docs/architecture.md) and [docs/eventkit-protocol.md](docs/eventkit-protocol.md).
## License

MIT
