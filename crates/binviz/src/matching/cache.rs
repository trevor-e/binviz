//! Scores kept between runs: a function scored once from some bytes of an
//! object against some bytes of the original gives the same score again, so
//! a batch that rescores objects that haven't changed (a search's variants,
//! a project's untouched units) reads them from disk. The key is a hash of
//! everything the score depends on: the original (its fingerprint), the
//! function's extent in it, the names the notes give (relocations are
//! checked against them), and the object function's code and relocations.
//! Correct by construction: a changed input is another key.
//!
//! The cache lives in `$BINVIZ_CACHE`, else `~/.cache/binviz/scores`
//! (`%LOCALAPPDATA%\binviz\scores` on Windows), one small JSON file per
//! score; delete the folder to clear it.

use std::path::{Path, PathBuf};

use super::{FunctionScore, ObjectFunction};
use crate::binary::Binary;

/// A folder of scores (see the module's description).
pub struct ScoreCache {
    dir: PathBuf,
}

/// A score's key: 64 bits of FNV-1a over what it depends on, as hex.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(String);

impl ScoreCache {
    /// The cache at `dir`, or the default place (`$BINVIZ_CACHE`, else the
    /// user's cache folder); None when there is no such place.
    pub fn open(dir: Option<&Path>) -> Option<ScoreCache> {
        let dir = match dir {
            Some(d) => d.to_path_buf(),
            None => default_dir()?,
        };
        std::fs::create_dir_all(&dir).ok()?;
        Some(ScoreCache { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The key for scoring the object's `func` against the original's
    /// function at `address` in `bin`.
    pub fn key(&self, bin: &Binary, address: u64, func: &ObjectFunction) -> Key {
        let mut h = Hasher::default();
        h.eat(bin.summary().fingerprint.as_bytes());
        h.eat(&address.to_le_bytes());
        let extent = bin
            .symbols()
            .function_containing(address)
            .map_or((0, 0), |f| (f.address, f.size));
        h.eat(&extent.0.to_le_bytes());
        h.eat(&extent.1.to_le_bytes());
        // The names the notes give: relocations are checked against where they point.
        for a in bin.annotations() {
            if !a.name.is_empty() {
                h.eat(&a.address.to_le_bytes());
                h.eat(a.name.as_bytes());
            }
        }
        h.eat(format!("{:?}", func.isa).as_bytes());
        h.eat(&func.code);
        for (at, r) in &func.relocs {
            h.eat(&at.to_le_bytes());
            h.eat(format!("{r:?}").as_bytes());
        }
        Key(format!("{:016x}", h.0))
    }

    pub fn get(&self, key: &Key) -> Option<FunctionScore> {
        let text = std::fs::read_to_string(self.path(key)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn put(&self, key: &Key, score: &FunctionScore) {
        if let Ok(text) = serde_json::to_string(score) {
            // Write then rename: a reader never sees half a file.
            let tmp = self.dir.join(format!("{}.{}.tmp", key.0, std::process::id()));
            if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, self.path(key)).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    fn path(&self, key: &Key) -> PathBuf {
        self.dir.join(format!("{}.json", key.0))
    }
}

fn default_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("BINVIZ_CACHE") {
        return Some(PathBuf::from(d));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?
    } else if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(x)
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))?
    };
    Some(base.join("binviz").join("scores"))
}

#[derive(Default)]
struct Hasher(u64);

impl Hasher {
    fn eat(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf2_9ce4_8422_2325;
        }
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // A separator, so that "ab"+"c" and "a"+"bc" differ.
        self.0 ^= 0xFF;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
}
