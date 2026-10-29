use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Logger {
    file: File,
}
impl Logger {
    pub fn open() -> io::Result<(Self, PathBuf)> {
        let root = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::other("LOCALAPPDATA is unavailable"))?;
        let dir = PathBuf::from(root).join("Veyva");
        fs::create_dir_all(&dir)?;
        let path = dir.join("veyva.log");
        if fs::metadata(&path).is_ok_and(|m| m.len() > 1_048_576) {
            let old = dir.join("veyva.previous.log");
            if old.exists() {
                fs::remove_file(&old)?;
            }
            fs::rename(&path, old)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok((Self { file }, path))
    }
    pub fn write(&mut self, message: &str) -> io::Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        println!("{message}");
        writeln!(self.file, "{timestamp} {message}")?;
        self.file.flush()
    }
}
