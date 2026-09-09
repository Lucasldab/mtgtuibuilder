//! Scryfall bulk ingestion.
//!
//! Cardmarket prices come from Scryfall's `eur` field rather than Cardmarket
//! directly: their own API needs OAuth1 with a registered dedicated app and
//! gates the price guide, while Scryfall redistributes the same numbers with
//! no auth and no rate limit.
//!
//! Upstream ships gzipped JSONL, so ingestion streams line by line -- the
//! uncompressed `default_cards` file is far too large to hold in memory.

use crate::card::{Card, CardDb, Printing};
use anyhow::{Context, Result, anyhow};
use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

const AGENT: &str = concat!("mtgtuibuilder/", env!("CARGO_PKG_VERSION"));
const BULK_INDEX: &str = "https://api.scryfall.com/bulk-data";

/// Layouts that are not real deck cards.
const SKIP_LAYOUTS: [&str; 7] = [
    "token",
    "double_faced_token",
    "emblem",
    "art_series",
    "vanguard",
    "scheme",
    "planar",
];

#[derive(Deserialize)]
struct BulkIndex {
    data: Vec<BulkEntry>,
}

#[derive(Deserialize)]
struct BulkEntry {
    #[serde(rename = "type")]
    kind: String,
    updated_at: String,
    jsonl_download_uri: String,
}

/// The subset of Scryfall's card object we keep. Unknown fields are ignored,
/// which is what keeps this resilient to upstream additions.
#[derive(Deserialize)]
struct RawCard {
    name: String,
    #[serde(default)]
    mana_cost: String,
    #[serde(default)]
    cmc: f32,
    #[serde(default)]
    type_line: String,
    #[serde(default)]
    oracle_text: String,
    #[serde(default)]
    color_identity: Vec<String>,
    #[serde(default)]
    layout: String,
    set: String,
    set_name: String,
    collector_number: String,
    #[serde(default)]
    digital: bool,
    #[serde(default)]
    prices: HashMap<String, Option<String>>,
    #[serde(default)]
    legalities: HashMap<String, String>,
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mtgtuibuilder")
}

fn cards_path() -> PathBuf {
    cache_dir().join("cards.jsonl")
}

fn stamp_path() -> PathBuf {
    cache_dir().join("updated_at")
}

/// Load the trimmed cache, downloading it first when absent or when `refresh`
/// is set. `progress` receives human-readable status lines.
pub fn load(refresh: bool, progress: &mut dyn FnMut(&str)) -> Result<CardDb> {
    if refresh || !cards_path().exists() {
        rebuild(progress)?;
    }
    progress("Loading card cache...");
    let file = File::open(cards_path()).context("opening card cache")?;
    let mut cards = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        cards.push(serde_json::from_str::<Card>(&line)?);
    }
    if cards.is_empty() {
        return Err(anyhow!("card cache is empty -- try --refresh"));
    }
    Ok(CardDb::new(cards))
}

/// When the local cache was built, for display.
pub fn cache_stamp() -> Option<String> {
    fs::read_to_string(stamp_path()).ok().map(|s| s.trim().to_string())
}

fn rebuild(progress: &mut dyn FnMut(&str)) -> Result<()> {
    fs::create_dir_all(cache_dir())?;

    progress("Fetching Scryfall bulk index...");
    let index_resp = ureq::get(BULK_INDEX)
        .set("User-Agent", AGENT)
        .call()
        .context("fetching bulk index")?;
    let index: BulkIndex = serde_json::from_reader(index_resp.into_reader())
        .context("parsing bulk index")?;

    // default_cards carries every printing, which is what per-printing
    // Cardmarket pricing needs; oracle_cards would collapse them to one.
    let entry = index
        .data
        .into_iter()
        .find(|e| e.kind == "default_cards")
        .ok_or_else(|| anyhow!("no default_cards bulk file published"))?;

    progress("Downloading default_cards (~80 MB compressed)...");
    let resp = ureq::get(&entry.jsonl_download_uri)
        .set("User-Agent", AGENT)
        .call()
        .context("downloading bulk data")?;

    let reader = BufReader::new(GzDecoder::new(resp.into_reader()));

    // Printings are grouped by name because that is how decklists address
    // cards; Scryfall emits them in no particular order.
    let mut grouped: HashMap<String, Card> = HashMap::new();
    let mut seen: u64 = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let raw: RawCard = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };
        seen += 1;
        if seen % 20_000 == 0 {
            progress(&format!("  parsed {seen} printings..."));
        }

        // Layout alone is not enough: Wilds of Eldraine Role tokens carry a
        // "Token Enchantment" type line under a non-token layout, so the type
        // line is checked too.
        if raw.digital
            || SKIP_LAYOUTS.contains(&raw.layout.as_str())
            || raw.type_line.starts_with("Token")
            || raw.type_line.starts_with("Emblem")
        {
            continue;
        }
        let commander_legal = raw.legalities.get("commander").map(|s| s == "legal").unwrap_or(false);

        let eur = raw
            .prices
            .get("eur")
            .and_then(|v| v.as_ref())
            .and_then(|s| s.parse::<f64>().ok());

        let printing = Printing {
            set: raw.set,
            set_name: raw.set_name,
            number: raw.collector_number,
            eur,
        };

        grouped
            .entry(raw.name.clone())
            .and_modify(|c| c.printings.push(printing.clone()))
            .or_insert_with(|| Card {
                name: raw.name,
                mana_cost: raw.mana_cost,
                cmc: raw.cmc,
                type_line: raw.type_line,
                oracle_text: raw.oracle_text,
                color_identity: raw.color_identity,
                commander_legal,
                printings: vec![printing],
            });
    }

    progress(&format!("Indexed {} distinct cards.", grouped.len()));

    // Cheapest first, so printings[0] is the default pick everywhere else.
    // Priceless printings sort last rather than reading as free.
    let mut cards: Vec<Card> = grouped.into_values().collect();
    for c in &mut cards {
        c.printings.sort_by(|a, b| {
            let ap = a.eur.unwrap_or(f64::MAX);
            let bp = b.eur.unwrap_or(f64::MAX);
            ap.partial_cmp(&bp).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    cards.sort_by(|a, b| a.name.cmp(&b.name));

    progress("Writing cache...");
    let tmp = cards_path().with_extension("tmp");
    {
        let mut out = BufWriter::new(File::create(&tmp)?);
        for c in &cards {
            serde_json::to_writer(&mut out, c)?;
            out.write_all(b"\n")?;
        }
        out.flush()?;
    }
    fs::rename(&tmp, cards_path())?;
    fs::write(stamp_path(), entry.updated_at)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real cache when one exists. Skipped rather than failed on
    /// a clean machine so `cargo test` works before the first `--refresh`.
    #[test]
    fn real_cache_has_prices_and_legality() {
        if !cards_path().exists() {
            eprintln!("skipping: no card cache, run with --refresh first");
            return;
        }
        let db = load(false, &mut |_| {}).expect("loading cache");
        assert!(db.len() > 20_000, "only {} cards", db.len());

        let sol = db.get("Sol Ring").expect("Sol Ring missing");
        assert!(sol.commander_legal);
        assert!(!sol.printings.is_empty());
        assert!(
            sol.printings.iter().any(|p| p.eur.is_some()),
            "no Cardmarket price on any Sol Ring printing"
        );

        // Cheapest-first ordering is what makes printings[0] the default pick.
        let priced: Vec<f64> = sol.printings.iter().filter_map(|p| p.eur).collect();
        assert!(
            priced.windows(2).all(|w| w[0] <= w[1]),
            "printings are not sorted cheapest-first"
        );

        // Colour identity drives Commander validation.
        let atraxa = db.get("Atraxa, Praetors' Voice").expect("Atraxa missing");
        let mut id = atraxa.color_identity.clone();
        id.sort();
        assert_eq!(id, vec!["B", "G", "U", "W"]);

        // A banned card must be caught.
        let lotus = db.get("Black Lotus").expect("Black Lotus missing");
        assert!(!lotus.commander_legal, "Black Lotus is banned in Commander");
    }

    #[test]
    fn tokens_are_excluded() {
        if !cards_path().exists() {
            return;
        }
        let db = load(false, &mut |_| {}).unwrap();
        // Every remaining card should be a real deck card.
        assert!(db.cards.iter().all(|c| !c.type_line.starts_with("Token")));
    }
}
