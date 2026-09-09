//! Deck model. Mirrors what an Archidekt text export can express: quantity,
//! name, an optional printing, and an optional category -- plus a maybeboard.

use crate::card::CardDb;
use std::path::PathBuf;

pub const COMMANDER: &str = "Commander";

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub qty: u32,
    pub name: String,
    /// Printing override. `None` means "whatever is cheapest today".
    pub set: Option<String>,
    pub number: Option<String>,
    pub category: Option<String>,
}

impl Entry {
    pub fn new(name: impl Into<String>) -> Self {
        Self { qty: 1, name: name.into(), set: None, number: None, category: None }
    }

    pub fn category_or_default(&self) -> &str {
        self.category.as_deref().unwrap_or("Uncategorized")
    }
}

/// Which list an entry lives in. The maybeboard is excluded from legality and
/// from the deck price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    Main,
    Maybe,
}

#[derive(Debug, Default)]
pub struct Deck {
    pub main: Vec<Entry>,
    pub maybe: Vec<Entry>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

impl Deck {
    pub fn board(&self, b: Board) -> &Vec<Entry> {
        match b {
            Board::Main => &self.main,
            Board::Maybe => &self.maybe,
        }
    }

    pub fn board_mut(&mut self, b: Board) -> &mut Vec<Entry> {
        match b {
            Board::Main => &mut self.main,
            Board::Maybe => &mut self.maybe,
        }
    }

    /// Adds one copy, merging into an existing line for the same name and
    /// printing so the list does not accumulate duplicate rows.
    pub fn add(&mut self, b: Board, entry: Entry) {
        let list = self.board_mut(b);
        if let Some(e) = list
            .iter_mut()
            .find(|e| e.name == entry.name && e.set == entry.set)
        {
            e.qty += entry.qty;
        } else {
            list.push(entry);
        }
        self.dirty = true;
    }

    pub fn remove(&mut self, b: Board, idx: usize) {
        let list = self.board_mut(b);
        if idx < list.len() {
            list.remove(idx);
            self.dirty = true;
        }
    }

    pub fn bump(&mut self, b: Board, idx: usize, delta: i32) {
        let list = self.board_mut(b);
        if let Some(e) = list.get_mut(idx) {
            let next = e.qty as i32 + delta;
            if next <= 0 {
                list.remove(idx);
            } else {
                e.qty = next as u32;
            }
            self.dirty = true;
        }
    }

    /// Moves an entry between the main deck and the maybeboard.
    pub fn shift(&mut self, from: Board, idx: usize) {
        let list = self.board_mut(from);
        if idx >= list.len() {
            return;
        }
        let entry = list.remove(idx);
        let to = match from {
            Board::Main => Board::Maybe,
            Board::Maybe => Board::Main,
        };
        self.add(to, entry);
    }

    pub fn total_cards(&self) -> u32 {
        self.main.iter().map(|e| e.qty).sum()
    }

    pub fn commander(&self) -> Option<&Entry> {
        self.main
            .iter()
            .find(|e| e.category.as_deref() == Some(COMMANDER))
    }

    /// Deck price in EUR, using each entry's chosen printing and falling back
    /// to the cheapest. Cards with no Cardmarket price are counted as zero and
    /// reported separately by the caller.
    pub fn price(&self, db: &CardDb) -> (f64, usize) {
        let mut total = 0.0;
        let mut unpriced = 0;
        for e in &self.main {
            let Some(card) = db.get(&e.name) else {
                unpriced += e.qty as usize;
                continue;
            };
            match card
                .printing(e.set.as_deref(), e.number.as_deref())
                .and_then(|p| p.eur)
            {
                Some(v) => total += v * e.qty as f64,
                None => unpriced += e.qty as usize,
            }
        }
        (total, unpriced)
    }

    /// Distinct categories in list order, with Commander pinned first.
    pub fn categories(&self, b: Board) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in self.board(b) {
            let c = e.category_or_default().to_string();
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out.sort_by_key(|c| (c != COMMANDER, c.clone()));
        out
    }
}
