//! The thumbnail cache. The picker re-renders and polls every few seconds;
//! serving a recent thumbnail instead of capturing again keeps the picker
//! from repeatedly capturing someone else's window (a running game feels it
//! as an FPS dip).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::Image;

/// How long a captured thumbnail stays fresh.
pub(crate) const THUMBNAIL_TTL: Duration = Duration::from_secs(4);

/// Thumbnails by `(source id, max width)`.
#[derive(Debug, Default)]
pub(crate) struct ThumbnailCache {
    entries: HashMap<(String, u32), (Image, Instant)>,
}

impl ThumbnailCache {
    /// A thumbnail captured less than [`THUMBNAIL_TTL`] before `now`.
    pub(crate) fn get(&self, id: &str, max_width: u32, now: Instant) -> Option<Image> {
        let (image, at) = self.entries.get(&(id.to_owned(), max_width))?;
        (now.saturating_duration_since(*at) < THUMBNAIL_TTL).then(|| image.clone())
    }

    /// Stores a thumbnail captured at `now`, dropping the stale ones.
    pub(crate) fn put(&mut self, id: &str, max_width: u32, image: Image, now: Instant) {
        self.entries
            .retain(|_, (_, at)| now.saturating_duration_since(*at) < THUMBNAIL_TTL);
        self.entries
            .insert((id.to_owned(), max_width), (image, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Image {
        Image {
            width: 1,
            height: 1,
            bgra: vec![1, 2, 3, 255],
        }
    }

    #[test]
    fn a_fresh_entry_is_served() {
        let mut cache = ThumbnailCache::default();
        let t0 = Instant::now();
        cache.put("window-1", 480, image(), t0);
        assert_eq!(
            cache.get("window-1", 480, t0 + Duration::from_secs(3)),
            Some(image())
        );
    }

    #[test]
    fn a_stale_entry_is_not() {
        let mut cache = ThumbnailCache::default();
        let t0 = Instant::now();
        cache.put("window-1", 480, image(), t0);
        assert_eq!(cache.get("window-1", 480, t0 + THUMBNAIL_TTL), None);
    }

    #[test]
    fn the_width_is_part_of_the_key() {
        let mut cache = ThumbnailCache::default();
        let t0 = Instant::now();
        cache.put("window-1", 480, image(), t0);
        assert_eq!(cache.get("window-1", 240, t0), None);
    }

    #[test]
    fn the_id_is_part_of_the_key() {
        let mut cache = ThumbnailCache::default();
        let t0 = Instant::now();
        cache.put("window-1", 480, image(), t0);
        assert_eq!(cache.get("window-2", 480, t0), None);
    }

    #[test]
    fn putting_drops_stale_entries() {
        let mut cache = ThumbnailCache::default();
        let t0 = Instant::now();
        cache.put("window-1", 480, image(), t0);
        cache.put("window-2", 480, image(), t0 + Duration::from_secs(10));
        assert_eq!(cache.entries.len(), 1);
    }
}
