//! The fuzzer's two folders: the corpus it mutates, with one folder for each target such as
//! `fuzz/corpus/fixture-panic/`, and the inputs it saves, as
//! `fuzz/regressions/<target>/<hash>.bin`.
//!
//! A saved input's name is the FNV-1a hash of its bytes, 64 bits written as 16 lowercase hex
//! digits. FNV-1a is the Fowler–Noll–Vo hash, which multiplies and mixes in one byte at a time.
//! The same input always gets the same name, so finding a failure again doesn't save a second
//! copy. The hash isn't cryptographic, and needn't be: it only names files.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::target::{self, Target};

/// A saved input, as [`saved`] reads it.
#[derive(Debug, Clone)]
pub(crate) struct Saved {
    /// The target it was saved for.
    pub(crate) target: &'static Target,
    /// Its target's name and its file's name, such as `fixture-panic/c6d0a3857a0d9820.bin`.
    pub(crate) name: String,
    /// Its bytes.
    pub(crate) input: Vec<u8>,
}

/// Reads the corpus in `folder`, one input to each file, in the order of the files' names. A
/// folder that doesn't exist or holds no file gives a corpus of one empty input.
///
/// # Errors
///
/// Returns [`Error::Corpus`] when the folder or one of its files can't be read.
pub(crate) fn corpus(folder: &Path) -> Result<Vec<Vec<u8>>, Error> {
    entries(folder)
        .and_then(|paths| paths.iter().map(fs::read).collect::<io::Result<Vec<_>>>())
        .map(|mut inputs| {
            if inputs.is_empty() {
                inputs.push(Vec::new());
            }
            inputs
        })
        .map_err(|source| Error::Corpus {
            path: folder.to_owned(),
            source,
        })
}

/// Saves `input` for the target named `target` in `regressions`, making the target's folder if it
/// doesn't exist, and returns the file's path.
///
/// # Errors
///
/// Returns [`Error::Save`] when the folder can't be made or the file can't be written.
pub(crate) fn save(regressions: &Path, target: &str, input: &[u8]) -> Result<PathBuf, Error> {
    let folder = regressions.join(target);
    let path = folder.join(file_name(input));
    fs::create_dir_all(&folder)
        .and_then(|()| fs::write(&path, input))
        .map(|()| path)
        .map_err(|source| Error::Save {
            path: folder,
            source,
        })
}

/// Reads every saved input in `regressions`, in the order of the targets' names and then of the
/// files' names. A folder that doesn't exist holds no saved input.
///
/// # Errors
///
/// Returns [`Error::UnknownFolder`] when `regressions` holds anything not named after a target,
/// and [`Error::Saved`] when a folder or a file in it can't be read.
pub(crate) fn saved(regressions: &Path) -> Result<Vec<Saved>, Error> {
    let unreadable = |path: &Path| {
        let path = path.to_owned();
        move |source| Error::Saved { path, source }
    };
    let mut saved = Vec::new();
    for folder in entries(regressions).map_err(unreadable(regressions))? {
        let target = folder
            .file_name()
            .and_then(OsStr::to_str)
            .and_then(target::find)
            .ok_or_else(|| Error::UnknownFolder {
                path: folder.clone(),
            })?;
        for path in entries(&folder).map_err(unreadable(&folder))? {
            let input = fs::read(&path).map_err(unreadable(&path))?;
            let file = path.file_name().unwrap_or_default().to_string_lossy();
            saved.push(Saved {
                target,
                name: format!("{}/{file}", target.name),
                input,
            });
        }
    }
    Ok(saved)
}

/// The name a saved input gets: the FNV-1a hash of its bytes, then `.bin`.
pub(crate) fn file_name(input: &[u8]) -> String {
    format!("{:016x}.bin", fnv1a(input))
}

/// The 64-bit FNV-1a hash of `bytes`.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The paths of the entries in `folder`, sorted, or none when `folder` doesn't exist. The order of
/// a folder's entries depends on the file system, so sorting keeps every run the same.
fn entries(folder: &Path) -> io::Result<Vec<PathBuf>> {
    match fs::read_dir(folder) {
        Ok(entries) => entries
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()
            .map(|mut paths| {
                paths.sort();
                paths
            }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    use super::{corpus, file_name, fnv1a, save, saved};
    use crate::error::Error;

    /// An empty folder of this test's own under the system's temporary folder.
    fn folder(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!(
            "ferrolua-fuzz-folder-{}-{name}",
            std::process::id()
        ));
        // The folder is left from an earlier run only if that run stopped early.
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn hashes_as_the_fnv_1a_reference_does() {
        // The test values of the FNV specification, at http://www.isthe.com/chongo/tech/comp/fnv/.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn names_an_input_after_its_hash() {
        assert_eq!(file_name(b"FUZZ"), "c6d0a3857a0d9820.bin");
        assert_eq!(file_name(b""), "cbf29ce484222325.bin");
    }

    #[test]
    fn reads_a_corpus_in_the_order_of_its_files_names() {
        let folder = folder("corpus");
        fs::write(folder.join("b.bin"), "second").unwrap();
        fs::write(folder.join("a.bin"), "first").unwrap();
        assert_eq!(
            corpus(&folder).unwrap(),
            [b"first".to_vec(), b"second".to_vec()]
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn gives_an_empty_input_for_a_missing_or_empty_corpus() {
        let folder = folder("empty-corpus");
        assert_eq!(corpus(&folder).unwrap(), [Vec::<u8>::new()]);
        assert_eq!(corpus(&folder.join("missing")).unwrap(), [Vec::<u8>::new()]);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_the_corpus_holds_a_folder() {
        let folder = folder("corpus-with-a-folder");
        fs::create_dir(folder.join("inner")).unwrap();
        assert!(matches!(corpus(&folder), Err(Error::Corpus { path, .. }) if path == folder));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_the_corpus_is_a_file() {
        let folder = folder("corpus-file");
        let file = folder.join("corpus");
        fs::write(&file, "not a folder").unwrap();
        assert!(matches!(corpus(&file), Err(Error::Corpus { path, .. }) if path == file));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn saves_an_input_under_its_hash_and_saves_it_again_in_place() {
        let folder = folder("save");
        let path = save(&folder, "fixture-panic", b"FUZZ").unwrap();
        assert_eq!(
            path,
            folder.join("fixture-panic").join("c6d0a3857a0d9820.bin")
        );
        assert_eq!(fs::read(&path).unwrap(), b"FUZZ");
        assert_eq!(save(&folder, "fixture-panic", b"FUZZ").unwrap(), path);
        assert_eq!(
            fs::read_dir(folder.join("fixture-panic")).unwrap().count(),
            1
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_the_input_cannot_be_saved() {
        let folder = folder("save-blocked");
        fs::write(folder.join("fixture-panic"), "a file where the folder goes").unwrap();
        let error = save(&folder, "fixture-panic", b"FUZZ").unwrap_err();
        assert!(matches!(error, Error::Save { path, .. } if path == folder.join("fixture-panic")));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn reads_every_saved_input_in_order() {
        let folder = folder("saved");
        save(&folder, "fixture-panic", b"FUZZ").unwrap();
        save(&folder, "fixture-panic", b"").unwrap();
        save(&folder, "fixture-hang", &[0]).unwrap();
        let saved: Vec<_> = saved(&folder)
            .unwrap()
            .into_iter()
            .map(|saved| (saved.target.name, saved.name, saved.input))
            .collect();
        assert_eq!(
            saved,
            [
                (
                    "fixture-hang",
                    "fixture-hang/af63bd4c8601b7df.bin".to_owned(),
                    vec![0]
                ),
                (
                    "fixture-panic",
                    "fixture-panic/c6d0a3857a0d9820.bin".to_owned(),
                    b"FUZZ".to_vec()
                ),
                (
                    "fixture-panic",
                    "fixture-panic/cbf29ce484222325.bin".to_owned(),
                    Vec::new()
                ),
            ]
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn reads_no_saved_input_from_a_missing_folder() {
        let folder = folder("no-saved");
        assert!(saved(&folder.join("regressions")).unwrap().is_empty());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_the_saved_inputs_are_a_file() {
        let folder = folder("saved-file");
        let file = folder.join("regressions");
        fs::write(&file, "not a folder").unwrap();
        assert!(matches!(saved(&file), Err(Error::Saved { path, .. }) if path == file));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_a_folder_is_named_after_no_target() {
        let folder = folder("saved-unknown");
        fs::create_dir(folder.join("parser")).unwrap();
        let error = saved(&folder).unwrap_err();
        assert!(matches!(error, Error::UnknownFolder { path } if path == folder.join("parser")));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_a_targets_saved_inputs_are_a_file() {
        let folder = folder("saved-target-file");
        let file = folder.join("fixture-panic");
        fs::write(&file, "not a folder").unwrap();
        assert!(matches!(saved(&folder), Err(Error::Saved { path, .. }) if path == file));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn fails_when_a_saved_input_cannot_be_read() {
        let folder = folder("saved-input-folder");
        let inner = folder.join("fixture-panic").join("inner");
        fs::create_dir_all(&inner).unwrap();
        assert!(matches!(saved(&folder), Err(Error::Saved { path, .. }) if path == inner));
        fs::remove_dir_all(folder).unwrap();
    }
}
