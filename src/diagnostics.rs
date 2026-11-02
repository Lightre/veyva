use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 1_048_576;
const MAX_MESSAGE_BYTES: usize = 16_384;

pub struct Logger {
    path: PathBuf,
}
impl Logger {
    pub fn open() -> io::Result<(Self, PathBuf)> {
        let root = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::other("LOCALAPPDATA is unavailable"))?;
        let dir = PathBuf::from(root).join("Veyva");
        fs::create_dir_all(&dir)?;
        let path = dir.join("veyva.log");
        // Keep no log handle across calls: another logger may rotate the file.
        OpenOptions::new().create(true).append(true).open(&path)?;
        Ok((Self { path: path.clone() }, path))
    }
    pub fn write(&mut self, message: &str) -> io::Result<()> {
        // Serialize rotation and append across threads and CLI/desktop processes.
        // Closing this handle releases the OS lock, including on an error path.
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.path.with_extension("log.lock"))?;
        lock.lock()?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut end = message.len().min(MAX_MESSAGE_BYTES);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        let suffix = if end < message.len() {
            " [truncated]"
        } else {
            ""
        };
        let line = format!("{timestamp} {}{suffix}\n", &message[..end]);
        let size = match fs::metadata(&self.path) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        };
        if size > 0 && size.saturating_add(line.len() as u64) > MAX_LOG_BYTES {
            rotate(&self.path, size)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        println!("{}{suffix}", &message[..end]);
        Ok(())
    }
}

fn rotate(path: &Path, size: u64) -> io::Result<()> {
    let previous = path.with_file_name("veyva.previous.log");
    // Migrate an oversized legacy log while retaining only its newest full lines.
    if size > MAX_LOG_BYTES {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::End(-(MAX_LOG_BYTES as i64)))?;
        let mut tail = Vec::with_capacity(MAX_LOG_BYTES as usize);
        file.take(MAX_LOG_BYTES).read_to_end(&mut tail)?;
        let start = tail
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(tail.len(), |n| n + 1);
        fs::write(&previous, &tail[start..])?;
        fs::remove_file(path)?;
    } else {
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(path, previous)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Logs(PathBuf);
    impl Logs {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "veyva-logs-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn logger(&self) -> Logger {
            Logger {
                path: self.0.join("veyva.log"),
            }
        }
    }
    impl Drop for Logs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn open_loggers_follow_rotation_and_replace_the_single_backup() {
        let logs = Logs::new();
        let mut first = logs.logger();
        let mut second = logs.logger();
        fs::write(&first.path, vec![b'x'; MAX_LOG_BYTES as usize - 5]).unwrap();
        second.write("rotation one").unwrap();
        first
            .write("first logger follows the new active file")
            .unwrap();
        let active = fs::read_to_string(&first.path).unwrap();
        assert!(active.contains("rotation one"));
        assert!(active.contains("first logger follows"));
        assert!(
            fs::metadata(logs.0.join("veyva.previous.log"))
                .unwrap()
                .len()
                <= MAX_LOG_BYTES
        );
        fs::write(&first.path, vec![b'y'; MAX_LOG_BYTES as usize]).unwrap();
        first.write("rotation two").unwrap();
        assert_eq!(
            fs::read(logs.0.join("veyva.previous.log")).unwrap()[0],
            b'y'
        );
        assert!(fs::metadata(&first.path).unwrap().len() <= MAX_LOG_BYTES);
    }

    #[test]
    fn concurrent_writers_serialize_rotation_without_losing_entries() {
        let logs = Logs::new();
        let path = logs.logger().path;
        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize]).unwrap();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for index in 0..8 {
                let path = path.clone();
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut logger = Logger { path };
                    barrier.wait();
                    logger.write(&format!("concurrent writer {index}")).unwrap();
                });
            }
        });
        let active = fs::read_to_string(&path).unwrap();
        for index in 0..8 {
            assert!(active.contains(&format!("concurrent writer {index}\n")));
        }
        assert!(
            fs::metadata(logs.0.join("veyva.previous.log"))
                .unwrap()
                .len()
                <= MAX_LOG_BYTES
        );
    }

    #[test]
    fn oversized_messages_are_bounded_without_splitting_utf8() {
        let logs = Logs::new();
        let mut logger = logs.logger();
        logger.write(&"€".repeat(MAX_MESSAGE_BYTES)).unwrap();
        let text = fs::read_to_string(&logger.path).unwrap();
        assert!(text.ends_with(" [truncated]\n"));
        assert!(text.len() < MAX_MESSAGE_BYTES + 64);
    }

    #[test]
    fn oversized_legacy_log_keeps_a_bounded_recent_tail() {
        let logs = Logs::new();
        let mut logger = logs.logger();
        let mut old = vec![b'x'; MAX_LOG_BYTES as usize + 100];
        old.extend_from_slice(b"\nrecent legacy entry\n");
        fs::write(&logger.path, old).unwrap();
        logger.write("new entry").unwrap();
        let previous = fs::read_to_string(logs.0.join("veyva.previous.log")).unwrap();
        assert_eq!(previous, "recent legacy entry\n");
        assert!(fs::metadata(&logger.path).unwrap().len() <= MAX_LOG_BYTES);
    }
}
