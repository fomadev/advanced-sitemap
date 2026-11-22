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

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// A safe artifact file name: 1 to 128 characters from `[A-Za-z0-9._-]`, not
/// starting with a dot. Names can therefore never escape the output directory.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactName(String);

/// The name is not a safe artifact name.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Publication {
    /// Name used when staging.
    pub staged: ArtifactName,
    /// Public name.
    pub target: ArtifactName,
}

impl Publication {
    /// Moves the artifact staged as `staged` to `target`.
    pub fn new(staged: ArtifactName, target: ArtifactName) -> Self {
        Self { staged, target }
    }
}

/// Description of a store, for reports.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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

    /// Asks the store to compare new artifacts with the published file of the
    /// same name while they are written, and to avoid writing them at all
    /// when they turn out identical. A hint: the default ignores it.
    fn set_compare_with_published(&mut self, enabled: bool) {
        let _ = enabled;
    }

    /// `true` when the finished staged artifact `staged` has exactly the same
    /// bytes as the published artifact `target`. Used to skip republishing
    /// unchanged files. The default (`false`) is always safe: the artifact is
    /// then republished.
    fn staged_matches_published(
        &self,
        staged: &ArtifactName,
        target: &ArtifactName,
    ) -> io::Result<bool> {
        let _ = (staged, target);
        Ok(false)
    }

    /// Takes exclusive use of the destination for the rest of the run, so
    /// that two processes never publish to it at the same time. The engine
    /// calls it before reading or writing anything. Returns an error of kind
    /// [`io::ErrorKind::WouldBlock`] when another process holds it. The
    /// default does nothing (stores private to one process).
    fn lock(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Describes the store and its guarantees.
    fn describe(&self) -> StoreDescription;
}

// ---------------------------------------------------------------------------
// Directory lock
// ---------------------------------------------------------------------------

/// An exclusive lock on a directory, held by an open lock file.
///
/// It uses the operating system's advisory file locks, which are released
/// when the process exits for any reason: a crash never leaves a stale lock.
/// Only processes that take the lock are excluded; other programs writing
/// to the directory are not.
#[derive(Debug)]
pub struct DirectoryLock {
    _file: File,
    path: PathBuf,
}

impl DirectoryLock {
    /// Takes the lock `<dir>/<name>` without waiting, creating the directory
    /// and the file if needed. Fails with [`io::ErrorKind::WouldBlock`] when
    /// another process (or another handle) holds it.
    pub fn acquire(dir: &Path, name: &str) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file, path }),
            Err(fs::TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "{} is locked by another advanced-sitemap process; retry when it has finished",
                    dir.display()
                ),
            )),
            Err(fs::TryLockError::Error(error)) => Err(error),
        }
    }

    /// Path of the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Name of the lock file [`FsStore`] keeps in its output directory.
pub const LOCK_FILE: &str = ".advanced-sitemap.lock";

/// Makes the renames done in `dir` durable (Unix: `fsync` of the directory).
/// On Windows, directory entries cannot be synced this way; this does
/// nothing there.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
    Ok(())
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
    compare: bool,
    /// Staged artifacts found identical to the published file of the same
    /// name while being written: no staging file exists for them.
    identical: Arc<Mutex<HashSet<ArtifactName>>>,
    /// Held from `lock` until the store is dropped (declared last: it is
    /// released after the staging directory is cleaned up).
    lock: Option<DirectoryLock>,
}

impl FsStore {
    /// A store writing into `root` (created on first write if missing).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            staging: None,
            staged: Vec::new(),
            compare: false,
            identical: Arc::new(Mutex::new(HashSet::new())),
            lock: None,
        }
    }

    /// Removes staging directories left by processes that stopped before
    /// publishing or discarding. Only called with the lock held: every
    /// process that writes here takes it first, so these are all stale.
    fn remove_stale_staging(&self) -> io::Result<usize> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let stale = name.to_str().is_some_and(|n| n.starts_with(STAGING_PREFIX))
                && entry.file_type()?.is_dir()
                && self.staging.as_deref() != Some(entry.path().as_path());
            if stale {
                fs::remove_dir_all(entry.path())?;
                removed += 1;
            }
        }
        Ok(removed)
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
        lock_set(&self.identical)?.clear();
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

const WRITE_BUFFER: usize = 256 * 1024;

struct FsWriter {
    state: WriterState,
    bytes: u64,
}

enum WriterState {
    /// Writing the staging file.
    Writing(BufWriter<File>),
    /// Every byte so far equals the published file: nothing written yet.
    Comparing(Box<Comparison>),
}

struct Comparison {
    name: ArtifactName,
    published: BufReader<File>,
    published_path: PathBuf,
    staged_path: PathBuf,
    scratch: Vec<u8>,
    identical: Arc<Mutex<HashSet<ArtifactName>>>,
}

impl FsWriter {
    /// Leaves comparison mode: creates the staging file and copies the
    /// `matched` bytes already known to equal the published file.
    fn diverge(&mut self) -> io::Result<()> {
        let WriterState::Comparing(comparison) = &self.state else {
            return Ok(());
        };
        let file = File::create_new(&comparison.staged_path)?;
        let mut writer = BufWriter::with_capacity(WRITE_BUFFER, file);
        let mut prefix = File::open(&comparison.published_path)?.take(self.bytes);
        let copied = io::copy(&mut prefix, &mut writer)?;
        if copied != self.bytes {
            return Err(io::Error::other(
                "published file changed while it was being compared",
            ));
        }
        self.state = WriterState::Writing(writer);
        Ok(())
    }
}

impl Write for FsWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let WriterState::Comparing(comparison) = &mut self.state {
            comparison.scratch.resize(buf.len(), 0);
            let n = read_full(&mut comparison.published, &mut comparison.scratch)?;
            if n == buf.len() && comparison.scratch[..] == *buf {
                self.bytes += buf.len() as u64;
                return Ok(buf.len());
            }
            self.diverge()?;
        }
        let WriterState::Writing(writer) = &mut self.state else {
            return Err(io::Error::other("writer is not writing"));
        };
        let written = writer.write(buf)?;
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.state {
            WriterState::Writing(writer) => writer.flush(),
            WriterState::Comparing(_) => Ok(()),
        }
    }
}

impl ArtifactWriter for FsWriter {
    fn finish(mut self: Box<Self>) -> io::Result<u64> {
        if let WriterState::Comparing(comparison) = &mut self.state {
            let mut probe = [0u8; 1];
            if read_full(&mut comparison.published, &mut probe)? == 0 {
                // Same bytes and same length: no staging file is needed.
                lock_set(&comparison.identical)?.insert(comparison.name.clone());
                return Ok(self.bytes);
            }
            // The published file is longer: the new artifact is a prefix.
            self.diverge()?;
        }
        let bytes = self.bytes;
        if let WriterState::Writing(writer) = self.state {
            // Durability is ensured in `publish`, right before the rename, so
            // that staged files which are never published cost no `fsync`.
            writer
                .into_inner()
                .map_err(io::IntoInnerError::into_error)?;
        }
        Ok(bytes)
    }
}

fn lock_set(
    set: &Mutex<HashSet<ArtifactName>>,
) -> io::Result<MutexGuard<'_, HashSet<ArtifactName>>> {
    set.lock()
        .map_err(|_| io::Error::other("identical-artifact set poisoned"))
}

impl ArtifactStore for FsStore {
    fn create(&mut self, name: &ArtifactName) -> io::Result<Box<dyn ArtifactWriter>> {
        let dir = self.staging_dir()?;
        let staged_path = dir.join(name.as_str());
        if self.staged.contains(name) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("artifact {name} is already staged"),
            ));
        }
        let published_path = self.root.join(name.as_str());
        let published = if self.compare {
            match File::open(&published_path) {
                Ok(file) if file.metadata()?.is_file() => Some(file),
                Ok(_) => None,
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        let state = match published {
            Some(file) => WriterState::Comparing(Box::new(Comparison {
                name: name.clone(),
                published: BufReader::with_capacity(WRITE_BUFFER, file),
                published_path,
                staged_path,
                scratch: Vec::new(),
                identical: Arc::clone(&self.identical),
            })),
            None => WriterState::Writing(BufWriter::with_capacity(
                WRITE_BUFFER,
                File::create_new(staged_path)?,
            )),
        };
        self.staged.push(name.clone());
        Ok(Box::new(FsWriter { state, bytes: 0 }))
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
        let identical = lock_set(&self.identical)?.clone();
        for step in plan {
            let staged = dir.join(step.staged.as_str());
            if identical.contains(&step.staged) {
                if step.staged == step.target {
                    // Already published with these exact bytes.
                    continue;
                }
                // Same bytes as the published file of the staged name:
                // materialize it under the new name.
                fs::copy(self.root.join(step.staged.as_str()), &staged)?;
            }
            // Flush the data to disk before it becomes visible under its
            // public name (write access is required to sync on Windows).
            fs::OpenOptions::new()
                .write(true)
                .open(&staged)?
                .sync_all()?;
            fs::rename(staged, self.root.join(step.target.as_str()))?;
        }
        if !plan.is_empty() {
            // Make the renames themselves durable (Unix).
            sync_dir(&self.root)?;
        }
        // Remaining staged files (not in the plan) and the staging directory.
        self.cleanup_staging()
    }

    fn discard(&mut self) -> io::Result<()> {
        self.cleanup_staging()
    }

    fn set_compare_with_published(&mut self, enabled: bool) {
        self.compare = enabled;
    }

    /// Takes `<root>/.advanced-sitemap.lock`, then removes staging
    /// directories left by interrupted runs. Calling it again is a no-op.
    fn lock(&mut self) -> io::Result<()> {
        if self.lock.is_none() {
            self.lock = Some(DirectoryLock::acquire(&self.root, LOCK_FILE)?);
            self.remove_stale_staging()?;
        }
        Ok(())
    }

    fn staged_matches_published(
        &self,
        staged: &ArtifactName,
        target: &ArtifactName,
    ) -> io::Result<bool> {
        let Some(dir) = &self.staging else {
            return Ok(false);
        };
        if !self.staged.contains(staged) {
            return Ok(false);
        }
        if lock_set(&self.identical)?.contains(staged) {
            if staged == target {
                return Ok(true);
            }
            // Identical to the published file of the staged name; compare that
            // file with the target.
            let a = self.root.join(staged.as_str());
            let b = self.root.join(target.as_str());
            return match (File::open(a), File::open(b)) {
                (Ok(a), Ok(b)) => same_bytes(a, b),
                _ => Ok(false),
            };
        }
        let published = self.root.join(target.as_str());
        let published_len = match fs::metadata(&published) {
            Ok(meta) if meta.is_file() => meta.len(),
            Ok(_) => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let staged_path = dir.join(staged.as_str());
        if fs::metadata(&staged_path)?.len() != published_len {
            return Ok(false);
        }
        same_bytes(File::open(staged_path)?, File::open(published)?)
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

/// Compares two readers byte for byte, in fixed-size chunks.
fn same_bytes(a: impl Read, b: impl Read) -> io::Result<bool> {
    const CHUNK: usize = 64 * 1024;
    let mut a = BufReader::with_capacity(CHUNK, a);
    let mut b = BufReader::with_capacity(CHUNK, b);
    let mut left = vec![0u8; CHUNK];
    let mut right = vec![0u8; CHUNK];
    loop {
        let n = read_full(&mut a, &mut left)?;
        let m = read_full(&mut b, &mut right)?;
        if n != m || left[..n] != right[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

/// Fills `buf` unless the end of input is reached; returns the bytes read.
fn read_full(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
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

    fn staged_matches_published(
        &self,
        staged: &ArtifactName,
        target: &ArtifactName,
    ) -> io::Result<bool> {
        let inner = lock(&self.inner)?;
        Ok(
            match (inner.staged.get(staged), inner.published.get(target)) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            },
        )
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

    fn stage(store: &mut dyn ArtifactStore, name: &str, bytes: &[u8]) -> ArtifactName {
        let name = ArtifactName::new(name).unwrap();
        let mut writer = store.create(&name).unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap();
        name
    }

    fn check_matching(store: &mut dyn ArtifactStore) {
        let a = stage(store, "a.xml", b"same content");
        store
            .publish(&[Publication {
                staged: a.clone(),
                target: a.clone(),
            }])
            .unwrap();
        let same = stage(store, "a.xml", b"same content");
        let longer = stage(store, "b.xml", b"same content!");
        let other = stage(store, "c.xml", b"same contenT");
        let target = ArtifactName::new("a.xml").unwrap();
        let missing = ArtifactName::new("missing.xml").unwrap();
        assert!(store.staged_matches_published(&same, &target).unwrap());
        assert!(!store.staged_matches_published(&longer, &target).unwrap());
        assert!(!store.staged_matches_published(&other, &target).unwrap());
        assert!(!store.staged_matches_published(&same, &missing).unwrap());
        assert!(!store.staged_matches_published(&missing, &target).unwrap());
        store.discard().unwrap();
    }

    #[test]
    fn staged_files_are_compared_byte_for_byte() {
        let tmp = TempDir::new("compare");
        check_matching(&mut FsStore::new(&tmp.0));
        check_matching(&mut MemoryStore::new());
        let mut null = NullStore::new();
        let name = stage(&mut null, "a.xml", b"x");
        assert!(!null.staged_matches_published(&name, &name).unwrap());
    }

    fn publish_same(store: &mut FsStore, name: &str, bytes: &[u8]) {
        let staged = stage(store, name, bytes);
        store
            .publish(&[Publication {
                staged: staged.clone(),
                target: staged,
            }])
            .unwrap();
    }

    #[test]
    fn compare_mode_writes_nothing_for_identical_files() {
        let tmp = TempDir::new("compare-mode");
        let mut store = FsStore::new(&tmp.0);
        let body = b"<urlset>abcdefghij</urlset>
"
        .repeat(20_000);
        publish_same(&mut store, "same.xml", &body);
        publish_same(&mut store, "middle.xml", &body);
        publish_same(&mut store, "longer.xml", &body);
        publish_same(&mut store, "shorter.xml", &body[..body.len() - 10]);

        store.set_compare_with_published(true);
        let same = stage(&mut store, "same.xml", &body);
        let mut changed = body.clone();
        changed[body.len() / 2] = b'#';
        let middle = stage(&mut store, "middle.xml", &changed);
        let longer = stage(&mut store, "longer.xml", &body[..body.len() - 10]);
        let shorter = stage(&mut store, "shorter.xml", &body);
        let fresh = stage(&mut store, "fresh.xml", b"new");

        let staging = store.staging.clone().unwrap();
        assert!(
            !staging.join("same.xml").exists(),
            "identical file was written"
        );
        assert!(store.staged_matches_published(&same, &same).unwrap());
        for (name, expected) in [
            (&middle, &changed[..]),
            (&longer, &body[..body.len() - 10]),
            (&shorter, &body[..]),
            (&fresh, &b"new"[..]),
        ] {
            assert!(
                !store.staged_matches_published(name, name).unwrap(),
                "{name}"
            );
            assert_eq!(
                fs::read(staging.join(name.as_str())).unwrap(),
                expected,
                "{name}"
            );
        }

        let plan: Vec<Publication> = [&same, &middle, &longer, &shorter, &fresh]
            .into_iter()
            .map(|n| Publication {
                staged: n.clone(),
                target: n.clone(),
            })
            .collect();
        store.publish(&plan).unwrap();
        assert_eq!(fs::read(tmp.0.join("same.xml")).unwrap(), body);
        assert_eq!(fs::read(tmp.0.join("middle.xml")).unwrap(), changed);
        assert_eq!(fs::read(tmp.0.join("shorter.xml")).unwrap(), body);
        assert!(
            !entries(&tmp.0)
                .iter()
                .any(|n| n.starts_with(STAGING_PREFIX))
        );
    }

    #[test]
    fn compare_mode_materializes_identical_files_under_a_new_name() {
        let tmp = TempDir::new("compare-rename");
        let mut store = FsStore::new(&tmp.0);
        publish_same(&mut store, "sitemap-1.xml", b"content");
        store.set_compare_with_published(true);
        let staged = stage(&mut store, "sitemap-1.xml", b"content");
        let target = ArtifactName::new("sitemap.xml").unwrap();
        assert!(!store.staged_matches_published(&staged, &target).unwrap());
        store.publish(&[Publication { staged, target }]).unwrap();
        assert_eq!(fs::read(tmp.0.join("sitemap.xml")).unwrap(), b"content");
        assert_eq!(fs::read(tmp.0.join("sitemap-1.xml")).unwrap(), b"content");
    }

    #[test]
    fn large_files_are_compared_across_chunks() {
        let tmp = TempDir::new("compare-large");
        let mut store = FsStore::new(&tmp.0);
        let mut big = vec![b'x'; 300_000];
        let a = stage(&mut store, "big.xml", &big);
        store
            .publish(&[Publication {
                staged: a.clone(),
                target: a.clone(),
            }])
            .unwrap();
        let same = stage(&mut store, "big.xml", &big);
        assert!(store.staged_matches_published(&same, &a).unwrap());
        store.discard().unwrap();
        big[299_999] = b'y';
        let changed = stage(&mut store, "big.xml", &big);
        assert!(!store.staged_matches_published(&changed, &a).unwrap());
        store.discard().unwrap();
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

    #[test]
    fn lock_excludes_a_second_store_until_the_first_is_dropped() {
        let tmp = TempDir::new("lock");
        let mut first = FsStore::new(&tmp.0);
        first.lock().unwrap();
        first.lock().unwrap(); // idempotent
        let mut second = FsStore::new(&tmp.0);
        let error = second.lock().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("locked"), "{error}");
        drop(first);
        second.lock().unwrap();
        // The lock file is hidden: never an artifact name.
        assert!(ArtifactName::new(LOCK_FILE).is_err());
    }

    #[test]
    fn lock_removes_staging_left_by_an_interrupted_run() {
        let tmp = TempDir::new("stale");
        let stale = tmp.0.join(format!("{STAGING_PREFIX}99999-0"));
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("sitemap-1.xml"), b"partial").unwrap();
        fs::write(tmp.0.join("sitemap.xml"), b"published").unwrap();
        let mut store = FsStore::new(&tmp.0);
        store.lock().unwrap();
        assert!(!stale.exists());
        assert_eq!(fs::read(tmp.0.join("sitemap.xml")).unwrap(), b"published");
        // A store's own staging directory is kept until it publishes.
        let mut writer = store.create(&name("a.xml")).unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
        store
            .publish(&[Publication {
                staged: name("a.xml"),
                target: name("a.xml"),
            }])
            .unwrap();
        assert_eq!(fs::read(tmp.0.join("a.xml")).unwrap(), b"x");
    }

    #[test]
    fn directory_lock_reports_its_path() {
        let tmp = TempDir::new("dirlock");
        let lock = DirectoryLock::acquire(&tmp.0.join("state"), "sitemap.lock").unwrap();
        assert!(lock.path().ends_with("sitemap.lock"));
        assert_eq!(
            DirectoryLock::acquire(&tmp.0.join("state"), "sitemap.lock")
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        sync_dir(&tmp.0).unwrap();
    }
}
