//! Background card-image loading.
//!
//! Scryfall's image CDN is addressable from the printing's id alone, so the
//! card cache only stores that id rather than four URLs per printing:
//!
//! ```text
//! https://cards.scryfall.io/normal/front/9/1/91fdb56b-...-505ff987fe9b.jpg
//! ```
//!
//! Fetching happens on a worker thread and decoded images are memoised, so
//! scrolling the deck never blocks on the network. Scryfall's API guidelines
//! ask clients to cache rather than re-fetch, so downloads also land on disk.

use image::DynamicImage;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

const AGENT: &str = concat!("mtgtuibuilder/", env!("CARGO_PKG_VERSION"));

/// Decoded images are ~1 MB each, so the deck's worth is not kept resident.
/// Evicted entries stay on disk and reload without a network round trip.
const MAX_RESIDENT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Loading,
    Ready,
    Failed,
}

pub struct Loader {
    requests: Sender<String>,
    replies: Receiver<(String, Option<DynamicImage>)>,
    images: HashMap<String, DynamicImage>,
    status: HashMap<String, Status>,
    resident: VecDeque<String>,
}

impl Loader {
    pub fn new() -> Self {
        let (requests, rx) = channel::<String>();
        let (tx, replies) = channel::<(String, Option<DynamicImage>)>();

        thread::spawn(move || {
            for id in rx {
                let decoded = fetch(&id);
                // A closed receiver just means the UI is gone.
                if tx.send((id, decoded)).is_err() {
                    break;
                }
            }
        });

        Self {
            requests,
            replies,
            images: HashMap::new(),
            status: HashMap::new(),
            resident: VecDeque::new(),
        }
    }

    /// Queues a fetch unless this id is already loading, loaded or known bad.
    pub fn request(&mut self, id: &str) {
        if id.is_empty() || self.status.contains_key(id) {
            return;
        }
        self.status.insert(id.to_string(), Status::Loading);
        let _ = self.requests.send(id.to_string());
    }

    /// Drains completed fetches. Returns true when something new arrived, so
    /// the caller knows to rebuild its render protocol.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.replies.try_recv() {
                Ok((id, Some(img))) => {
                    self.status.insert(id.clone(), Status::Ready);
                    self.images.insert(id.clone(), img);
                    self.resident.push_back(id);
                    while self.resident.len() > MAX_RESIDENT {
                        if let Some(old) = self.resident.pop_front() {
                            self.images.remove(&old);
                            // Dropping the status too lets it reload from disk.
                            self.status.remove(&old);
                        }
                    }
                    changed = true;
                }
                Ok((id, None)) => {
                    self.status.insert(id, Status::Failed);
                    changed = true;
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    pub fn get(&self, id: &str) -> Option<&DynamicImage> {
        self.images.get(id)
    }

    pub fn status(&self, id: &str) -> Option<Status> {
        self.status.get(id).copied()
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

pub fn image_dir() -> PathBuf {
    crate::scryfall::cache_dir().join("images")
}

/// Scryfall shards its CDN paths by the first two characters of the id.
pub fn image_url(id: &str) -> Option<String> {
    let mut chars = id.chars();
    let a = chars.next()?;
    let b = chars.next()?;
    Some(format!("https://cards.scryfall.io/normal/front/{a}/{b}/{id}.jpg"))
}

fn fetch(id: &str) -> Option<DynamicImage> {
    let path = image_dir().join(format!("{id}.jpg"));

    if let Ok(bytes) = fs::read(&path) {
        if let Some(img) = decode(&bytes) {
            return Some(img);
        }
        // A truncated file from an interrupted run: drop it and refetch.
        let _ = fs::remove_file(&path);
    }

    let url = image_url(id)?;
    let resp = ureq::get(&url).set("User-Agent", AGENT).call().ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes).ok()?;

    let img = decode(&bytes)?;
    // Write via a temp file so a kill mid-write cannot leave a partial JPEG.
    if fs::create_dir_all(image_dir()).is_ok() {
        let tmp = path.with_extension("part");
        if fs::write(&tmp, &bytes).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
    Some(img)
}

fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_sharded_cdn_path() {
        let id = "91fdb56b-54d5-4272-8319-505ff987fe9b";
        assert_eq!(
            image_url(id).unwrap(),
            format!("https://cards.scryfall.io/normal/front/9/1/{id}.jpg")
        );
    }

    #[test]
    fn rejects_ids_too_short_to_shard() {
        assert_eq!(image_url(""), None);
        assert_eq!(image_url("9"), None);
    }

    #[test]
    fn requests_are_deduplicated() {
        let mut l = Loader::new();
        l.request("aaaaaaaa-0000-0000-0000-000000000000");
        assert_eq!(l.status("aaaaaaaa-0000-0000-0000-000000000000"), Some(Status::Loading));
        // A second request must not reset a slot that is already in flight.
        l.request("aaaaaaaa-0000-0000-0000-000000000000");
        assert_eq!(l.status("aaaaaaaa-0000-0000-0000-000000000000"), Some(Status::Loading));
    }

    #[test]
    fn empty_id_is_never_requested() {
        let mut l = Loader::new();
        l.request("");
        assert_eq!(l.status(""), None);
    }

    #[test]
    fn poll_is_non_blocking_when_idle() {
        let mut l = Loader::new();
        assert!(!l.poll());
    }
}
