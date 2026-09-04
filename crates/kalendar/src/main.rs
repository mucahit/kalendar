use anyhow::{Context, Result, anyhow};
use chrono::{Local, NaiveDate};
use kalendar_core::{CalendarBackend, MockBackend, PermissionStatus};
use kalendar_macos::MacOsBackend;
use kalendar_tui::{App, Config, View};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::filter::LevelFilter;

struct Options {
    demo: bool,
    debug: bool,
    doctor: bool,
    date: NaiveDate,
    view: Option<View>,
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = try_main().await {
        eprintln!("kalendar: {error:#}");
        std::process::exit(1);
    }
}

async fn try_main() -> Result<()> {
    let options = parse_options()?;
    if options.doctor {
        return doctor().await;
    }
    init_logging(options.debug)?;
    let config = Config::load(options.config.as_deref())?;
    let backend: Arc<dyn CalendarBackend> = if options.demo {
        Arc::new(MockBackend::demo(options.date))
    } else {
        Arc::new(MacOsBackend::discover()?)
    };
    let mut app = App::new(backend, config, options.config, options.date, options.view);
    if let Err(error) = app.initialize().await {
        app.set_error(error);
    }
    kalendar_tui::run(app).await
}

fn parse_options() -> Result<Options> {
    let mut demo = false;
    let mut debug = false;
    let mut doctor = false;
    let mut date = Local::now().date_naive();
    let mut view = None;
    let mut config = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--demo" => demo = true,
            "--debug" => debug = true,
            "doctor" => doctor = true,
            "--date" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow!("--date requires YYYY-MM-DD"))?;
                date = NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                    .map_err(|_| anyhow!("invalid date `{value}`; expected YYYY-MM-DD"))?;
            }
            "--view" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow!("--view requires agenda, week, or month"))?;
                view = Some(match value.as_str() {
                    "agenda" => View::Agenda,
                    "week" => View::Week,
                    "month" => View::Month,
                    _ => {
                        return Err(anyhow!(
                            "invalid view `{value}`; expected agenda, week, or month"
                        ));
                    }
                });
            }
            "--config" => {
                config = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| anyhow!("--config requires a path"))?,
                ));
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("kalendar {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => return Err(anyhow!("unknown argument `{argument}`; use --help")),
        }
    }
    Ok(Options {
        demo,
        debug,
        doctor,
        date,
        view,
        config,
    })
}

fn print_help() {
    println!(
        "kalendar — a keyboard-first terminal calendar\n\nUSAGE:\n    kalendar [--demo] [--view VIEW] [--date YYYY-MM-DD] [--debug]\n    kalendar doctor\n\nOPTIONS:\n    --demo         use deterministic sample data\n    --view VIEW    agenda, week, or month\n    --date DATE    initial date in YYYY-MM-DD format\n    --config PATH  use a specific TOML configuration\n    --debug        enable detailed logs\n    -h, --help     show this help\n    -V, --version  show the version"
    );
}

fn init_logging(debug: bool) -> Result<()> {
    let directory = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Logs/kalendar");
    fs::create_dir_all(&directory)
        .with_context(|| format!("creating log directory {}", directory.display()))?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("kalendar.log"))
        .context("opening log file")?;
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(debug)
        .with_max_level(if debug {
            LevelFilter::DEBUG
        } else {
            LevelFilter::INFO
        })
        .with_writer(Mutex::new(file))
        .try_init();
    Ok(())
}

async fn doctor() -> Result<()> {
    println!("kalendar {}", env!("CARGO_PKG_VERSION"));
    println!("configuration: {}", Config::path().display());
    let backend = MacOsBackend::discover()?;
    let permission = backend.permissions().await?;
    println!("EventKit bridge: ok");
    println!(
        "Calendar permission: {}",
        match permission {
            PermissionStatus::Granted => "granted",
            PermissionStatus::NotDetermined => "not determined",
            PermissionStatus::Denied => "denied",
        }
    );
    if permission == PermissionStatus::Granted {
        println!("calendars: {}", backend.calendars().await?.len());
    }
    Ok(())
}
