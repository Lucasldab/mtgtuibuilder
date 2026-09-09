//! Commander-level card suggestions from EDHREC.
//!
//! EDHREC publishes no documented API; this reads the JSON its own pages are
//! built from. That makes the shape unofficial and liable to change without
//! notice, so parsing is deliberately lenient -- missing sections or fields
//! degrade to fewer suggestions rather than an error -- and every response is
//! cached on disk so a commander is fetched at most once.

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

const AGENT: &str = concat!(
    "mtgtuibuilder/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Lucasldab/mtgtuibuilder)"
);

/// One recommended card.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub name: String,
    /// The EDHREC section it came from, e.g. "High Synergy Cards".
    pub section: String,
    pub num_decks: u32,
    pub potential_decks: u32,
    /// How much more often this appears here than in decks that merely could
    /// run it. High inclusion with low synergy is just a staple.
    pub synergy: f32,
}

impl Suggestion {
    /// Share of eligible decks running this card, as a percentage.
    pub fn inclusion(&self) -> f32 {
        if self.potential_decks == 0 {
            return 0.0;
        }
        100.0 * self.num_decks as f32 / self.potential_decks as f32
    }
}

/// EDHREC's URL form of a card name: lowercase, punctuation dropped, spaces
/// hyphenated. "Atraxa, Praetors' Voice" -> "atraxa-praetors-voice".
pub fn slug(name: &str) -> String {
    // Only the front face is addressable for split and double-faced names.
    let name = name.split("//").next().unwrap_or(name);
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true; // leading dashes are trimmed by this
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if matches!(c, '\'' | '\u{2019}' | ',' | '.' | ':' | '!' | '?' | '"') {
            // Dropped outright rather than hyphenated, so "Praetors' Voice"
            // becomes "praetors-voice" and not "praetors--voice".
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn cache_dir() -> PathBuf {
    crate::scryfall::cache_dir().join("edhrec")
}

// --- Response shape -------------------------------------------------------

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    container: Container,
}

#[derive(Deserialize, Default)]
struct Container {
    #[serde(default)]
    json_dict: JsonDict,
}

#[derive(Deserialize, Default)]
struct JsonDict {
    #[serde(default)]
    cardlists: Vec<CardList>,
}

#[derive(Deserialize)]
struct CardList {
    #[serde(default)]
    header: String,
    #[serde(default)]
    cardviews: Vec<CardView>,
}

#[derive(Deserialize)]
struct CardView {
    name: String,
    #[serde(default)]
    num_decks: u32,
    #[serde(default)]
    potential_decks: u32,
    #[serde(default)]
    synergy: f32,
}

/// Flattens the page's sections into one list, most-played first.
///
/// A card appears in several sections -- "Top Cards" repeats what is also
/// under "Creatures" -- so the first occurrence wins, which keeps the more
/// interesting section label since those come first in the response.
pub fn parse(body: &str) -> Result<Vec<Suggestion>> {
    let page: Page = serde_json::from_str(body).context("parsing EDHREC page")?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();

    for list in page.container.json_dict.cardlists {
        for cv in list.cardviews {
            if !seen.insert(cv.name.clone()) {
                continue;
            }
            out.push(Suggestion {
                name: cv.name,
                section: list.header.clone(),
                num_decks: cv.num_decks,
                potential_decks: cv.potential_decks,
                synergy: cv.synergy,
            });
        }
    }

    if out.is_empty() {
        return Err(anyhow!("no suggestions in response"));
    }
    out.sort_by(|a, b| {
        b.inclusion()
            .partial_cmp(&a.inclusion())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

fn fetch(slug: &str) -> Result<Vec<Suggestion>> {
    let path = cache_dir().join(format!("{slug}.json"));
    if let Ok(body) = fs::read_to_string(&path) {
        if let Ok(list) = parse(&body) {
            return Ok(list);
        }
        let _ = fs::remove_file(&path);
    }

    let url = format!("https://json.edhrec.com/pages/commanders/{slug}.json");
    let body = ureq::get(&url)
        .set("User-Agent", AGENT)
        .call()
        .context("fetching EDHREC page")?
        .into_string()?;

    let list = parse(&body)?;
    if fs::create_dir_all(cache_dir()).is_ok() {
        let tmp = path.with_extension("part");
        if fs::write(&tmp, &body).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
    Ok(list)
}

/// Background fetcher. One commander at a time is all the UI ever needs.
pub struct Loader {
    requests: Sender<String>,
    replies: Receiver<(String, Result<Vec<Suggestion>, String>)>,
    pub pending: Option<String>,
}

impl Loader {
    pub fn new() -> Self {
        let (requests, rx) = channel::<String>();
        let (tx, replies) = channel();
        thread::spawn(move || {
            for slug in rx {
                let result = fetch(&slug).map_err(|e| e.to_string());
                if tx.send((slug, result)).is_err() {
                    break;
                }
            }
        });
        Self { requests, replies, pending: None }
    }

    pub fn request(&mut self, slug: &str) {
        self.pending = Some(slug.to_string());
        let _ = self.requests.send(slug.to_string());
    }

    /// Returns a finished fetch, if one has arrived.
    pub fn poll(&mut self) -> Option<(String, Result<Vec<Suggestion>, String>)> {
        match self.replies.try_recv() {
            Ok(reply) => {
                if self.pending.as_deref() == Some(reply.0.as_str()) {
                    self.pending = None;
                }
                Some(reply)
            }
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_edhrec_urls() {
        assert_eq!(slug("Atraxa, Praetors' Voice"), "atraxa-praetors-voice");
        assert_eq!(slug("Kenrith, the Returned King"), "kenrith-the-returned-king");
        assert_eq!(slug("Sol Ring"), "sol-ring");
    }

    #[test]
    fn slug_drops_apostrophes_rather_than_hyphenating_them() {
        // "praetors--voice" would 404.
        assert!(!slug("Atraxa, Praetors' Voice").contains("--"));
        assert_eq!(slug("Gix, Yawgmoth Praetor"), "gix-yawgmoth-praetor");
    }

    #[test]
    fn slug_uses_the_front_face_only() {
        assert_eq!(slug("Fire // Ice"), "fire");
    }

    #[test]
    fn slug_handles_a_typographic_apostrophe() {
        assert_eq!(slug("Atraxa, Praetors\u{2019} Voice"), "atraxa-praetors-voice");
    }

    fn page() -> &'static str {
        r#"{"container":{"json_dict":{"cardlists":[
          {"header":"High Synergy Cards","cardviews":[
            {"name":"Evolution Sage","num_decks":591,"potential_decks":1000,"synergy":0.24}
          ]},
          {"header":"Creatures","cardviews":[
            {"name":"Evolution Sage","num_decks":591,"potential_decks":1000,"synergy":0.24},
            {"name":"Solemn Simulacrum","num_decks":800,"potential_decks":1000,"synergy":0.02}
          ]}
        ]}}}"#
    }

    #[test]
    fn parses_and_sorts_by_inclusion() {
        let s = parse(page()).unwrap();
        assert_eq!(s[0].name, "Solemn Simulacrum");
        assert_eq!(s[1].name, "Evolution Sage");
        assert!((s[0].inclusion() - 80.0).abs() < 0.01);
    }

    #[test]
    fn a_card_in_two_sections_keeps_the_first_label() {
        // "Top Cards"-style sections come first and are the more useful label.
        let s = parse(page()).unwrap();
        let sage = s.iter().find(|x| x.name == "Evolution Sage").unwrap();
        assert_eq!(sage.section, "High Synergy Cards");
        assert_eq!(s.len(), 2, "the duplicate must not be listed twice");
    }

    #[test]
    fn inclusion_survives_a_zero_denominator() {
        let s = Suggestion {
            name: "x".into(),
            section: String::new(),
            num_decks: 5,
            potential_decks: 0,
            synergy: 0.0,
        };
        assert_eq!(s.inclusion(), 0.0);
    }

    #[test]
    fn missing_sections_are_an_error_not_a_panic() {
        assert!(parse(r#"{"container":{"json_dict":{"cardlists":[]}}}"#).is_err());
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        // The shape is unofficial; additions upstream must not break parsing.
        let body = r#"{"unexpected":1,"container":{"json_dict":{"cardlists":[
          {"header":"Top Cards","extra":true,"cardviews":[
            {"name":"Sol Ring","num_decks":1,"potential_decks":2,"synergy":0.1,"trend_zscore":9}
          ]}]}}}"#;
        assert_eq!(parse(body).unwrap()[0].name, "Sol Ring");
    }
}
