//! Archidekt-dialect decklist text. This is the only on-disk format, so it has
//! to round-trip everything the deck model holds:
//!
//! ```text
//! 1x Sol Ring (ltc) 292 [Ramp]
//! 1x Atraxa, Grand Unifier (one) 196 [Commander]
//!
//! Maybeboard
//! 1x Mana Crypt (2xm) 270 [Ramp]
//! ```
//!
//! Quantity may be `1` or `1x`; the printing and category are both optional,
//! so a plain Moxfield or MTGO list parses unchanged.

use crate::deck::{Board, Deck, Entry};
use anyhow::Result;
use std::fs;
use std::path::Path;

const MAYBE_HEADERS: [&str; 3] = ["maybeboard", "sideboard", "maybe"];
const MAIN_HEADERS: [&str; 4] = ["deck", "mainboard", "main", "commander"];

pub fn parse(text: &str) -> Deck {
    let mut deck = Deck::default();
    let mut board = Board::Main;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }

        let lower = line.to_lowercase();
        let header = lower.trim_end_matches(':').trim();
        if MAYBE_HEADERS.contains(&header) {
            board = Board::Maybe;
            continue;
        }
        if MAIN_HEADERS.contains(&header) {
            board = Board::Main;
            continue;
        }

        if let Some(entry) = parse_line(line) {
            deck.add(board, entry);
        }
    }

    deck.dirty = false;
    deck
}

fn parse_line(line: &str) -> Option<Entry> {
    let (qty, rest) = split_qty(line)?;
    let (rest, category) = split_bracket(rest);
    let (name, set, number) = split_printing(rest);

    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    // Archidekt marks the commander's category as `Commander{top}`, and allows
    // several comma-separated categories per card; the first one wins here.
    let category = category.map(|c| {
        c.split('{')
            .next()
            .unwrap_or(&c)
            .split(',')
            .next()
            .unwrap_or(&c)
            .trim()
            .to_string()
    });

    Some(Entry {
        qty,
        name: name.to_string(),
        set,
        number,
        category: category.filter(|c| !c.is_empty()),
    })
}

/// Leading `12x ` or `12 `.
fn split_qty(line: &str) -> Option<(u32, &str)> {
    let end = line.find(|c: char| !c.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    let qty: u32 = line[..end].parse().ok()?;
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('x').unwrap_or(rest).trim_start();
    Some((qty.max(1), rest))
}

/// Splits off a trailing `[category]`.
fn split_bracket(s: &str) -> (&str, Option<String>) {
    let s = s.trim_end();
    let Some(o) = s.rfind('[') else {
        return (s, None);
    };
    let Some(c) = s[o..].find(']').map(|i| i + o) else {
        return (s, None);
    };
    (s[..o].trim_end(), Some(s[o + 1..c].to_string()))
}

/// Splits `Sol Ring (ltc) 292` into name, set and collector number. The number
/// trails the closing paren, so it is picked up separately rather than being
/// dropped -- losing it would break round-tripping an Archidekt export.
fn split_printing(s: &str) -> (&str, Option<String>, Option<String>) {
    let s = s.trim_end();
    let Some(o) = s.rfind('(') else {
        return (s, None, None);
    };
    let Some(c) = s[o..].find(')').map(|i| i + o) else {
        return (s, None, None);
    };
    let set = s[o + 1..c].trim().to_lowercase();
    if set.is_empty() {
        return (s, None, None);
    }
    let number = s[c + 1..].trim();
    let number = (!number.is_empty()).then(|| number.to_string());
    (s[..o].trim_end(), Some(set), number)
}

pub fn render(deck: &Deck) -> String {
    let mut out = String::new();
    render_board(&mut out, deck, Board::Main);
    if !deck.maybe.is_empty() {
        out.push_str("\nMaybeboard\n");
        render_board(&mut out, deck, Board::Maybe);
    }
    out
}

fn render_board(out: &mut String, deck: &Deck, board: Board) {
    for cat in deck.categories(board) {
        for e in deck.board(board).iter().filter(|e| e.category_or_default() == cat) {
            out.push_str(&format!("{}x {}", e.qty, e.name));
            if let Some(set) = &e.set {
                out.push_str(&format!(" ({set})"));
                if let Some(n) = &e.number {
                    out.push_str(&format!(" {n}"));
                }
            }
            if let Some(c) = &e.category {
                out.push_str(&format!(" [{c}]"));
            }
            out.push('\n');
        }
    }
}

pub fn load(path: &Path) -> Result<Deck> {
    let text = fs::read_to_string(path)?;
    let mut deck = parse(&text);
    deck.path = Some(path.to_path_buf());
    Ok(deck)
}

pub fn save(deck: &Deck, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir)?;
        }
    }
    fs::write(path, render(deck))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::Board;

    #[test]
    fn parses_plain_moxfield_line() {
        let d = parse("1 Sol Ring\n2 Forest\n");
        assert_eq!(d.main.len(), 2);
        assert_eq!(d.main[0].name, "Sol Ring");
        assert_eq!(d.main[0].qty, 1);
        assert_eq!(d.main[0].set, None);
        assert_eq!(d.main[1].qty, 2);
    }

    #[test]
    fn parses_full_archidekt_line() {
        let d = parse("1x Sol Ring (ltc) 292 [Ramp]\n");
        let e = &d.main[0];
        assert_eq!(e.name, "Sol Ring");
        assert_eq!(e.set.as_deref(), Some("ltc"));
        assert_eq!(e.number.as_deref(), Some("292"));
        assert_eq!(e.category.as_deref(), Some("Ramp"));
    }

    #[test]
    fn strips_archidekt_commander_marker() {
        let d = parse("1x Atraxa, Grand Unifier (one) 196 [Commander{top}]\n");
        assert_eq!(d.main[0].category.as_deref(), Some("Commander"));
        assert_eq!(d.main[0].name, "Atraxa, Grand Unifier");
    }

    #[test]
    fn takes_first_of_several_categories() {
        let d = parse("1x Sol Ring (ltc) 292 [Ramp,Artifact]\n");
        assert_eq!(d.main[0].category.as_deref(), Some("Ramp"));
    }

    #[test]
    fn splits_maybeboard() {
        let d = parse("1x Sol Ring\n\nMaybeboard\n1x Mana Crypt\n");
        assert_eq!(d.main.len(), 1);
        assert_eq!(d.maybe.len(), 1);
        assert_eq!(d.maybe[0].name, "Mana Crypt");
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let d = parse("// a comment\n\n# another\n1x Sol Ring\n");
        assert_eq!(d.main.len(), 1);
    }

    #[test]
    fn merges_duplicate_lines() {
        let d = parse("1x Forest\n1x Forest\n");
        assert_eq!(d.main.len(), 1);
        assert_eq!(d.main[0].qty, 2);
    }

    #[test]
    fn round_trips_through_render() {
        let src = "1x Atraxa, Grand Unifier (one) 196 [Commander]\n\
                   1x Sol Ring (ltc) 292 [Ramp]\n\
                   \nMaybeboard\n1x Mana Crypt (2xm) 270 [Ramp]\n";
        let once = parse(src);
        let twice = parse(&render(&once));
        assert_eq!(once.main, twice.main);
        assert_eq!(once.maybe, twice.maybe);
        assert_eq!(twice.main[0].number.as_deref(), Some("196"));
    }

    #[test]
    fn parsing_leaves_deck_clean() {
        // A freshly loaded deck must not look modified, or every open would
        // prompt to save on quit.
        assert!(!parse("1x Sol Ring\n").dirty);
    }

    #[test]
    fn commander_category_pins_to_top_on_render() {
        let d = parse("1x Sol Ring [Ramp]\n1x Atraxa, Grand Unifier [Commander]\n");
        let out = render(&d);
        let cmd = out.find("Atraxa").unwrap();
        let ramp = out.find("Sol Ring").unwrap();
        assert!(cmd < ramp, "commander should render first:\n{out}");
    }

    #[test]
    fn board_move_keeps_entry() {
        let mut d = parse("1x Sol Ring [Ramp]\n");
        d.shift(Board::Main, 0);
        assert!(d.main.is_empty());
        assert_eq!(d.maybe[0].name, "Sol Ring");
    }
}
