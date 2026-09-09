//! Rendering. Colours follow the purple palette used across the dotfiles.

use crate::app::{App, Mode, Row};
use crate::commander::Severity;
use crate::deck::Board;
use crate::stats::{CURVE_BUCKETS, pip_order};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

const ACCENT: Color = Color::Rgb(0x9A, 0x6D, 0xD7);
const ACCENT_DIM: Color = Color::Rgb(0x7A, 0x3A, 0xAF);
const MUTED: Color = Color::Rgb(0x8A, 0x8A, 0x8A);

pub fn draw(f: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(f.area());

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(outer[0]);

    draw_deck(f, app, panes[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Percentage(45)])
        .split(panes[1]);
    draw_stats(f, app, right[0]);
    draw_legality(f, app, right[1]);

    draw_status(f, app, outer[1]);

    match app.mode {
        Mode::Search => draw_search(f, app),
        Mode::Printing => draw_printing(f, app),
        Mode::Help => draw_help(f),
        _ => {}
    }
}

fn draw_deck(f: &mut Frame, app: &App, area: Rect) {
    let (label, count) = match app.board {
        Board::Main => ("Deck", app.deck.total_cards()),
        Board::Maybe => ("Maybeboard", app.deck.maybe.iter().map(|e| e.qty).sum()),
    };

    let list = app.deck.board(app.board);
    let items: Vec<ListItem> = app
        .rows
        .iter()
        .map(|row| match row {
            Row::Header(name, n) => ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{name} "),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("({n})"), Style::default().fg(MUTED)),
            ])),
            Row::Entry(i) => {
                let e = &list[*i];
                let card = app.db.get(&e.name);
                let price = card
                    .and_then(|c| c.printing(e.set.as_deref(), e.number.as_deref()))
                    .and_then(|p| p.eur);
                let set = e
                    .set
                    .as_ref()
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                let unknown = card.is_none();

                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {}x ", e.qty), Style::default().fg(MUTED)),
                    Span::styled(
                        e.name.clone(),
                        if unknown {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default()
                        },
                    ),
                    Span::styled(set, Style::default().fg(ACCENT_DIM)),
                    Span::raw("  "),
                    Span::styled(
                        match price {
                            Some(v) => format!("€{v:.2}"),
                            None => "—".into(),
                        },
                        Style::default().fg(MUTED),
                    ),
                ]))
            }
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.cursor));

    let title = format!(" {label} — {count} cards ");
    let widget = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT_DIM))
                .title(Span::styled(title, Style::default().fg(ACCENT))),
        )
        .highlight_style(Style::default().bg(ACCENT_DIM).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(widget, area, &mut state);
}

fn draw_stats(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.stats;
    let mut lines: Vec<Line> = Vec::new();

    // The commander fixes the deck's colour identity, so it leads the panel.
    if let Some(cmd) = app.deck.commander() {
        let identity = app
            .db
            .get(&cmd.name)
            .map(|c| c.color_identity.join(""))
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(
                truncate(&cmd.name, 24),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if identity.is_empty() { "  (colourless)".into() } else { format!("  {identity}") },
                Style::default().fg(MUTED),
            ),
        ]));
        lines.push(Line::from(""));
    }

    let (price, unpriced) = app.price;
    lines.push(Line::from(vec![
        Span::styled("Price  ", Style::default().fg(MUTED)),
        Span::styled(
            format!("€{price:.2}"),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if unpriced > 0 { format!("  ({unpriced} unpriced)") } else { String::new() },
            Style::default().fg(MUTED),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Lands  ", Style::default().fg(MUTED)),
        Span::raw(format!("{}", s.lands)),
        Span::styled("   Avg CMC  ", Style::default().fg(MUTED)),
        Span::raw(format!("{:.2}", s.avg_cmc)),
    ]));

    let pips = pip_order(&s.pips);
    if !pips.is_empty() {
        let mut spans = vec![Span::styled("Pips   ", Style::default().fg(MUTED))];
        for (c, n) in pips {
            spans.push(Span::styled(
                format!("{c}{n} "),
                Style::default().fg(pip_color(c)),
            ));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Curve",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )));
    let peak = s.curve.iter().copied().max().unwrap_or(1).max(1);
    for (i, n) in s.curve.iter().enumerate() {
        let label = if i == CURVE_BUCKETS - 1 { format!("{i}+") } else { i.to_string() };
        // Bar width is proportional to the tallest bucket, capped so the
        // panel never wraps.
        let width = (*n as usize * 18 / peak as usize).min(18);
        lines.push(Line::from(vec![
            Span::styled(format!("{label:>2} "), Style::default().fg(MUTED)),
            Span::styled("█".repeat(width), Style::default().fg(ACCENT_DIM)),
            Span::styled(format!(" {n}"), Style::default().fg(MUTED)),
        ]));
    }

    if !s.types.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Types",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        for (t, n) in &s.types {
            lines.push(Line::from(vec![
                Span::raw(format!("{t:<14}")),
                Span::styled(format!("{n}"), Style::default().fg(MUTED)),
            ]));
        }
    }

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT_DIM))
            .title(Span::styled(" Stats ", Style::default().fg(ACCENT))),
    );
    f.render_widget(widget, area);
}

fn pip_color(c: char) -> Color {
    match c {
        'W' => Color::Rgb(0xF0, 0xE8, 0xD0),
        'U' => Color::Rgb(0x6A, 0xA8, 0xE0),
        'B' => Color::Rgb(0xA0, 0x90, 0xB0),
        'R' => Color::Rgb(0xE0, 0x70, 0x60),
        'G' => Color::Rgb(0x70, 0xC0, 0x80),
        _ => MUTED,
    }
}

fn draw_legality(f: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = if app.issues.is_empty() {
        vec![Line::from(Span::styled(
            "Legal for Commander",
            Style::default().fg(Color::Green),
        ))]
    } else {
        app.issues
            .iter()
            .map(|i| {
                let (mark, color) = match i.severity {
                    Severity::Error => ("✗ ", Color::Red),
                    Severity::Warn => ("! ", Color::Yellow),
                };
                Line::from(vec![
                    Span::styled(mark, Style::default().fg(color)),
                    Span::raw(i.text.clone()),
                ])
            })
            .collect()
    };

    let errors = app
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let title = if errors == 0 {
        " Commander ".to_string()
    } else {
        format!(" Commander — {errors} problem{} ", if errors == 1 { "" } else { "s" })
    };

    let widget = Paragraph::new(lines).wrap(Wrap { trim: true }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT_DIM))
            .title(Span::styled(title, Style::default().fg(ACCENT))),
    );
    f.render_widget(widget, area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let line = match app.mode {
        Mode::Category => Line::from(vec![
            Span::styled("Category: ", Style::default().fg(ACCENT)),
            Span::raw(app.input.clone()),
            Span::styled("█", Style::default().fg(ACCENT)),
        ]),
        Mode::SaveAs => Line::from(vec![
            Span::styled("Save to: ", Style::default().fg(ACCENT)),
            Span::raw(app.input.clone()),
            Span::styled("█", Style::default().fg(ACCENT)),
        ]),
        _ => {
            let dirty = if app.deck.dirty { "*" } else { " " };
            let path = app
                .deck
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "[no file]".into());
            Line::from(vec![
                Span::styled(format!("{dirty}{path}  "), Style::default().fg(MUTED)),
                Span::raw(app.status.clone()),
                Span::styled(
                    "   ? help  / add  p printing  c category  s save  q quit",
                    Style::default().fg(MUTED),
                ),
            ])
        }
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_search(f: &mut Frame, app: &App) {
    let area = centered(70, 70, f.area());
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    let input = Paragraph::new(Line::from(vec![
        Span::raw(app.input.clone()),
        Span::styled("█", Style::default().fg(ACCENT)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT))
            .title(" Search "),
    );
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = app
        .results
        .iter()
        .map(|name| {
            let card = app.db.get(name);
            let price = card.and_then(|c| c.cheapest()).and_then(|p| p.eur);
            let type_line = card.map(|c| c.type_line.clone()).unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::raw(format!("{name:<34}")),
                Span::styled(format!("{type_line:<28}"), Style::default().fg(MUTED)),
                Span::styled(
                    match price {
                        Some(v) => format!("€{v:.2}"),
                        None => "—".into(),
                    },
                    Style::default().fg(ACCENT_DIM),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.result_cursor));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT_DIM))
                .title(format!(" {} matches — Enter adds, Esc cancels ", app.results.len())),
        )
        .highlight_style(Style::default().bg(ACCENT_DIM).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, chunks[1], &mut state);
}

fn draw_printing(f: &mut Frame, app: &App) {
    let Some(idx) = app.selected_entry() else { return };
    let entry = &app.deck.board(app.board)[idx];
    let Some(card) = app.db.get(&entry.name) else { return };

    let area = centered(60, 60, f.area());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = card
        .printings
        .iter()
        .map(|p| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<6}", p.set.to_uppercase()), Style::default().fg(ACCENT_DIM)),
                Span::raw(format!("{:<32}", truncate(&p.set_name, 31))),
                Span::styled(format!("{:>6}  ", p.number), Style::default().fg(MUTED)),
                Span::styled(
                    match p.eur {
                        Some(v) => format!("€{v:.2}"),
                        None => "—".into(),
                    },
                    Style::default().fg(ACCENT),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.printing_cursor));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(format!(" {} — Enter picks, 0 resets to cheapest ", truncate(&card.name, 40))),
        )
        .highlight_style(Style::default().bg(ACCENT_DIM).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_help(f: &mut Frame) {
    let area = centered(56, 70, f.area());
    f.render_widget(Clear, area);
    let rows = [
        ("j / k", "move"),
        ("g / G", "top / bottom"),
        ("Tab", "switch deck / maybeboard"),
        ("/ or a", "search and add a card"),
        ("+ / -", "change quantity"),
        ("d", "remove card"),
        ("m", "move to the other board"),
        ("c", "set category"),
        ("C", "set as commander"),
        ("p", "choose printing"),
        ("s / S", "save / save as"),
        ("?", "this help"),
        ("q", "quit"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("  {k:<9}"), Style::default().fg(ACCENT)),
                Span::raw(*d),
            ])
        })
        .collect();
    let widget = Paragraph::new(lines).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT))
            .title(" Keys — any key closes "),
    );
    f.render_widget(widget, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn centered(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(v[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::card::{Card, Printing};
    use crate::decklist;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn db() -> crate::card::CardDb {
        let mk = |name: &str, cost: &str, cmc: f32, tl: &str, id: &[&str], eur: f64| Card {
            name: name.into(),
            mana_cost: cost.into(),
            cmc,
            type_line: tl.into(),
            oracle_text: String::new(),
            color_identity: id.iter().map(|s| s.to_string()).collect(),
            commander_legal: true,
            printings: vec![Printing {
                set: "tst".into(),
                set_name: "Test Set".into(),
                number: "1".into(),
                eur: Some(eur),
            }],
        };
        crate::card::CardDb::new(vec![
            mk("Atraxa, Grand Unifier", "{4}{W}{B}", 7.0, "Legendary Creature — Angel", &["W","U","B","G"], 25.0),
            mk("Sol Ring", "{1}", 1.0, "Artifact", &[], 1.50),
            mk("Forest", "", 0.0, "Basic Land — Forest", &["G"], 0.10),
        ])
    }

    /// Renders the whole frame to an in-memory buffer -- the only way to
    /// exercise the UI without a TTY.
    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_deck_with_prices_and_stats() {
        let deck = decklist::parse(
            "1x Atraxa, Grand Unifier [Commander]\n1x Sol Ring [Ramp]\n2x Forest [Lands]\n",
        );
        let mut app = App::new(db(), deck);
        let out = render(&mut app, 120, 40);

        assert!(out.contains("Sol Ring"), "deck list missing:\n{out}");
        assert!(out.contains("Commander"), "category header missing");
        assert!(out.contains("Ramp"), "category header missing");
        assert!(out.contains("€1.50"), "per-card price missing");
        // 25.00 + 1.50 + 2x0.10 = 26.70
        assert!(out.contains("€26.70"), "deck total missing:\n{out}");
        assert!(out.contains("Curve"), "stats panel missing");
    }

    #[test]
    fn renders_legality_problems() {
        let deck = decklist::parse("2x Sol Ring [Ramp]\n");
        let mut app = App::new(db(), deck);
        let out = render(&mut app, 120, 40);
        assert!(out.contains("singleton"), "singleton breach not shown:\n{out}");
    }

    #[test]
    fn renders_search_overlay() {
        let mut app = App::new(db(), decklist::parse(""));
        app.on_key(key(KeyCode::Char('/')));
        app.on_key(key(KeyCode::Char('s')));
        app.on_key(key(KeyCode::Char('o')));
        let out = render(&mut app, 120, 40);
        assert!(out.contains("Search"), "search overlay missing:\n{out}");
        assert!(out.contains("Sol Ring"), "search result missing:\n{out}");
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = App::new(db(), decklist::parse(""));
        app.on_key(key(KeyCode::Char('?')));
        let out = render(&mut app, 120, 40);
        assert!(out.contains("Keys"), "help overlay missing");
    }

    #[test]
    fn survives_a_tiny_terminal() {
        // Panicking on a small window would take the whole app down.
        let deck = decklist::parse("1x Sol Ring [Ramp]\n");
        let mut app = App::new(db(), deck);
        for (w, h) in [(20u16, 6u16), (40, 10), (200, 60)] {
            let _ = render(&mut app, w, h);
        }
    }

    use crossterm::event::{KeyCode, KeyEvent};
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }
}

#[cfg(test)]
mod real_render {
    use super::tests_support::*;

    /// Renders a real decklist against the downloaded card data. Skipped when
    /// no cache exists so a clean checkout still passes `cargo test`.
    #[test]
    fn renders_a_real_commander_deck() {
        let Some(db) = real_db() else { return };
        let text = include_str!("../examples/atraxa.txt");
        let deck = crate::decklist::parse(text);
        let mut app = crate::app::App::new(db, deck);

        assert_eq!(app.deck.total_cards(), 34);
        assert_eq!(app.deck.maybe.len(), 2);
        assert!(app.price.0 > 0.0, "deck priced at zero");
        assert_eq!(app.price.1, 0, "some cards had no Cardmarket price");

        let out = render_to_string(&mut app, 118, 34);
        if std::env::var("SHOW_UI").is_ok() {
            println!("{out}");
            println!("total €{:.2}", app.price.0);
        }
        assert!(out.contains("Atraxa"));
        assert!(out.contains("Removal"));
    }
}

#[cfg(test)]
mod tests_support {
    use crate::app::App;
    use crate::card::CardDb;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    pub fn real_db() -> Option<CardDb> {
        crate::scryfall::load(false, &mut |_| {}).ok()
    }

    pub fn render_to_string(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| super::draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }
}
