//! Reading a log the game is still writing: only the bytes added since the
//! last read, a bounded amount at a time, and starting over when the file
//! is replaced or truncated (the game recreates it on every start).
//!
//! The file is opened read-only for each read and closed again. Rust opens
//! it with every share mode (`FILE_SHARE_READ | WRITE | DELETE`), so the
//! game can keep writing, truncate or delete it at any time.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How far back the first read starts (the game writes a few MB a session).
pub const RESYNC_BYTES: u64 = 4 << 20;
/// Most bytes one read takes; the rest waits for the next.
pub const READ_MAX: u64 = 512 << 10;
/// Longer lines keep their first this many bytes (every signal is near the
/// start of its line).
pub const LINE_MAX: usize = 4 << 10;
/// The file's first bytes, which tell one run's file from the next (each
/// starts with its own time).
const HEAD_BYTES: usize = 64;

/// One read.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Chunk {
    /// The file is new (first read, replaced or truncated): what came
    /// before it no longer applies.
    pub reset: bool,
    /// Complete lines, in order.
    pub lines: Vec<String>,
    /// More is waiting than one read takes.
    pub behind: bool,
    /// When the file was last written.
    pub modified: Option<SystemTime>,
}

/// A followed file.
#[derive(Debug)]
pub struct LogTail {
    path: PathBuf,
    /// Read up to here.
    offset: u64,
    /// No read yet (or the file went away): the next one resyncs.
    fresh: bool,
    head: Vec<u8>,
    /// An unfinished last line.
    partial: Vec<u8>,
    /// Dropping bytes up to the next line end (the first read started in
    /// the middle of a line).
    skip_line: bool,
}

impl LogTail {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            fresh: true,
            head: Vec::new(),
            partial: Vec::new(),
            skip_line: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn restart_at(&mut self, offset: u64) {
        self.offset = offset;
        self.partial.clear();
        self.skip_line = offset > 0;
    }

    /// Reads what was added. `Ok(None)`: there is no file.
    pub fn poll(&mut self) -> io::Result<Option<Chunk>> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.fresh = true;
                return Ok(None);
            }
            Err(err) => return Err(err),
        };
        let meta = file.metadata()?;
        let len = meta.len();
        let mut chunk = Chunk {
            modified: meta.modified().ok(),
            ..Chunk::default()
        };
        let head = read_head(&mut file, len)?;
        if self.fresh {
            self.fresh = false;
            chunk.reset = true;
            self.restart_at(len.saturating_sub(RESYNC_BYTES));
        } else if len < self.offset || !same_head(&self.head, &head) {
            chunk.reset = true;
            self.restart_at(0);
        }
        self.head = head;
        if len <= self.offset {
            return Ok(Some(chunk));
        }
        let want = (len - self.offset).min(READ_MAX);
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::with_capacity(want as usize);
        file.take(want).read_to_end(&mut bytes)?;
        self.offset += bytes.len() as u64;
        chunk.behind = self.offset < len;
        self.split(&bytes, &mut chunk.lines);
        Ok(Some(chunk))
    }

    fn split(&mut self, bytes: &[u8], lines: &mut Vec<String>) {
        for &byte in bytes {
            if byte == b'\n' {
                if !self.skip_line {
                    let line = String::from_utf8_lossy(&self.partial);
                    lines.push(line.trim_end_matches('\r').to_owned());
                }
                self.skip_line = false;
                self.partial.clear();
            } else if !self.skip_line && self.partial.len() < LINE_MAX {
                self.partial.push(byte);
            }
        }
    }
}

fn read_head(file: &mut File, len: u64) -> io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(HEAD_BYTES);
    file.seek(SeekFrom::Start(0))?;
    file.take(len.min(HEAD_BYTES as u64)).read_to_end(&mut head)?;
    Ok(head)
}

/// Whether two reads of the head are the same file (one may have grown
/// past the other's length).
fn same_head(old: &[u8], new: &[u8]) -> bool {
    let n = old.len().min(new.len());
    old[..n] == new[..n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn append(path: &Path, text: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn only_new_complete_lines_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        let mut tail = LogTail::new(&path);
        assert_eq!(tail.poll().unwrap(), None);

        append(&path, "11/09 20:00:00 one\r\n11/09 20:00:01 tw");
        let chunk = tail.poll().unwrap().unwrap();
        assert!(chunk.reset);
        assert_eq!(chunk.lines, ["11/09 20:00:00 one"]);

        assert!(tail.poll().unwrap().unwrap().lines.is_empty());
        append(&path, "o\n11/09 20:00:02 three\n");
        let chunk = tail.poll().unwrap().unwrap();
        assert!(!chunk.reset);
        assert_eq!(chunk.lines, ["11/09 20:00:01 two", "11/09 20:00:02 three"]);
    }

    #[test]
    fn truncation_and_replacement_start_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        let mut tail = LogTail::new(&path);
        append(&path, "11/09 20:00:00 first run\n11/09 20:00:01 more of it\n");
        tail.poll().unwrap();

        // Truncated: shorter than what was read.
        std::fs::write(&path, "11/09 21:00:00 second\n").unwrap();
        let chunk = tail.poll().unwrap().unwrap();
        assert!(chunk.reset);
        assert_eq!(chunk.lines, ["11/09 21:00:00 second"]);

        // Replaced by a longer file at once: the head tells.
        std::fs::write(
            &path,
            "11/09 22:00:00 third run, already longer than before\n11/09 22:00:01 x\n",
        )
        .unwrap();
        let chunk = tail.poll().unwrap().unwrap();
        assert!(chunk.reset);
        assert_eq!(chunk.lines.len(), 2);

        // Deleted, then back.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(tail.poll().unwrap(), None);
        append(&path, "11/09 23:00:00 fourth\n");
        let chunk = tail.poll().unwrap().unwrap();
        assert!(chunk.reset);
        assert_eq!(chunk.lines, ["11/09 23:00:00 fourth"]);
    }

    #[test]
    fn reads_are_bounded_and_lines_capped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        let long = "y".repeat(LINE_MAX * 3);
        let mut text = format!("{long}\n");
        let line = "11/09 20:00:00 a line of filler text for the bound\n";
        while text.len() < READ_MAX as usize * 2 {
            text.push_str(line);
        }
        std::fs::write(&path, &text).unwrap();
        let mut tail = LogTail::new(&path);
        let first = tail.poll().unwrap().unwrap();
        assert!(first.behind);
        assert_eq!(first.lines[0].len(), LINE_MAX);
        let mut total = first.lines.len();
        loop {
            let chunk = tail.poll().unwrap().unwrap();
            assert!(!chunk.reset);
            total += chunk.lines.len();
            if !chunk.behind {
                break;
            }
        }
        assert_eq!(total, text.lines().count());
    }

    #[test]
    fn a_big_file_resyncs_from_its_tail_on_a_line_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        let line = "11/09 20:00:00 filler filler filler filler filler\n";
        let count = (RESYNC_BYTES as usize / line.len()) + 100;
        std::fs::write(&path, line.repeat(count)).unwrap();
        let mut tail = LogTail::new(&path);
        let mut lines = Vec::new();
        loop {
            let chunk = tail.poll().unwrap().unwrap();
            lines.extend(chunk.lines);
            if !chunk.behind {
                break;
            }
        }
        assert!(lines.len() < count);
        assert!(lines.iter().all(|l| l == line.trim_end()));
    }
}
