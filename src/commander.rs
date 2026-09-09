//! Commander legality. Deeper than a generic format check because the rules
//! that actually constrain deckbuilding -- singleton and colour identity --
//! are Commander's.

use crate::card::CardDb;
use crate::deck::{COMMANDER, Deck};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub text: String,
}

impl Issue {
    fn error(text: impl Into<String>) -> Self {
        Self { severity: Severity::Error, text: text.into() }
    }
    fn warn(text: impl Into<String>) -> Self {
        Self { severity: Severity::Warn, text: text.into() }
    }
}

pub fn validate(deck: &Deck, db: &CardDb) -> Vec<Issue> {
    let mut issues = Vec::new();

    let commanders: Vec<_> = deck
        .main
        .iter()
        .filter(|e| e.category.as_deref() == Some(COMMANDER))
        .collect();

    // Colour identity is the union across partners, so it is derived before
    // the per-card checks below.
    let mut identity: Vec<String> = Vec::new();
    match commanders.len() {
        0 => issues.push(Issue::warn(
            "No commander set -- tag a card with the Commander category (c)",
        )),
        1 | 2 => {
            for e in &commanders {
                match db.get(&e.name) {
                    Some(card) => {
                        let face = card.type_line.split("//").next().unwrap_or("");
                        let eligible = (face.contains("Legendary") && face.contains("Creature"))
                            || card.oracle_text.contains("can be your commander");
                        if !eligible {
                            issues.push(Issue::error(format!(
                                "{} is not a legal commander",
                                card.name
                            )));
                        }
                        for c in &card.color_identity {
                            if !identity.contains(c) {
                                identity.push(c.clone());
                            }
                        }
                    }
                    None => issues
                        .push(Issue::error(format!("Unknown commander: {}", e.name))),
                }
            }
            if commanders.len() == 2 {
                issues.push(Issue::warn(
                    "Two commanders -- legal only with Partner or Background",
                ));
            }
        }
        n => issues.push(Issue::error(format!("{n} commanders tagged (max 2)"))),
    }

    let total = deck.total_cards();
    if total != 100 {
        let sev = if total > 100 { Severity::Error } else { Severity::Warn };
        issues.push(Issue {
            severity: sev,
            text: format!("{total}/100 cards ({:+})", total as i64 - 100),
        });
    }

    for e in &deck.main {
        let Some(card) = db.get(&e.name) else {
            issues.push(Issue::error(format!("Unknown card: {}", e.name)));
            continue;
        };

        if !card.commander_legal {
            issues.push(Issue::error(format!("{} is banned or not legal", card.name)));
        }

        if e.qty > 1 && !card.is_basic_land() && !card.unlimited() {
            issues.push(Issue::error(format!(
                "{}x {} breaks singleton",
                e.qty, card.name
            )));
        }

        // Only meaningful once a commander has fixed the identity.
        if !identity.is_empty() || !commanders.is_empty() {
            let outside: Vec<&str> = card
                .color_identity
                .iter()
                .filter(|c| !identity.contains(c))
                .map(|s| s.as_str())
                .collect();
            if !outside.is_empty() {
                issues.push(Issue::error(format!(
                    "{} is outside colour identity ({})",
                    card.name,
                    outside.join("")
                )));
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Card, Printing};
    use crate::deck::{Board, Entry};
    use crate::decklist;

    fn card(name: &str, type_line: &str, identity: &[&str], text: &str) -> Card {
        Card {
            name: name.into(),
            mana_cost: String::new(),
            cmc: 0.0,
            type_line: type_line.into(),
            oracle_text: text.into(),
            color_identity: identity.iter().map(|s| s.to_string()).collect(),
            commander_legal: true,
            printings: vec![Printing {
                set: "tst".into(),
                set_name: "Test".into(),
                number: "1".into(),
                eur: Some(1.0),
                id: String::new(),
            }],
        }
    }

    fn db() -> CardDb {
        CardDb::new(vec![
            card("Atraxa, Grand Unifier", "Legendary Creature — Phyrexian Angel", &["W", "U", "B", "G"], ""),
            card("Sol Ring", "Artifact", &[], ""),
            card("Lightning Bolt", "Instant", &["R"], ""),
            card("Forest", "Basic Land — Forest", &["G"], ""),
            card("Relentless Rats", "Creature — Rat", &["B"], "A deck can have any number of cards named Relentless Rats."),
            card("Goblin Guide", "Creature — Goblin Scout", &["R"], ""),
        ])
    }

    fn has(issues: &[Issue], needle: &str) -> bool {
        issues.iter().any(|i| i.text.contains(needle))
    }

    #[test]
    fn flags_missing_commander() {
        let deck = decklist::parse("1x Sol Ring\n");
        assert!(has(&validate(&deck, &db()), "No commander"));
    }

    #[test]
    fn flags_colour_identity_violation() {
        // Atraxa is WUBG, so a red card is outside her identity.
        let deck = decklist::parse(
            "1x Atraxa, Grand Unifier [Commander]\n1x Lightning Bolt\n",
        );
        assert!(has(&validate(&deck, &db()), "outside colour identity"));
    }

    #[test]
    fn accepts_card_inside_identity() {
        let deck = decklist::parse("1x Atraxa, Grand Unifier [Commander]\n1x Sol Ring\n");
        assert!(!has(&validate(&deck, &db()), "outside colour identity"));
    }

    #[test]
    fn flags_singleton_break() {
        let deck = decklist::parse("1x Atraxa, Grand Unifier [Commander]\n2x Sol Ring\n");
        assert!(has(&validate(&deck, &db()), "breaks singleton"));
    }

    #[test]
    fn basic_lands_are_exempt_from_singleton() {
        let deck = decklist::parse("1x Atraxa, Grand Unifier [Commander]\n30x Forest\n");
        assert!(!has(&validate(&deck, &db()), "breaks singleton"));
    }

    #[test]
    fn unlimited_cards_are_exempt_from_singleton() {
        let deck = decklist::parse("1x Relentless Rats [Commander]\n9x Relentless Rats\n");
        let issues = validate(&deck, &db());
        assert!(!has(&issues, "breaks singleton"));
    }

    #[test]
    fn rejects_non_legendary_commander() {
        let deck = decklist::parse("1x Goblin Guide [Commander]\n");
        assert!(has(&validate(&deck, &db()), "not a legal commander"));
    }

    #[test]
    fn reports_card_count_delta() {
        let deck = decklist::parse("1x Atraxa, Grand Unifier [Commander]\n");
        assert!(has(&validate(&deck, &db()), "1/100"));
    }

    #[test]
    fn flags_unknown_card() {
        let deck = decklist::parse("1x Definitely Not A Real Card\n");
        assert!(has(&validate(&deck, &db()), "Unknown card"));
    }

    #[test]
    fn maybeboard_is_not_validated() {
        let mut deck = decklist::parse("1x Atraxa, Grand Unifier [Commander]\n");
        deck.add(Board::Maybe, Entry::new("Lightning Bolt"));
        assert!(!has(&validate(&deck, &db()), "outside colour identity"));
    }

    #[test]
    fn warns_on_two_commanders() {
        let deck = decklist::parse(
            "1x Atraxa, Grand Unifier [Commander]\n1x Relentless Rats [Commander]\n",
        );
        assert!(has(&validate(&deck, &db()), "Two commanders"));
    }
}
