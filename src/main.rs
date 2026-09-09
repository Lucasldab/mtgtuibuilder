//! mtgtuibuilder -- a terminal Magic deck builder with Cardmarket prices.

mod app;
mod card;
mod commander;
mod deck;
mod decklist;
mod scryfall;
mod stats;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::execute;
use deck::Deck;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

const HELP: &str = "\
mtgtuibuilder -- terminal Magic deck builder

USAGE:
    mtgtuibuilder [DECKFILE] [OPTIONS]

ARGS:
    DECKFILE          Archidekt-format decklist to open or create

OPTIONS:
    -r, --refresh     Re-download the Scryfall bulk card data
    -h, --help        Show this help

Decks are plain text, round-trip with Archidekt and Moxfield:
    1x Sol Ring (ltc) 292 [Ramp]
";

fn main() -> Result<()> {
    let mut refresh = false;
    let mut path: Option<PathBuf> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-r" | "--refresh" => refresh = true,
            other if other.starts_with('-') => {
                eprintln!("unknown option: {other}");
                std::process::exit(2);
            }
            other => path = Some(PathBuf::from(other)),
        }
    }

    // Ingestion runs before the alternate screen so its progress is visible
    // and a failure leaves a readable terminal.
    let mut progress = |msg: &str| {
        println!("{msg}");
        let _ = io::stdout().flush();
    };
    let db = scryfall::load(refresh, &mut progress)?;
    println!("{} cards ready.", db.len());

    let deck = match &path {
        Some(p) if p.exists() => decklist::load(p)?,
        Some(p) => Deck { path: Some(p.clone()), ..Default::default() },
        None => Deck::default(),
    };

    let mut app = App::new(db, deck);
    if let Some(stamp) = scryfall::cache_stamp() {
        app.status = format!("prices from {}", &stamp[..stamp.len().min(10)]);
    }

    let mut terminal = setup()?;
    let result = run(&mut terminal, &mut app);
    restore(&mut terminal)?;
    result
}

fn setup() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll rather than block so a resize repaints promptly.
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                // Windows terminals emit both press and release.
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
        if app.quit {
            return Ok(());
        }
    }
}
