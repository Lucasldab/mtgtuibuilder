//! mtgtuibuilder -- a terminal Magic deck builder with Cardmarket prices.

mod app;
mod card;
mod commander;
mod deck;
mod decklist;
mod images;
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
use ratatui_image::FontSize;
use ratatui_image::picker::{Picker, ProtocolType};
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
        --doctor      Report what the preview will do in this terminal

ENV:
    MTGTUI_IMAGE_PROTOCOL   force kitty|sixel|iterm2|halfblocks for previews

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
            "--doctor" => {
                doctor();
                return Ok(());
            }
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
    app.picker = Some(build_picker());
    if let Some(stamp) = scryfall::cache_stamp() {
        app.status = format!("prices from {}", &stamp[..stamp.len().min(10)]);
    }

    let mut terminal = setup()?;
    let result = run(&mut terminal, &mut app);
    restore(&mut terminal)?;
    result
}

/// Builds the image picker without querying the terminal over stdin.
///
/// ratatui-image's `from_query_stdio` spawns a thread that enables raw mode,
/// reads a reply from stdin, then disables raw mode. If the terminal never
/// answers -- a bare TTY, a detached multiplexer, anything piped -- the call
/// times out but that thread keeps running: it holds stdin so the TUI receives
/// no keys, and it later disables the raw mode the TUI had switched on. The
/// font size comes from an ioctl instead and the protocol from the
/// environment, so nothing ever reads stdin behind the event loop's back.
fn build_picker() -> Picker {
    let (font_size, _source) = font_size();

    // Deprecated upstream in favour of `from_query_stdio`, which is precisely
    // the function whose orphaned thread breaks the event loop. `halfblocks`,
    // the other suggested replacement, would give up graphics entirely.
    #[allow(deprecated)]
    let mut picker = Picker::from_fontsize(font_size);

    // Escape hatch for wrong detection, and for forcing halfblocks to see the
    // image as text.
    if let Ok(forced) = std::env::var("MTGTUI_IMAGE_PROTOCOL") {
        let forced = match forced.to_lowercase().as_str() {
            "kitty" => Some(ProtocolType::Kitty),
            "sixel" => Some(ProtocolType::Sixel),
            "iterm2" => Some(ProtocolType::Iterm2),
            "halfblocks" => Some(ProtocolType::Halfblocks),
            _ => None,
        };
        if let Some(p) = forced {
            picker.set_protocol_type(p);
            return picker;
        }
    }

    // from_fontsize only guesses iTerm2 from the environment, so kitty -- the
    // one we can identify reliably without a query -- is filled in here.
    if matches!(picker.protocol_type(), ProtocolType::Halfblocks) && kitty_from_env() {
        picker.set_protocol_type(ProtocolType::Kitty);
    }
    picker
}

/// Cell size in pixels, and where the number came from.
///
/// This is not cosmetic: with kitty's unicode placeholders the image is
/// transmitted sized to a cell grid derived from it, so a wrong font size
/// makes the image span the wrong number of cells and land outside the
/// placeholders that are supposed to show it -- which looks like no image
/// at all rather than a badly scaled one.
fn font_size() -> (FontSize, &'static str) {
    if let Some(size) = crossterm::terminal::window_size()
        .ok()
        .filter(|w| w.width > 0 && w.height > 0 && w.columns > 0 && w.rows > 0)
        .map(|w| (w.width / w.columns, w.height / w.rows))
    {
        return (size, "ioctl");
    }

    // Terminals under tmux report no pixel size through the ioctl, but tmux
    // has queried the outer terminal itself and will hand over the answer.
    if std::env::var_os("TMUX").is_some() {
        if let Some(size) = tmux_cell_size() {
            return (size, "tmux");
        }
    }

    ((8, 16), "fallback")
}

fn tmux_cell_size() -> Option<FontSize> {
    let out = std::process::Command::new("tmux")
        .args(["display-message", "-p", "#{client_cell_width}x#{client_cell_height}"])
        .output()
        .ok()?;
    parse_cell_size(&String::from_utf8_lossy(&out.stdout))
}

fn parse_cell_size(s: &str) -> Option<FontSize> {
    let (w, h) = s.trim().split_once('x')?;
    let w: u16 = w.trim().parse().ok()?;
    let h: u16 = h.trim().parse().ok()?;
    // tmux reports zeroes when it has no answer from the outer terminal.
    (w > 0 && h > 0).then_some((w, h))
}

fn kitty_from_env() -> bool {
    std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var("TERM").is_ok_and(|t| t.contains("kitty"))
}

/// Reports everything that decides whether a preview can render. Printed
/// rather than guessed at, because the failure mode -- an empty pane -- looks
/// identical no matter which stage broke.
fn doctor() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| "<unset>".into());

    println!("terminal");
    println!("  TERM                  {}", env("TERM"));
    println!("  TERM_PROGRAM          {}", env("TERM_PROGRAM"));
    println!("  KITTY_WINDOW_ID       {}", env("KITTY_WINDOW_ID"));
    println!("  TMUX                  {}", env("TMUX"));
    println!("  MTGTUI_IMAGE_PROTOCOL {}", env("MTGTUI_IMAGE_PROTOCOL"));

    println!("\nwindow size (ioctl)");
    match crossterm::terminal::window_size() {
        Ok(w) => {
            println!("  cells                 {}x{}", w.columns, w.rows);
            println!("  pixels                {}x{}", w.width, w.height);
            if w.width == 0 || w.height == 0 {
                println!("  note                  no pixel size reported; font size falls back to 8x16");
            }
        }
        Err(e) => println!("  failed                {e}"),
    }

    let (_, source) = font_size();
    let picker = build_picker();
    println!("\npreview");
    println!("  font size             {:?} (from {source})", picker.font_size());
    println!("  protocol              {:?}", picker.protocol_type());
    if matches!(picker.protocol_type(), ProtocolType::Halfblocks) {
        println!("  note                  halfblocks renders as coloured text, not a real image");
    }

    let dir = images::image_dir();
    let count = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    println!("\ncache");
    println!("  cards                 {}", scryfall::cache_stamp().unwrap_or_else(|| "<none>".into()));
    println!("  images                {count} files in {}", dir.display());
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
        app.tick_images();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tmux_cell_size() {
        assert_eq!(parse_cell_size("12x27\n"), Some((12, 27)));
        assert_eq!(parse_cell_size(" 8x16 "), Some((8, 16)));
    }

    #[test]
    fn rejects_tmux_zeroes() {
        // tmux prints zeroes when the outer terminal never answered.
        assert_eq!(parse_cell_size("0x0"), None);
        assert_eq!(parse_cell_size("12x0"), None);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_cell_size(""), None);
        assert_eq!(parse_cell_size("12"), None);
        assert_eq!(parse_cell_size("axb"), None);
    }
}
