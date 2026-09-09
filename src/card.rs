//! Card model. Trimmed from Scryfall's card object: only the fields the
//! builder actually reads, so the on-disk cache stays ~20x smaller than the
//! upstream bulk file and startup stays under a second.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One physical printing. Cardmarket's price is Scryfall's `eur` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Printing {
    pub set: String,
    pub set_name: String,
    pub number: String,
    pub eur: Option<f64>,
    /// Scryfall id, which is all that is needed to address the card image.
    /// Defaulted so a cache written before previews existed still loads.
    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub name: String,
    pub mana_cost: String,
    pub cmc: f32,
    pub type_line: String,
    pub oracle_text: String,
    pub color_identity: Vec<String>,
    pub commander_legal: bool,
    /// Sorted cheapest-first, so `printings[0]` is the default pick.
    pub printings: Vec<Printing>,
}

impl Card {
    pub fn cheapest(&self) -> Option<&Printing> {
        self.printings.first()
    }

    /// Resolve a decklist's `(set) number` back to a printing. Falls back to
    /// the cheapest when the set is unknown or the deck names no printing at
    /// all -- a decklist from elsewhere may reference a set we filtered out.
    pub fn printing(&self, set: Option<&str>, number: Option<&str>) -> Option<&Printing> {
        match set {
            None => self.cheapest(),
            Some(s) => {
                let s = s.to_lowercase();
                self.printings
                    .iter()
                    .find(|p| {
                        p.set == s && number.map(|n| p.number == n).unwrap_or(true)
                    })
                    .or_else(|| self.printings.iter().find(|p| p.set == s))
                    .or_else(|| self.cheapest())
            }
        }
    }

    pub fn is_land(&self) -> bool {
        self.type_line.contains("Land")
    }

    pub fn is_basic_land(&self) -> bool {
        self.type_line.contains("Basic") && self.is_land()
    }

    /// Relentless Rats and friends sidestep the singleton rule.
    pub fn unlimited(&self) -> bool {
        self.oracle_text
            .contains("A deck can have any number of cards named")
    }

    /// First type word after any supertypes -- what the type breakdown groups on.
    pub fn primary_type(&self) -> &str {
        const ORDER: [&str; 9] = [
            "Creature",
            "Planeswalker",
            "Instant",
            "Sorcery",
            "Artifact",
            "Enchantment",
            "Battle",
            "Land",
            "Kindred",
        ];
        let face = self.type_line.split("//").next().unwrap_or(&self.type_line);
        ORDER
            .iter()
            .find(|t| face.contains(**t))
            .copied()
            .unwrap_or("Other")
    }

    /// Coloured mana symbols in the cost, hybrids counted for both halves.
    pub fn pips(&self) -> Vec<char> {
        let mut out = Vec::new();
        for sym in self.mana_cost.split('{') {
            for c in sym.chars() {
                if matches!(c, 'W' | 'U' | 'B' | 'R' | 'G') {
                    out.push(c);
                }
            }
        }
        out
    }
}

/// Name-keyed card database. Decklists address cards by name, so that -- not
/// Scryfall's oracle_id -- is the primary key here.
pub struct CardDb {
    pub cards: Vec<Card>,
    index: HashMap<String, usize>,
}

impl CardDb {
    pub fn new(cards: Vec<Card>) -> Self {
        let index = cards
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.to_lowercase(), i))
            .collect();
        Self { cards, index }
    }

    pub fn get(&self, name: &str) -> Option<&Card> {
        let key = name.to_lowercase();
        if let Some(i) = self.index.get(&key) {
            return Some(&self.cards[*i]);
        }
        // Split cards are listed under "Fire // Ice" but often written as "Fire".
        self.index
            .iter()
            .find(|(k, _)| k.split(" // ").next() == Some(key.as_str()))
            .map(|(_, i)| &self.cards[*i])
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// Substring match, ranked: exact, then prefix, then contained. Cheap
    /// enough to re-run on every keystroke over ~30k names.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&Card> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u8, &Card)> = self
            .cards
            .iter()
            .filter_map(|c| {
                let name = c.name.to_lowercase();
                let rank = if name == q {
                    0
                } else if name.starts_with(&q) {
                    1
                } else if name.contains(&q) {
                    2
                } else if c.type_line.to_lowercase().contains(&q) {
                    3
                } else {
                    return None;
                };
                Some((rank, c))
            })
            .collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        hits.into_iter().take(limit).map(|(_, c)| c).collect()
    }
}
