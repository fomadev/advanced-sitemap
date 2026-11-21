//! URL-level snapshots of a publication and the diff between two of them
//! (phase B of the lifecycle design, ADR-0010 and ADR-0011).
//!
//! A snapshot lists, for every published URL, its `loc`, a 128-bit digest of
//! its encoded `<url>` block and its partition number, sorted by `loc`. It
//! contains no timestamp and no machine-dependent data.
//!
//! * [`SnapshotBuilder`] accepts records in any order and sorts them with an
//!   external merge sort: memory stays below a configured budget whatever the
//!   number of URLs (invariant I8).
//! * [`SnapshotReader`] streams records back and checks the integrity trailer
//!   (record count and a BLAKE3 digest of the whole file) at the end.
//! * [`diff`] merges two sorted streams in constant memory.
//!
//! # File format (version 1)
//!
//! ```text
//! magic        8 bytes  "ASNAPv1\n"
//! config      16 bytes  digest of the settings that shape the output
//! records      *        loc length (u16 LE, >= 1) | loc | content (16) | partition (u32 LE)
//! end          2 bytes  0x0000 (a loc length of zero)
//! count        8 bytes  number of records (u64 LE)
//! checksum    32 bytes  BLAKE3 of every preceding byte
//! ```
//!
//! Records are sorted by `loc` bytes, then content, then partition.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A 128-bit digest: the first 16 bytes of BLAKE3 (ADR-0011).
pub type Digest = [u8; 16];

/// BLAKE3 truncated to 128 bits. Frozen: persisted snapshots depend on it.
pub fn digest128(bytes: &[u8]) -> Digest {
    let hash = blake3::hash(bytes);
    let mut out = [0u8; 16];
    out.copy_from_slice(&hash.as_bytes()[..16]);
    out
}

/// Lowercase hexadecimal form of a digest.
pub fn to_hex(digest: &Digest) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

const MAGIC: &[u8; 8] = b"ASNAPv1\n";
/// Largest `loc` length the format can store, in bytes.
pub const MAX_LOC_BYTES: usize = u16::MAX as usize;

/// One published URL.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Record {
    /// Serialized URL (sort key).
    pub loc: String,
    /// Digest of the encoded `<url>` block.
    pub content: Digest,
    /// 1-based partition number.
    pub partition: u32,
}

impl Record {
    /// Approximate memory used by the record while sorting.
    fn memory(&self) -> usize {
        self.loc.len() + std::mem::size_of::<Record>()
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn write_record(out: &mut impl Write, record: &Record) -> io::Result<()> {
    if record.loc.is_empty() || record.loc.len() > MAX_LOC_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "loc must have 1 to 65535 bytes",
        ));
    }
    out.write_all(&(record.loc.len() as u16).to_le_bytes())?;
    out.write_all(record.loc.as_bytes())?;
    out.write_all(&record.content)?;
    out.write_all(&record.partition.to_le_bytes())
}

/// Reads one record; `Ok(None)` at the end marker.
fn read_record(input: &mut impl Read) -> io::Result<Option<Record>> {
    let mut len = [0u8; 2];
    input.read_exact(&mut len)?;
    let len = u16::from_le_bytes(len) as usize;
    if len == 0 {
        return Ok(None);
    }
    let mut loc = vec![0u8; len];
    input.read_exact(&mut loc)?;
    let mut content = [0u8; 16];
    input.read_exact(&mut content)?;
    let mut partition = [0u8; 4];
    input.read_exact(&mut partition)?;
    Ok(Some(Record {
        loc: String::from_utf8(loc).map_err(|_| invalid("loc is not UTF-8"))?,
        content,
        partition: u32::from_le_bytes(partition),
    }))
}

// ---------------------------------------------------------------------------
// Writing

/// Writes sorted records with the header and the integrity trailer.
struct SnapshotWriter<W: Write> {
    out: W,
    hasher: blake3::Hasher,
    count: u64,
    last: Option<Record>,
}

impl<W: Write> SnapshotWriter<W> {
    fn new(mut out: W, config: &Digest) -> io::Result<Self> {
        let mut hasher = blake3::Hasher::new();
        let mut header = Vec::with_capacity(24);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(config);
        out.write_all(&header)?;
        hasher.update(&header);
        Ok(Self {
            out,
            hasher,
            count: 0,
            last: None,
        })
    }

    fn push(&mut self, record: Record) -> io::Result<()> {
        if self.last.as_ref().is_some_and(|last| *last > record) {
            return Err(io::Error::other("snapshot records are not sorted"));
        }
        let mut bytes = Vec::with_capacity(record.loc.len() + 22);
        write_record(&mut bytes, &record)?;
        self.out.write_all(&bytes)?;
        self.hasher.update(&bytes);
        self.count += 1;
        self.last = Some(record);
        Ok(())
    }

    fn finish(mut self) -> io::Result<(W, u64)> {
        let mut tail = Vec::with_capacity(10);
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&self.count.to_le_bytes());
        self.hasher.update(&tail);
        self.out.write_all(&tail)?;
        self.out.write_all(self.hasher.finalize().as_bytes())?;
        self.out.flush()?;
        Ok((self.out, self.count))
    }
}

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Collects records in any order and writes them sorted, spilling sorted
/// runs to temporary files when the memory budget is reached.
pub struct SnapshotBuilder {
    temp_dir: PathBuf,
    budget: usize,
    buffer: Vec<Record>,
    buffered: usize,
    runs: Vec<PathBuf>,
    count: u64,
}

impl SnapshotBuilder {
    /// A builder keeping at most about `budget` bytes of records in memory.
    /// Sorted runs go to unique files in `temp_dir`, removed at the end.
    pub fn new(temp_dir: impl Into<PathBuf>, budget: usize) -> Self {
        Self {
            temp_dir: temp_dir.into(),
            budget: budget.max(1),
            buffer: Vec::new(),
            buffered: 0,
            runs: Vec::new(),
            count: 0,
        }
    }

    /// Records added so far.
    pub fn len(&self) -> u64 {
        self.count
    }

    /// `true` when no record was added.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Number of sorted runs spilled to disk so far.
    pub fn spilled_runs(&self) -> usize {
        self.runs.len()
    }

    /// Adds a record.
    pub fn push(&mut self, record: Record) -> io::Result<()> {
        if record.loc.is_empty() || record.loc.len() > MAX_LOC_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "loc must have 1 to 65535 bytes",
            ));
        }
        self.buffered += record.memory();
        self.buffer.push(record);
        self.count += 1;
        if self.buffered >= self.budget {
            self.spill()?;
        }
        Ok(())
    }

    fn spill(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        self.buffer.sort_unstable();
        fs::create_dir_all(&self.temp_dir)?;
        let n = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = self
            .temp_dir
            .join(format!(".snapshot-run-{}-{n}", std::process::id()));
        let mut out = BufWriter::new(File::create_new(&path)?);
        self.runs.push(path);
        for record in self.buffer.drain(..) {
            write_record(&mut out, &record)?;
        }
        out.write_all(&0u16.to_le_bytes())?;
        out.flush()?;
        self.buffered = 0;
        Ok(())
    }

    /// Writes the sorted snapshot to `out` and returns the record count.
    pub fn finish(mut self, config: &Digest, out: impl Write) -> io::Result<u64> {
        let mut writer = SnapshotWriter::new(out, config)?;
        if self.runs.is_empty() {
            self.buffer.sort_unstable();
            for record in std::mem::take(&mut self.buffer) {
                writer.push(record)?;
            }
        } else {
            self.spill()?;
            let mut readers = Vec::with_capacity(self.runs.len());
            for path in &self.runs {
                readers.push(BufReader::with_capacity(64 * 1024, File::open(path)?));
            }
            // k-way merge; ties are broken by run index, but equal records
            // are identical, so the output does not depend on it.
            let mut heap = BinaryHeap::with_capacity(readers.len());
            for (i, reader) in readers.iter_mut().enumerate() {
                if let Some(record) = read_record(reader)? {
                    heap.push(Reverse((record, i)));
                }
            }
            while let Some(Reverse((record, i))) = heap.pop() {
                writer.push(record)?;
                if let Some(next) = read_record(&mut readers[i])? {
                    heap.push(Reverse((next, i)));
                }
            }
        }
        let (_, count) = writer.finish()?;
        Ok(count)
    }
}

impl Drop for SnapshotBuilder {
    fn drop(&mut self) {
        for path in &self.runs {
            let _ = fs::remove_file(path);
        }
    }
}

/// Writes a snapshot file atomically: into `<path>.tmp`, synced, then renamed.
pub fn write_file(builder: SnapshotBuilder, config: &Digest, path: &Path) -> io::Result<u64> {
    let temp = path.with_extension("tmp");
    let file = File::create(&temp)?;
    let mut out = BufWriter::new(file);
    let count = builder.finish(config, &mut out)?;
    let file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp, path)?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// Reading

/// Hashes everything read through it.
struct HashingReader<R> {
    inner: R,
    hasher: blake3::Hasher,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

/// Streams the records of a snapshot. The integrity trailer is checked when
/// the last record has been read: until then, records may come from a
/// truncated or damaged file, so callers must read to the end (the iterator
/// then returns an error for a damaged file).
pub struct SnapshotReader<R: Read> {
    input: HashingReader<R>,
    config: Digest,
    count: u64,
    last: Option<Record>,
    done: bool,
}

impl<R: Read> SnapshotReader<R> {
    /// Reads and checks the header.
    pub fn new(inner: R) -> io::Result<Self> {
        let mut input = HashingReader {
            inner,
            hasher: blake3::Hasher::new(),
        };
        let mut magic = [0u8; 8];
        input
            .read_exact(&mut magic)
            .map_err(|_| invalid("not a snapshot (too short)"))?;
        if &magic != MAGIC {
            return Err(invalid("not a snapshot, or an unsupported version"));
        }
        let mut config = [0u8; 16];
        input.read_exact(&mut config)?;
        Ok(Self {
            input,
            config,
            count: 0,
            last: None,
            done: false,
        })
    }

    /// Digest of the configuration recorded in the header.
    pub fn config(&self) -> &Digest {
        &self.config
    }

    fn next_record(&mut self) -> io::Result<Option<Record>> {
        if self.done {
            return Ok(None);
        }
        let truncated = |e: io::Error| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                invalid("snapshot is truncated")
            } else {
                e
            }
        };
        match read_record(&mut self.input).map_err(truncated)? {
            Some(record) => {
                if self.last.as_ref().is_some_and(|last| *last > record) {
                    return Err(invalid("snapshot records are not sorted"));
                }
                self.count += 1;
                self.last = Some(record.clone());
                Ok(Some(record))
            }
            None => {
                self.done = true;
                let mut count = [0u8; 8];
                self.input.read_exact(&mut count).map_err(truncated)?;
                let expected = self.input.hasher.finalize();
                let mut checksum = [0u8; 32];
                self.input
                    .inner
                    .read_exact(&mut checksum)
                    .map_err(truncated)?;
                if u64::from_le_bytes(count) != self.count {
                    return Err(invalid("snapshot record count does not match"));
                }
                if checksum != *expected.as_bytes() {
                    return Err(invalid("snapshot checksum does not match"));
                }
                Ok(None)
            }
        }
    }
}

impl<R: Read> Iterator for SnapshotReader<R> {
    type Item = io::Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_record().transpose()
    }
}

/// Opens a snapshot file; `Ok(None)` when it does not exist.
pub fn open_file(path: &Path) -> io::Result<Option<SnapshotReader<BufReader<File>>>> {
    match File::open(path) {
        Ok(file) => SnapshotReader::new(BufReader::with_capacity(256 * 1024, file)).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

// ---------------------------------------------------------------------------
// Diff

/// Differences between two snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// URLs only in the new snapshot.
    pub added: u64,
    /// URLs only in the old snapshot.
    pub removed: u64,
    /// URLs whose encoded `<url>` block changed.
    pub modified: u64,
    /// URLs with the same content that changed partition.
    pub moved: u64,
    /// URLs identical in both.
    pub unchanged: u64,
    /// Partitions (of either snapshot) that gained, lost or changed a URL.
    pub partitions_touched: BTreeSet<u32>,
    /// First added URLs, in `loc` order, at most `limit`.
    pub added_urls: Vec<String>,
    /// First removed URLs.
    pub removed_urls: Vec<String>,
    /// First modified URLs.
    pub modified_urls: Vec<String>,
    /// At least one list was cut at `limit`.
    pub truncated: bool,
}

impl ChangeSet {
    /// `true` when nothing differs.
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0 && self.modified == 0 && self.moved == 0
    }

    fn list(list: &mut Vec<String>, loc: String, limit: usize, truncated: &mut bool) {
        if list.len() < limit {
            list.push(loc);
        } else {
            *truncated = true;
        }
    }
}

/// Compares two sorted record streams in constant memory. URL lists keep at
/// most `limit` entries each; counts are always exact.
pub fn diff(
    old: impl IntoIterator<Item = io::Result<Record>>,
    new: impl IntoIterator<Item = io::Result<Record>>,
    limit: usize,
) -> io::Result<ChangeSet> {
    let mut old = old.into_iter();
    let mut new = new.into_iter();
    let mut a = old.next().transpose()?;
    let mut b = new.next().transpose()?;
    let mut set = ChangeSet::default();
    loop {
        match (a.take(), b.take()) {
            (None, None) => break,
            (Some(x), None) => {
                set.removed += 1;
                set.partitions_touched.insert(x.partition);
                ChangeSet::list(&mut set.removed_urls, x.loc, limit, &mut set.truncated);
                a = old.next().transpose()?;
            }
            (None, Some(y)) => {
                set.added += 1;
                set.partitions_touched.insert(y.partition);
                ChangeSet::list(&mut set.added_urls, y.loc, limit, &mut set.truncated);
                b = new.next().transpose()?;
            }
            (Some(x), Some(y)) => match x.loc.cmp(&y.loc) {
                std::cmp::Ordering::Less => {
                    set.removed += 1;
                    set.partitions_touched.insert(x.partition);
                    ChangeSet::list(&mut set.removed_urls, x.loc, limit, &mut set.truncated);
                    a = old.next().transpose()?;
                    b = Some(y);
                }
                std::cmp::Ordering::Greater => {
                    set.added += 1;
                    set.partitions_touched.insert(y.partition);
                    ChangeSet::list(&mut set.added_urls, y.loc, limit, &mut set.truncated);
                    a = Some(x);
                    b = new.next().transpose()?;
                }
                std::cmp::Ordering::Equal => {
                    if x.content != y.content {
                        set.modified += 1;
                        set.partitions_touched.insert(x.partition);
                        set.partitions_touched.insert(y.partition);
                        ChangeSet::list(&mut set.modified_urls, y.loc, limit, &mut set.truncated);
                    } else if x.partition != y.partition {
                        set.moved += 1;
                        set.partitions_touched.insert(x.partition);
                        set.partitions_touched.insert(y.partition);
                    } else {
                        set.unchanged += 1;
                    }
                    a = old.next().transpose()?;
                    b = new.next().transpose()?;
                }
            },
        }
    }
    Ok(set)
}
