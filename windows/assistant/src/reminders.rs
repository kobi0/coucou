//! The reminders the assistant has set, kept in a small private file so they survive a restart. The app
//! holds a timer for the next one; when it is due the island shows a note. This is the Windows and Linux
//! counterpart of the local notifications the Mac uses: those platforms have no scheduling service the app
//! can ask, so the app keeps its own list and says so.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Most reminders kept at once. More are refused, so a runaway model cannot fill the disk.
pub const MAX_REMINDERS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: u64,
    /// Seconds since 1970.
    pub due: i64,
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderBook {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    items: Vec<Reminder>,
}

impl ReminderBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[Reminder] {
        &self.items
    }

    /// `None` when the book is full.
    pub fn add(&mut self, due: i64, title: &str) -> Option<u64> {
        if self.items.len() >= MAX_REMINDERS {
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.items.push(Reminder { id, due, title: title.to_string() });
        Some(id)
    }

    /// Removes and returns every reminder due at or before `now`, earliest first.
    pub fn take_due(&mut self, now: i64) -> Vec<Reminder> {
        let (mut due, keep): (Vec<Reminder>, Vec<Reminder>) = self.items.drain(..).partition(|r| r.due <= now);
        self.items = keep;
        due.sort_by_key(|r| (r.due, r.id));
        due
    }

    /// When the next reminder is due.
    pub fn next_due(&self) -> Option<i64> {
        self.items.iter().map(|r| r.due).min()
    }

    /// A missing or damaged file gives an empty book: reminders are best effort and must never stop the app.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else { return Self::new() };
        let mut book: Self = serde_json::from_slice(&bytes).unwrap_or_default();
        book.items.truncate(MAX_REMINDERS);
        let highest = book.items.iter().map(|r| r.id).max().unwrap_or(0);
        book.next_id = book.next_id.max(highest);
        book
    }

    /// Writes to a temporary file and renames it over the real one, so a crash cannot leave half a file.
    /// On Unix the file is readable by its owner only. Returns false on any failure.
    pub fn save(&self, path: &Path) -> bool {
        let attempt = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let json = serde_json::to_vec_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let mut temp = path.as_os_str().to_os_string();
            temp.push(".tmp");
            let temp = std::path::PathBuf::from(temp);
            let mut options = std::fs::OpenOptions::new();
            options.create(true).write(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut file = options.open(&temp)?;
            file.write_all(&json)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, path)
        };
        attempt().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("coucou-reminders-{}-{tag}", std::process::id())).join("reminders.json")
    }

    #[test]
    fn due_reminders_come_out_earliest_first() {
        let mut book = ReminderBook::new();
        book.add(300, "later").unwrap();
        book.add(100, "first").unwrap();
        book.add(200, "second").unwrap();
        assert_eq!(book.next_due(), Some(100));
        let due = book.take_due(250);
        assert_eq!(due.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), vec!["first", "second"]);
        assert_eq!(book.len(), 1);
        assert_eq!(book.next_due(), Some(300));
        assert!(book.take_due(250).is_empty(), "a reminder fires once");
        assert_eq!(book.take_due(300).len(), 1);
        assert!(book.is_empty() && book.next_due().is_none());
    }

    #[test]
    fn the_book_is_limited_and_ids_are_not_reused() {
        let mut book = ReminderBook::new();
        let first = book.add(1, "a").unwrap();
        book.take_due(5);
        let second = book.add(2, "b").unwrap();
        assert_ne!(first, second);
        for i in 0..(MAX_REMINDERS as i64 - 1) {
            assert!(book.add(i, "x").is_some());
        }
        assert_eq!(book.len(), MAX_REMINDERS);
        assert!(book.add(0, "one too many").is_none());
    }

    #[test]
    fn it_survives_a_restart() {
        let path = temp_path("restart");
        let mut book = ReminderBook::new();
        book.add(1_800_000_000, "Call Mum").unwrap();
        assert!(book.save(&path));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let back = ReminderBook::load(&path);
        assert_eq!(back, book);
        let mut back = back;
        assert_ne!(back.add(1, "next"), Some(1), "ids carry on after a restart");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_or_damaged_file_is_an_empty_book() {
        let path = temp_path("damaged");
        assert!(ReminderBook::load(&path).is_empty());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(ReminderBook::load(&path).is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_oversized_file_is_cut_down() {
        let path = temp_path("big");
        let items: Vec<Reminder> = (1..=(MAX_REMINDERS as u64 + 50)).map(|id| Reminder { id, due: id as i64, title: "x".into() }).collect();
        let big = ReminderBook { next_id: 0, items };
        assert!(big.save(&path));
        let back = ReminderBook::load(&path);
        assert_eq!(back.len(), MAX_REMINDERS);
        assert!(back.items().iter().map(|r| r.id).max().unwrap() <= MAX_REMINDERS as u64);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
