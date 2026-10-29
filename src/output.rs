//! Date-first clip names; publication still enforces non-overwrite semantics.
use std::path::{Path, PathBuf};

pub fn choose_path(folder: &Path, timestamp: [u16; 7], replay: bool) -> Result<PathBuf, String> {
    let [year, month, day, hour, minute, second, millis] = timestamp;
    let kind = if replay { "replay" } else { "record" };
    let stem = format!(
        "{year:04}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}-{millis:03}-{kind}"
    );
    // Bound collision probing. A concurrent publisher may still win the name;
    // the recording/export writer must retain its create-new/non-replace checks.
    for index in 1..=10_000 {
        let name = if index == 1 {
            format!("{stem}.mp4")
        } else {
            format!("{stem}-{index:02}.mp4")
        };
        let path = folder.join(name);
        let exists = |candidate: &Path| {
            candidate
                .try_exists()
                .map_err(|error| format!("Could not inspect output path: {error}"))
        };
        if !exists(&path)? && !exists(&path.with_extension("partial.mp4"))? {
            return Ok(path);
        }
    }
    Err("Could not select an unused clip name".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_lowercase_date_first_and_sort_chronologically() {
        let folder = std::env::temp_dir();
        let first = choose_path(&folder, [2098, 1, 2, 3, 4, 5, 6], false).unwrap();
        let next = choose_path(&folder, [2098, 1, 2, 3, 4, 6, 0], true).unwrap();
        assert_eq!(
            first.file_name().unwrap(),
            "2098-01-02_03-04-05-006-record.mp4"
        );
        assert_eq!(
            next.file_name().unwrap(),
            "2098-01-02_03-04-06-000-replay.mp4"
        );
        assert!(first < next);
    }
    #[test]
    fn existing_and_partial_outputs_are_preserved() {
        let folder = std::env::temp_dir().join(format!("veyva-naming-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let date = [2098, 1, 2, 3, 4, 5, 6];
        let first = choose_path(&folder, date, false).unwrap();
        std::fs::write(&first, b"original").unwrap();
        let second = choose_path(&folder, date, false).unwrap();
        assert!(second
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-record-02.mp4"));
        let partial = second.with_extension("partial.mp4");
        std::fs::write(&partial, b"pending").unwrap();
        let third = choose_path(&folder, date, false).unwrap();
        assert!(third
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-record-03.mp4"));
        assert_eq!(std::fs::read(&first).unwrap(), b"original");
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(partial).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
}
