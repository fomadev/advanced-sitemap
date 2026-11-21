//! Artifact stores (ADR-0007).
//!
//! The engine never writes final files directly. It asks a store for staged
//! artifacts, then either publishes them in a given order (child sitemaps
//! first, index next, manifest last) or discards them. Stores never delete a
//! published artifact.
//!
//! Guarantees of the bundled stores:
//!
//! | Store | Staging | Per-artifact publication | Whole-set publication |
//! |---|---|---|---|
//! | [`FsStore`] | hidden directory inside the output directory | atomic rename on the same volume | not atomic, ordered |
//! | [`MemoryStore`] | in memory | atomic | atomic |

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// A safe artifact file name: 1 to 128 characters from `[A-Za-z0-9._-]`, not
/// starting with a dot. Names can therefore never escape the output directory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactName(String);

/// The name is not a safe artifact name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidArtifactName(pub String);

impl fmt::Display for InvalidArtifactName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid artifact name {:?}: use 1-128 characters from [A-Za-z0-9._-], not starting with '.'",
            self.0
        )
    }
}

impl std::error::Error for InvalidArtifactName {}

impl ArtifactName {
    /// Validates a name.
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidArtifactName> {
        let name = name.into();
        let valid = !name.is_empty()
            && name.len() <= 128
            && !name.starts_with('.')
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if valid {
            Ok(Self(name))
        } else {
            Err(InvalidArtifactName(name))
        }
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A staged artifact being written.
pub trait ArtifactWriter: Write {
    /// Flushes and durably stores the staged content; returns its size in bytes.
    fn finish(self: Box<Self>) -> io::Result<u64>;
}

/// One step of a publication: move a staged artifact to its public name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    /// Name used when staging.
    pub staged: ArtifactName,
    /// Public name.
    pub target: ArtifactName,
}

/// Description of a store, for reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreDescription {
    /// Store kind, e.g. `local-fs` or `memory`.
    pub kind: &'static str,
    /// Human-readable location.
    pub location: String,
    /// Whether each artifact is published atomically.
    pub atomic_per_artifact: bool,
    /// Whether the whole set is published atomically.
    pub atomic_set: bool,
}

/// A destination for generated artifacts.
pub trait ArtifactStore {
    /// Starts a staged artifact. Staging the same name twice is an error.
    fn create(&mut self, name: &ArtifactName) -> io::Result<Box<dyn ArtifactWriter>>;

    /// Reads a published artifact, if it exists.
    fn read_published(&self, name: &ArtifactName) -> io::Result<Option<Vec<u8>>>;

    /// Publishes staged artifacts in order. Staged artifacts that are not part
    /// of `plan` are discarded.
    fn publish(&mut self, plan: &[Publication]) -> io::Result<()>;

    /// Discards every staged artifact. Published artifacts are untouched.
    fn discard(&mut self) -> io::Result<()>;

    /// Describes the store and its guarantees.
    fn describe(&self) -> StoreDescription;
}

// ---------------------------------------------------------------------------
// Local file system
// ---------------------------------------------------------------------------

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Prefix of staging directories created inside the output directory.
pub const STAGING_PREFIX: &str = ".advanced-sitemap-staging-";

/// Stores artifacts in a local directory.
///
/// Artifacts are staged in a hidden directory inside the output directory, so
/// the final `rename` stays on the same volume and is atomic per file. If the
/// store is dropped without `publish` or `discard`, staged files are removed.
pub struct FsStore {
    root: PathBuf,
    staging: Option<PathBuf>,
    staged: Vec<ArtifactName>,
}

impl FsStore {
    /// A store writing into `root` (created on first write if missing).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            staging: None,
            staged: Vec::new(),
        }
    }

    /// The output directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn staging_dir(&mut self) -> io::Result<PathBuf> {
        if let Some(dir) = &self.staging {
            return Ok(dir.clone());
        }
        fs::create_dir_all(&self.root)?;
        loop {
            let n = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = self
                .root
                .join(format!("{STAGING_PREFIX}{}-{n}", std::process::id()));
            match fs::create_dir(&dir) {
                Ok(()) => {
                    self.staging = Some(dir.clone());
                    return Ok(dir);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    fn cleanup_staging(&mut self) -> io::Result<()> {
        if let Some(dir) = self.staging.take() {
            for name in self.staged.drain(..) {
                match fs::remove_file(dir.join(name.as_str())) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            fs::remove_dir(&dir)?;
        }
        self.staged.clear();
        Ok(())
    }
}

struct FsWriter {
    inner: BufWriter<File>,
    bytes: u64,
}

impl Write for FsWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl ArtifactWriter for FsWriter {
    fn finish(self: Box<Self>) -> io::Result<u64> {
        let bytes = self.bytes;
        let file = self
            .inner
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        file.sync_all()?;
        Ok(bytes)
    }
}

impl ArtifactStore for FsStore {
    fn create(&mut self, name: &ArtifactName) -> io::Result<Box<dyn ArtifactWriter>> {
        let dir = self.staging_dir()?;
        let file = File::create_new(dir.join(name.as_str()))?;
        self.staged.push(name.clone());
        Ok(Box::new(FsWriter {
            inner: BufWriter::with_capacity(256 * 1024, file),
            bytes: 0,
        }))
    }

    fn read_published(&self, name: &ArtifactName) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.root.join(name.as_str())) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn publish(&mut self, plan: &[Publication]) -> io::Result<()> {
        let dir = match &self.staging {
            Some(dir) => dir.clone(),
            None if plan.is_empty() => return Ok(()),
            None => return Err(io::Error::other("nothing was staged")),
        };
        for step in plan {
            if !self.staged.contains(&step.staged) {
                return Err(io::Error::other(format!(
                    "artifact {} was not staged",
                    step.staged
                )));
            }
        }
        for step in plan {
            fs::rename(
                dir.join(step.staged.as_str()),
                self.root.join(step.target.as_str()),
            )?;
        }
        // Remaining staged files (not in the plan) and the staging directory.
        self.cleanup_staging()
    }

    fn discard(&mut self) -> io::Result<()> {
        self.cleanup_staging()
    }

    fn describe(&self) -> StoreDescription {
        StoreDescription {
            kind: "local-fs",
            location: self.root.display().to_string(),
            atomic_per_artifact: true,
            atomic_set: false,
        }
    }
}

impl Drop for FsStore {
    fn drop(&mut self) {
        // Best effort: never leave a staging directory behind.
        let _ = self.cleanup_staging();
    }
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MemoryInner {
    staged: BTreeMap<ArtifactName, Vec<u8>>,
    published: BTreeMap<ArtifactName, Vec<u8>>,
}

/// Stores artifacts in memory. Cloning yields a handle to the same storage,
/// which lets callers read results after handing the store to the engine.
#[derive(Clone, Default)]
pub struct MemoryStore {
    inner: Arc<Mutex<MemoryInner>>,
}

fn lock(inner: &Mutex<MemoryInner>) -> io::Result<MutexGuard<'_, MemoryInner>> {
    inner
        .lock()
        .map_err(|_| io::Error::other("memory store lock poisoned"))
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// A copy of the published artifacts, keyed by name.
    pub fn published(&self) -> io::Result<BTreeMap<String, Vec<u8>>> {
        Ok(lock(&self.inner)?
            .published
            .iter()
            .map(|(name, bytes)| (name.as_str().to_owned(), bytes.clone()))
            .collect())
    }

    /// Number of staged (unpublished) artifacts.
    pub fn staged_count(&self) -> io::Result<usize> {
        Ok(lock(&self.inner)?.staged.len())
    }

    /// Inserts a published artifact directly (useful to simulate a previous run).
    pub fn insert_published(&self, name: &ArtifactName, bytes: Vec<u8>) -> io::Result<()> {
        lock(&self.inner)?.published.insert(name.clone(), bytes);
        Ok(())
    }
}

struct MemoryWriter {
    name: ArtifactName,
    buffer: Vec<u8>,
    inner: Arc<Mutex<MemoryInner>>,
}

impl Write for MemoryWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ArtifactWriter for MemoryWriter {
    fn finish(self: Box<Self>) -> io::Result<u64> {
        let size = self.buffer.len() as u64;
        lock(&self.inner)?.staged.insert(self.name, self.buffer);
        Ok(size)
    }
}

impl ArtifactStore for MemoryStore {
    fn create(&mut self, name: &ArtifactName) -> io::Result<Box<dyn ArtifactWriter>> {
        let mut inner = lock(&self.inner)?;
        if inner.staged.contains_key(name) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("artifact {name} is already staged"),
            ));
        }
        // Reserve the name so a second `create` fails like on a file system.
        inner.staged.insert(name.clone(), Vec::new());
        Ok(Box::new(MemoryWriter {
            name: name.clone(),
            buffer: Vec::new(),
            inner: Arc::clone(&self.inner),
        }))
    }

    fn read_published(&self, name: &ArtifactName) -> io::Result<Option<Vec<u8>>> {
        Ok(lock(&self.inner)?.published.get(name).cloned())
    }

    fn publish(&mut self, plan: &[Publication]) -> io::Result<()> {
        let mut inner = lock(&self.inner)?;
        if let Some(missing) = plan.iter().find(|p| !inner.staged.contains_key(&p.staged)) {
            return Err(io::Error::other(format!(
                "artifact {} was not staged",
                missing.staged
            )));
        }
        let mut staged = std::mem::take(&mut inner.staged);
        for step in plan {
            if let Some(bytes) = staged.remove(&step.staged) {
                inner.published.insert(step.target.clone(), bytes);
            }
        }
        Ok(())
    }

    fn discard(&mut self) -> io::Result<()> {
        lock(&self.inner)?.staged.clear();
        Ok(())
    }

    fn describe(&self) -> StoreDescription {
        StoreDescription {
            kind: "memory",
            location: "memory".to_owned(),
            atomic_per_artifact: true,
            atomic_set: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Null
// ---------------------------------------------------------------------------

/// Accepts and measures writes, keeps nothing, publishes nothing. Used for
/// dry runs (`validate`) so that they execute exactly the generation pipeline.
#[derive(Clone, Debug, Default)]
pub struct NullStore {
    staged: Vec<ArtifactName>,
}

impl NullStore {
    /// A new null store.
    pub fn new() -> Self {
        Self::default()
    }
}

struct NullWriter {
    bytes: u64,
}

impl Write for NullWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ArtifactWriter for NullWriter {
    fn finish(self: Box<Self>) -> io::Result<u64> {
        Ok(self.bytes)
    }
}

impl ArtifactStore for NullStore {
    fn create(&mut self, name: &ArtifactName) -> io::Result<Box<dyn ArtifactWriter>> {
        if self.staged.contains(name) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("artifact {name} is already staged"),
            ));
        }
        self.staged.push(name.clone());
        Ok(Box::new(NullWriter { bytes: 0 }))
    }

    fn read_published(&self, _name: &ArtifactName) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn publish(&mut self, _plan: &[Publication]) -> io::Result<()> {
        self.staged.clear();
        Ok(())
    }

    fn discard(&mut self) -> io::Result<()> {
        self.staged.clear();
        Ok(())
    }

    fn describe(&self) -> StoreDescription {
        StoreDescription {
            kind: "null",
            location: "none (dry run)".to_owned(),
            atomic_per_artifact: true,
            atomic_set: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> ArtifactName {
        ArtifactName::new(s).unwrap()
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let n = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "advanced-sitemap-test-{label}-{}-{n}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn artifact_names_cannot_escape() {
        for bad in [
            "",
            "../x",
            "a/b",
            "a\\b",
            ".hidden",
            "c:x",
            "x y",
            &"a".repeat(129),
        ] {
            assert!(ArtifactName::new(bad).is_err(), "{bad}");
        }
        assert!(ArtifactName::new("sitemap-1.xml").is_ok());
    }

    #[test]
    fn fs_publish_moves_files_in_order_and_cleans_staging() {
        let tmp = TempDir::new("publish");
        let mut store = FsStore::new(&tmp.0);
        let mut w = store.create(&name("sitemap-1.xml")).unwrap();
        w.write_all(b"<urlset/>").unwrap();
        assert_eq!(w.finish().unwrap(), 9);
        let w = store.create(&name("unused.xml")).unwrap();
        w.finish().unwrap();
        assert!(
            store.create(&name("sitemap-1.xml")).is_err(),
            "duplicate staging"
        );

        // Nothing is visible before publication, apart from the staging directory.
        let before = entries(&tmp.0);
        assert_eq!(before.len(), 1);
        assert!(before[0].starts_with(STAGING_PREFIX));

        store
            .publish(&[Publication {
                staged: name("sitemap-1.xml"),
                target: name("sitemap.xml"),
            }])
            .unwrap();
        assert_eq!(entries(&tmp.0), ["sitemap.xml"]);
        assert_eq!(fs::read(tmp.0.join("sitemap.xml")).unwrap(), b"<urlset/>");
        assert_eq!(
            store.read_published(&name("sitemap.xml")).unwrap().unwrap(),
            b"<urlset/>"
        );
    }

    #[test]
    fn fs_publish_replaces_existing_and_discard_keeps_published() {
        let tmp = TempDir::new("replace");
        fs::write(tmp.0.join("sitemap.xml"), b"old").unwrap();
        fs::write(tmp.0.join("other.xml"), b"keep").unwrap();

        let mut store = FsStore::new(&tmp.0);
        let mut w = store.create(&name("sitemap.xml")).unwrap();
        w.write_all(b"new").unwrap();
        w.finish().unwrap();
        store.discard().unwrap();
        assert_eq!(fs::read(tmp.0.join("sitemap.xml")).unwrap(), b"old");
        assert_eq!(entries(&tmp.0), ["other.xml", "sitemap.xml"]);

        let mut store = FsStore::new(&tmp.0);
        let mut w = store.create(&name("sitemap.xml")).unwrap();
        w.write_all(b"new").unwrap();
        w.finish().unwrap();
        store
            .publish(&[Publication {
                staged: name("sitemap.xml"),
                target: name("sitemap.xml"),
            }])
            .unwrap();
        assert_eq!(fs::read(tmp.0.join("sitemap.xml")).unwrap(), b"new");
        assert_eq!(fs::read(tmp.0.join("other.xml")).unwrap(), b"keep");
    }

    #[test]
    fn fs_drop_without_publish_removes_staging() {
        let tmp = TempDir::new("drop");
        {
            let mut store = FsStore::new(&tmp.0);
            let w = store.create(&name("a.xml")).unwrap();
            w.finish().unwrap();
        }
        assert!(entries(&tmp.0).is_empty());
    }

    #[test]
    fn memory_store_publishes_and_discards() {
        let mut store = MemoryStore::new();
        let handle = store.clone();
        let mut w = store.create(&name("a.xml")).unwrap();
        w.write_all(b"A").unwrap();
        w.finish().unwrap();
        assert!(store.create(&name("a.xml")).is_err());
        assert_eq!(handle.staged_count().unwrap(), 1);
        store
            .publish(&[Publication {
                staged: name("a.xml"),
                target: name("b.xml"),
            }])
            .unwrap();
        assert_eq!(handle.published().unwrap()["b.xml"], b"A");
        assert_eq!(handle.staged_count().unwrap(), 0);

        let w = store.create(&name("c.xml")).unwrap();
        w.finish().unwrap();
        store.discard().unwrap();
        assert_eq!(handle.published().unwrap().len(), 1);
    }
}
