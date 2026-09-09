//! Application state and key handling.

use crate::card::CardDb;
use crate::commander::{self, Issue};
use crate::deck::{Board, COMMANDER, Deck, Entry};
use crate::decklist;
use crate::stats::{self, Stats};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Deck,
    Search,
    Printing,
    Category,
    SaveAs,
    Help,
}

/// A rendered line in the deck pane. Headers are not selectable.
#[derive(Debug, Clone)]
pub enum Row {
    Header(String, u32),
    Entry(usize),
}

pub struct App {
    pub db: CardDb,
    pub deck: Deck,
    pub mode: Mode,
    pub board: Board,
    pub rows: Vec<Row>,
    pub cursor: usize,
    /// Shared by the search, category and save-as prompts.
    pub input: String,
    pub results: Vec<String>,
    pub result_cursor: usize,
    pub printing_cursor: usize,
    pub status: String,
    pub issues: Vec<Issue>,
    pub stats: Stats,
    pub price: (f64, usize),
    pub quit: bool,
    /// Set after a quit attempt with unsaved changes, cleared by any other key.
    confirm_quit: bool,
}

impl App {
    pub fn new(db: CardDb, deck: Deck) -> Self {
        let mut app = Self {
            db,
            deck,
            mode: Mode::Deck,
            board: Board::Main,
            rows: Vec::new(),
            cursor: 0,
            input: String::new(),
            results: Vec::new(),
            result_cursor: 0,
            printing_cursor: 0,
            status: String::new(),
            issues: Vec::new(),
            stats: Stats::default(),
            price: (0.0, 0),
            quit: false,
            confirm_quit: false,
        };
        app.refresh();
        app
    }

    /// Recomputes everything derived from the deck. Cheap at Commander sizes,
    /// so it runs after any mutation rather than being invalidated piecemeal.
    pub fn refresh(&mut self) {
        self.rows = self.build_rows();
        self.issues = commander::validate(&self.deck, &self.db);
        self.stats = stats::compute(&self.deck, &self.db);
        self.price = self.deck.price(&self.db);
        let max = self.rows.len().saturating_sub(1);
        if self.cursor > max {
            self.cursor = max;
        }
        if matches!(self.rows.first(), Some(Row::Header(..))) && self.cursor == 0 {
            self.move_cursor(1);
        }
    }

    fn build_rows(&self) -> Vec<Row> {
        let list = self.deck.board(self.board);
        let mut rows = Vec::new();
        for cat in self.deck.categories(self.board) {
            let idxs: Vec<usize> = list
                .iter()
                .enumerate()
                .filter(|(_, e)| e.category_or_default() == cat)
                .map(|(i, _)| i)
                .collect();
            let count: u32 = idxs.iter().map(|i| list[*i].qty).sum();
            rows.push(Row::Header(cat, count));
            rows.extend(idxs.into_iter().map(Row::Entry));
        }
        rows
    }

    pub fn selected_entry(&self) -> Option<usize> {
        match self.rows.get(self.cursor) {
            Some(Row::Entry(i)) => Some(*i),
            _ => None,
        }
    }

    /// Moves by `delta` rows, skipping headers so the cursor always lands on a
    /// card.
    pub fn move_cursor(&mut self, delta: i32) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as i32;
        let mut pos = self.cursor as i32;
        for _ in 0..len {
            pos = (pos + delta).clamp(0, len - 1);
            if matches!(self.rows[pos as usize], Row::Entry(_)) {
                self.cursor = pos as usize;
                return;
            }
            if pos == 0 || pos == len - 1 {
                // Nothing selectable in this direction; stay put.
                if delta > 0 && pos == len - 1 {
                    break;
                }
                if delta < 0 && pos == 0 {
                    break;
                }
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code != KeyCode::Char('q') {
            self.confirm_quit = false;
        }
        match self.mode {
            Mode::Deck => self.key_deck(key),
            Mode::Search => self.key_search(key),
            Mode::Printing => self.key_printing(key),
            Mode::Category | Mode::SaveAs => self.key_input(key),
            Mode::Help => {
                self.mode = Mode::Deck;
            }
        }
    }

    fn key_deck(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => {
                if self.deck.dirty && !self.confirm_quit {
                    self.confirm_quit = true;
                    self.status = "Unsaved changes -- press q again to quit".into();
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('g') | KeyCode::Home => {
                self.cursor = 0;
                self.move_cursor(1);
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.cursor = self.rows.len().saturating_sub(1);
                self.move_cursor(-1);
            }
            KeyCode::Tab => {
                self.board = match self.board {
                    Board::Main => Board::Maybe,
                    Board::Maybe => Board::Main,
                };
                self.cursor = 0;
                self.refresh();
            }
            KeyCode::Char('/') | KeyCode::Char('a') => {
                self.mode = Mode::Search;
                self.input.clear();
                self.results.clear();
                self.result_cursor = 0;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if let Some(i) = self.selected_entry() {
                    self.deck.bump(self.board, i, 1);
                    self.refresh();
                }
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                if let Some(i) = self.selected_entry() {
                    self.deck.bump(self.board, i, -1);
                    self.refresh();
                }
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                if let Some(i) = self.selected_entry() {
                    self.deck.remove(self.board, i);
                    self.refresh();
                }
            }
            KeyCode::Char('m') => {
                if let Some(i) = self.selected_entry() {
                    self.deck.shift(self.board, i);
                    self.refresh();
                    self.status = "Moved".into();
                }
            }
            KeyCode::Char('c') => {
                if let Some(i) = self.selected_entry() {
                    self.mode = Mode::Category;
                    self.input = self.deck.board(self.board)[i]
                        .category
                        .clone()
                        .unwrap_or_default();
                }
            }
            KeyCode::Char('C') => {
                if let Some(i) = self.selected_entry() {
                    self.deck.board_mut(self.board)[i].category = Some(COMMANDER.into());
                    self.deck.dirty = true;
                    self.refresh();
                    self.status = "Set as commander".into();
                }
            }
            KeyCode::Char('p') => {
                if self.selected_entry().is_some() {
                    self.mode = Mode::Printing;
                    self.printing_cursor = 0;
                }
            }
            KeyCode::Char('s') => self.save(),
            KeyCode::Char('S') => {
                self.mode = Mode::SaveAs;
                self.input = self
                    .deck
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    fn key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Deck;
                self.input.clear();
            }
            KeyCode::Enter => {
                if let Some(name) = self.results.get(self.result_cursor).cloned() {
                    self.deck.add(self.board, Entry::new(name.clone()));
                    self.refresh();
                    self.status = format!("Added {name}");
                }
            }
            KeyCode::Down => {
                if self.result_cursor + 1 < self.results.len() {
                    self.result_cursor += 1;
                }
            }
            KeyCode::Up => self.result_cursor = self.result_cursor.saturating_sub(1),
            KeyCode::Backspace => {
                self.input.pop();
                self.run_search();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
                self.run_search();
            }
            _ => {}
        }
    }

    fn run_search(&mut self) {
        self.results = self
            .db
            .search(&self.input, 200)
            .into_iter()
            .map(|c| c.name.clone())
            .collect();
        self.result_cursor = 0;
    }

    fn key_printing(&mut self, key: KeyEvent) {
        let Some(idx) = self.selected_entry() else {
            self.mode = Mode::Deck;
            return;
        };
        let name = self.deck.board(self.board)[idx].name.clone();
        let count = self.db.get(&name).map(|c| c.printings.len()).unwrap_or(0);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.mode = Mode::Deck,
            KeyCode::Down | KeyCode::Char('j') => {
                if self.printing_cursor + 1 < count {
                    self.printing_cursor += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.printing_cursor = self.printing_cursor.saturating_sub(1)
            }
            KeyCode::Char('0') => {
                // Back to "cheapest today" rather than a pinned printing.
                let e = &mut self.deck.board_mut(self.board)[idx];
                e.set = None;
                e.number = None;
                self.deck.dirty = true;
                self.mode = Mode::Deck;
                self.refresh();
                self.status = "Printing reset to cheapest".into();
            }
            KeyCode::Enter => {
                if let Some(p) = self
                    .db
                    .get(&name)
                    .and_then(|c| c.printings.get(self.printing_cursor))
                {
                    let (set, number) = (p.set.clone(), p.number.clone());
                    let e = &mut self.deck.board_mut(self.board)[idx];
                    e.set = Some(set);
                    e.number = Some(number);
                    self.deck.dirty = true;
                    self.mode = Mode::Deck;
                    self.refresh();
                    self.status = "Printing set".into();
                }
            }
            _ => {}
        }
    }

    fn key_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Deck;
                self.input.clear();
            }
            KeyCode::Enter => {
                let value = self.input.trim().to_string();
                match self.mode {
                    Mode::Category => {
                        if let Some(i) = self.selected_entry() {
                            self.deck.board_mut(self.board)[i].category =
                                (!value.is_empty()).then_some(value);
                            self.deck.dirty = true;
                            self.refresh();
                        }
                    }
                    Mode::SaveAs => {
                        if !value.is_empty() {
                            self.deck.path = Some(PathBuf::from(shell_expand(&value)));
                            self.save();
                        }
                    }
                    _ => {}
                }
                self.mode = Mode::Deck;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn save(&mut self) {
        let Some(path) = self.deck.path.clone() else {
            self.mode = Mode::SaveAs;
            self.input.clear();
            self.status = "No path set -- enter one".into();
            return;
        };
        match decklist::save(&self.deck, &path) {
            Ok(()) => {
                self.deck.dirty = false;
                self.status = format!("Saved {}", path.display());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }
}

/// Minimal `~` expansion so the save-as prompt accepts a home-relative path.
fn shell_expand(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Card, Printing};
    use crate::decklist;
    use crossterm::event::KeyCode;

    fn db() -> CardDb {
        let mk = |name: &str, tl: &str, prints: Vec<(&str, &str, f64)>| Card {
            name: name.into(),
            mana_cost: String::new(),
            cmc: 1.0,
            type_line: tl.into(),
            oracle_text: String::new(),
            color_identity: Vec::new(),
            commander_legal: true,
            printings: prints
                .into_iter()
                .map(|(set, number, eur)| Printing {
                    set: set.into(),
                    set_name: format!("{} set", set.to_uppercase()),
                    number: number.into(),
                    eur: Some(eur),
                })
                .collect(),
        };
        CardDb::new(vec![
            mk("Sol Ring", "Artifact", vec![("ltc", "292", 1.50), ("c21", "263", 2.75)]),
            mk("Solemn Simulacrum", "Artifact Creature", vec![("m21", "234", 0.60)]),
            mk("Forest", "Basic Land — Forest", vec![("blb", "280", 0.10)]),
        ])
    }

    fn app_with(text: &str) -> App {
        App::new(db(), decklist::parse(text))
    }

    fn press(app: &mut App, c: char) {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }

    #[test]
    fn cursor_skips_category_headers() {
        let mut app = app_with("1x Sol Ring [Ramp]\n1x Forest [Lands]\n");
        // Row 0 is a header, so the initial cursor must already be on a card.
        assert!(app.selected_entry().is_some(), "cursor parked on a header");
        press(&mut app, 'j');
        assert!(app.selected_entry().is_some());
    }

    #[test]
    fn search_then_enter_adds_the_card() {
        let mut app = app_with("");
        press(&mut app, '/');
        for c in "sol ".chars() {
            press(&mut app, c);
        }
        assert!(!app.results.is_empty(), "no search results");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.deck.main.len(), 1);
        assert_eq!(app.deck.main[0].name, "Sol Ring");
    }

    #[test]
    fn search_ranks_exact_match_first() {
        let mut app = app_with("");
        press(&mut app, '/');
        for c in "sol ring".chars() {
            press(&mut app, c);
        }
        assert_eq!(app.results[0], "Sol Ring");
    }

    #[test]
    fn quantity_keys_adjust_and_remove() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, '+');
        assert_eq!(app.deck.main[0].qty, 2);
        press(&mut app, '-');
        press(&mut app, '-');
        assert!(app.deck.main.is_empty(), "dropping to zero should remove the entry");
    }

    #[test]
    fn commander_key_tags_the_selected_card() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, 'C');
        assert_eq!(app.deck.main[0].category.as_deref(), Some(COMMANDER));
    }

    #[test]
    fn printing_picker_sets_set_and_number() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, 'p');
        assert_eq!(app.mode, Mode::Printing);
        app.on_key(KeyEvent::from(KeyCode::Enter));
        // Cheapest printing is first after ingestion sorts them.
        assert_eq!(app.deck.main[0].set.as_deref(), Some("ltc"));
        assert_eq!(app.deck.main[0].number.as_deref(), Some("292"));
    }

    #[test]
    fn printing_picker_can_reset_to_cheapest() {
        let mut app = app_with("1x Sol Ring (c21) 263 [Ramp]\n");
        assert_eq!(app.price.0, 2.75);
        press(&mut app, 'p');
        press(&mut app, '0');
        assert_eq!(app.deck.main[0].set, None);
        assert_eq!(app.price.0, 1.50, "should fall back to the cheapest printing");
    }

    #[test]
    fn tab_switches_board_and_m_moves_cards() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, 'm');
        assert!(app.deck.main.is_empty());
        assert_eq!(app.deck.maybe.len(), 1);
        app.on_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.board, Board::Maybe);
        assert!(app.selected_entry().is_some());
    }

    #[test]
    fn quit_is_guarded_while_dirty() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, '+');
        press(&mut app, 'q');
        assert!(!app.quit, "first q should warn, not quit");
        press(&mut app, 'q');
        assert!(app.quit, "second q should quit");
    }

    #[test]
    fn clean_deck_quits_immediately() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, 'q');
        assert!(app.quit);
    }

    #[test]
    fn category_prompt_rewrites_the_entry() {
        let mut app = app_with("1x Sol Ring [Ramp]\n");
        press(&mut app, 'c');
        assert_eq!(app.mode, Mode::Category);
        app.input.clear();
        for c in "Artifacts".chars() {
            press(&mut app, c);
        }
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.deck.main[0].category.as_deref(), Some("Artifacts"));
    }

    #[test]
    fn save_round_trips_through_disk() {
        let dir = std::env::temp_dir().join("mtgtuibuilder-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deck.txt");
        let _ = std::fs::remove_file(&path);

        let mut app = app_with("1x Sol Ring (ltc) 292 [Ramp]\n");
        app.deck.path = Some(path.clone());
        app.deck.dirty = true;
        press(&mut app, 's');
        assert!(!app.deck.dirty, "save should clear the dirty flag: {}", app.status);

        let reloaded = decklist::load(&path).unwrap();
        assert_eq!(reloaded.main, app.deck.main);
        let _ = std::fs::remove_file(&path);
    }
}
