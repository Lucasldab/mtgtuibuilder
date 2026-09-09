//! Deck statistics: the numbers Archidekt puts beside the list.

use crate::card::CardDb;
use crate::deck::Deck;
use std::collections::BTreeMap;

/// Curve buckets are 0..=6 with everything 7+ collapsed into the last slot.
pub const CURVE_BUCKETS: usize = 8;

#[derive(Debug, Default)]
pub struct Stats {
    pub curve: [u32; CURVE_BUCKETS],
    pub pips: BTreeMap<char, u32>,
    pub types: Vec<(String, u32)>,
    pub avg_cmc: f32,
    pub lands: u32,
    pub nonlands: u32,
}

pub fn compute(deck: &Deck, db: &CardDb) -> Stats {
    let mut s = Stats::default();
    let mut type_counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut cmc_total = 0.0f32;

    for e in &deck.main {
        let Some(card) = db.get(&e.name) else { continue };
        let qty = e.qty;

        *type_counts.entry(card.primary_type().to_string()).or_default() += qty;

        if card.is_land() {
            s.lands += qty;
            // Lands sit outside the curve and the average; including them
            // would flatten both and misrepresent the deck's speed.
            continue;
        }

        s.nonlands += qty;
        cmc_total += card.cmc * qty as f32;
        let bucket = (card.cmc as usize).min(CURVE_BUCKETS - 1);
        s.curve[bucket] += qty;

        for pip in card.pips() {
            *s.pips.entry(pip).or_default() += qty;
        }
    }

    s.avg_cmc = if s.nonlands > 0 { cmc_total / s.nonlands as f32 } else { 0.0 };

    // Largest group first -- that ordering is what makes the panel scannable.
    let mut types: Vec<(String, u32)> = type_counts.into_iter().collect();
    types.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    s.types = types;

    s
}

/// Ordered WUBRG, which is how every other Magic tool renders pips.
pub fn pip_order(pips: &BTreeMap<char, u32>) -> Vec<(char, u32)> {
    ['W', 'U', 'B', 'R', 'G']
        .iter()
        .filter_map(|c| pips.get(c).map(|n| (*c, *n)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Card, Printing};
    use crate::decklist;

    fn card(name: &str, cost: &str, cmc: f32, type_line: &str) -> Card {
        Card {
            name: name.into(),
            mana_cost: cost.into(),
            cmc,
            type_line: type_line.into(),
            oracle_text: String::new(),
            color_identity: Vec::new(),
            commander_legal: true,
            printings: vec![Printing {
                set: "tst".into(),
                set_name: "Test".into(),
                number: "1".into(),
                eur: Some(2.50),
            }],
        }
    }

    fn db() -> CardDb {
        CardDb::new(vec![
            card("Sol Ring", "{1}", 1.0, "Artifact"),
            card("Lightning Bolt", "{R}", 1.0, "Instant"),
            card("Wrath of God", "{2}{W}{W}", 4.0, "Sorcery"),
            card("Forest", "", 0.0, "Basic Land — Forest"),
            card("Boros Charm", "{R}{W}", 2.0, "Instant"),
        ])
    }

    #[test]
    fn lands_are_excluded_from_curve_and_average() {
        let deck = decklist::parse("10x Forest\n1x Wrath of God\n");
        let s = compute(&deck, &db());
        assert_eq!(s.lands, 10);
        assert_eq!(s.nonlands, 1);
        assert_eq!(s.curve[0], 0, "lands must not land in the 0-drop bucket");
        assert_eq!(s.avg_cmc, 4.0);
    }

    #[test]
    fn curve_buckets_by_cmc_and_respects_quantity() {
        let deck = decklist::parse("1x Sol Ring\n1x Lightning Bolt\n1x Wrath of God\n");
        let s = compute(&deck, &db());
        assert_eq!(s.curve[1], 2);
        assert_eq!(s.curve[4], 1);
    }

    #[test]
    fn counts_pips_per_copy() {
        let deck = decklist::parse("1x Wrath of God\n1x Boros Charm\n");
        let s = compute(&deck, &db());
        assert_eq!(s.pips.get(&'W'), Some(&3));
        assert_eq!(s.pips.get(&'R'), Some(&1));
    }

    #[test]
    fn pips_render_in_wubrg_order() {
        let deck = decklist::parse("1x Boros Charm\n");
        let s = compute(&deck, &db());
        let order: Vec<char> = pip_order(&s.pips).into_iter().map(|(c, _)| c).collect();
        assert_eq!(order, vec!['W', 'R']);
    }

    #[test]
    fn types_are_sorted_by_count() {
        let deck = decklist::parse("2x Forest\n1x Lightning Bolt\n1x Boros Charm\n");
        let s = compute(&deck, &db());
        assert_eq!(s.types[0], ("Instant".to_string(), 2));
    }

    #[test]
    fn price_multiplies_by_quantity_and_skips_maybeboard() {
        let deck = decklist::parse("2x Sol Ring\n\nMaybeboard\n5x Forest\n");
        let (total, unpriced) = deck.price(&db());
        assert!((total - 5.00).abs() < 1e-9, "got {total}");
        assert_eq!(unpriced, 0);
    }

    #[test]
    fn unknown_cards_are_reported_not_priced_as_zero() {
        let deck = decklist::parse("3x Not A Card\n");
        let (total, unpriced) = deck.price(&db());
        assert_eq!(total, 0.0);
        assert_eq!(unpriced, 3);
    }

    #[test]
    fn empty_deck_has_zero_average() {
        let s = compute(&decklist::parse(""), &db());
        assert_eq!(s.avg_cmc, 0.0);
    }
}
