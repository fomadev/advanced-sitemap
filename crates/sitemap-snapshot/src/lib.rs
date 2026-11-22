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
//! records      *        shared (u16 LE) | suffix length (u16 LE) | suffix | content (16) | partition (u32 LE)
//! end          2 bytes  0xFFFF (in place of `shared`)
//! count        8 bytes  number of records (u64 LE)
//! checksum    32 bytes  BLAKE3 of every preceding byte
//! ```
//!
//! Records are sorted by `loc` bytes, then content, then partition. Each
//! `loc` is stored as the number of leading bytes it shares with the previous
//! record's `loc` plus the remaining suffix: sorted URLs share long prefixes,
//! which makes the file several times smaller.

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

/// Digest of a sequence of `(loc, content)` records, fed in `loc` order.
///
/// Used by the delta mode to check a published partition against the
/// snapshot with constant memory: both sides are visited in `loc` order.
/// Each record is framed by the length of its `loc`, so different
/// sequences cannot produce the same input.
#[derive(Clone, Debug, Default)]
pub struct SequenceDigest {
    hasher: blake3::Hasher,
    records: u64,
}

impl SequenceDigest {
    /// Adds one record.
    pub fn update(&mut self, loc: &str, content: &Digest) {
        self.hasher.update(&(loc.len() as u64).to_le_bytes());
        self.hasher.update(loc.as_bytes());
        self.hasher.update(content);
        self.records += 1;
    }

    /// Records added so far.
    pub fn records(&self) -> u64 {
        self.records
    }

    /// The digest of the records added so far.
    pub fn finish(&self) -> Digest {
        let hash = self.hasher.finalize();
        let mut out = [0u8; 16];
        out.copy_from_slice(&hash.as_bytes()[..16]);
        out
    }
}

/// Lowercase hexadecimal form of a digest.
pub fn to_hex(digest: &Digest) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

const MAGIC: &[u8; 8] = b"ASNAPv1\n";
/// Bytes hashed and written, or read, per call.
const CHUNK: usize = 64 * 1024;
/// Largest `loc` length the format can store, in bytes.
pub const MAX_LOC_BYTES: usize = u16::MAX as usize - 1;
/// `shared` value that marks the end of the records.
const END: u16 = u16::MAX;

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

/// Encodes and decodes records with prefix compression against the previous
/// record of the same stream.
#[derive(Default)]
struct Codec {
    previous: Vec<u8>,
}

impl Codec {
    fn write(&mut self, out: &mut impl Write, record: &Record) -> io::Result<()> {
        let loc = record.loc.as_bytes();
        if loc.is_empty() || loc.len() > MAX_LOC_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "loc must have 1 to 65534 bytes",
            ));
        }
        let shared = self
            .previous
            .iter()
            .zip(loc)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = &loc[shared..];
        out.write_all(&(shared as u16).to_le_bytes())?;
        out.write_all(&(suffix.len() as u16).to_le_bytes())?;
        out.write_all(suffix)?;
        out.write_all(&record.content)?;
        out.write_all(&record.partition.to_le_bytes())?;
        self.previous.clear();
        self.previous.extend_from_slice(loc);
        Ok(())
    }

    fn write_end(out: &mut impl Write) -> io::Result<()> {
        out.write_all(&END.to_le_bytes())
    }

    /// Reads one record; `Ok(None)` at the end marker.
    fn read(&mut self, input: &mut impl Read) -> io::Result<Option<Record>> {
        let mut word = [0u8; 2];
        input.read_exact(&mut word)?;
        let shared = u16::from_le_bytes(word);
        if shared == END {
            return Ok(None);
        }
        let shared = shared as usize;
        if shared > self.previous.len() {
            return Err(invalid("snapshot prefix is longer than the previous loc"));
        }
        input.read_exact(&mut word)?;
        let suffix = u16::from_le_bytes(word) as usize;
        if shared + suffix == 0 || shared + suffix > MAX_LOC_BYTES {
            return Err(invalid("snapshot loc length is out of range"));
        }
        self.previous.truncate(shared);
        let start = self.previous.len();
        self.previous.resize(start + suffix, 0);
        input.read_exact(&mut self.previous[start..])?;
        let mut content = [0u8; 16];
        input.read_exact(&mut content)?;
        let mut partition = [0u8; 4];
        input.read_exact(&mut partition)?;
        let loc = std::str::from_utf8(&self.previous)
            .map_err(|_| invalid("loc is not UTF-8"))?
            .to_owned();
        Ok(Some(Record {
            loc,
            content,
            partition: u32::from_le_bytes(partition),
        }))
    }
}

// ---------------------------------------------------------------------------
// Writing

/// Writes sorted records with the header and the integrity trailer.
struct SnapshotWriter<W: Write> {
    out: W,
    hasher: blake3::Hasher,
    count: u64,
    last: Option<Record>,
    codec: Codec,
    bytes: Vec<u8>,
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
            codec: Codec::default(),
            bytes: Vec::with_capacity(CHUNK + 1024),
        })
    }

    fn push(&mut self, record: Record) -> io::Result<()> {
        if self.last.as_ref().is_some_and(|last| *last > record) {
            return Err(io::Error::other("snapshot records are not sorted"));
        }
        self.codec.write(&mut self.bytes, &record)?;
        if self.bytes.len() >= CHUNK {
            self.flush_chunk()?;
        }
        self.count += 1;
        self.last = Some(record);
        Ok(())
    }

    /// Hashes and writes the pending bytes in one call each: hashing record
    /// by record costs more than the hashing itself.
    fn flush_chunk(&mut self) -> io::Result<()> {
        self.out.write_all(&self.bytes)?;
        self.hasher.update(&self.bytes);
        self.bytes.clear();
        Ok(())
    }

    fn finish(mut self) -> io::Result<(W, u64)> {
        self.flush_chunk()?;
        let mut tail = Vec::with_capacity(10);
        Codec::write_end(&mut tail)?;
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
                "loc must have 1 to 65534 bytes",
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
        let mut codec = Codec::default();
        for record in self.buffer.drain(..) {
            codec.write(&mut out, &record)?;
        }
        Codec::write_end(&mut out)?;
        out.flush()?;
        self.buffered = 0;
        Ok(())
    }

    /// Writes the sorted snapshot to `out` and returns the record count.
    pub fn finish(self, config: &Digest, out: impl Write) -> io::Result<u64> {
        self.finish_merged(config, std::iter::empty(), out)
    }

    /// Like [`finish`], merging in `sorted`, a stream of records already in
    /// order (for example the unchanged part of a previous snapshot), so
    /// that they need not be sorted or spilled again. The records of
    /// `sorted` and of the builder must not share a `loc`.
    ///
    /// [`finish`]: SnapshotBuilder::finish
    pub fn finish_merged(
        mut self,
        config: &Digest,
        sorted: impl IntoIterator<Item = io::Result<Record>>,
        out: impl Write,
    ) -> io::Result<u64> {
        let mut writer = SnapshotWriter::new(out, config)?;
        let mut sources: Vec<Source<'_>> = Vec::with_capacity(self.runs.len() + 2);
        if self.runs.is_empty() {
            self.buffer.sort_unstable();
            sources.push(Source::Memory(std::mem::take(&mut self.buffer).into_iter()));
        } else {
            self.spill()?;
            for path in &self.runs {
                sources.push(Source::Run(
                    BufReader::with_capacity(64 * 1024, File::open(path)?),
                    Codec::default(),
                ));
            }
        }
        sources.push(Source::Sorted(Box::new(sorted.into_iter())));

        // k-way merge; ties are broken by source index, but equal records
        // are identical, so the output does not depend on it.
        let mut heap = BinaryHeap::with_capacity(sources.len());
        for (i, source) in sources.iter_mut().enumerate() {
            if let Some(record) = source.next()? {
                heap.push(Reverse((record, i)));
            }
        }
        while let Some(Reverse((record, i))) = heap.pop() {
            writer.push(record)?;
            if let Some(next) = sources[i].next()? {
                heap.push(Reverse((next, i)));
            }
        }
        let (_, count) = writer.finish()?;
        Ok(count)
    }
}

/// One input of the final merge.
enum Source<'a> {
    Memory(std::vec::IntoIter<Record>),
    Run(BufReader<File>, Codec),
    Sorted(Box<dyn Iterator<Item = io::Result<Record>> + 'a>),
}

impl Source<'_> {
    fn next(&mut self) -> io::Result<Option<Record>> {
        match self {
            Source::Memory(records) => Ok(records.next()),
            Source::Run(reader, codec) => codec.read(reader),
            Source::Sorted(records) => records.next().transpose(),
        }
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

/// Buffers its input and hashes every byte handed out, a whole buffer at a
/// time rather than record by record.
struct HashingReader<R> {
    inner: R,
    buffer: Box<[u8]>,
    /// Next byte to hand out.
    pos: usize,
    /// End of the valid bytes.
    len: usize,
    /// Bytes before this position are hashed.
    hashed: usize,
    hasher: blake3::Hasher,
}

impl<R: Read> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: vec![0; CHUNK].into_boxed_slice(),
            pos: 0,
            len: 0,
            hashed: 0,
            hasher: blake3::Hasher::new(),
        }
    }

    /// Hashes the bytes handed out since the last call.
    fn hash_consumed(&mut self) {
        self.hasher.update(&self.buffer[self.hashed..self.pos]);
        self.hashed = self.pos;
    }

    /// Digest of every byte handed out so far.
    fn digest(&mut self) -> blake3::Hash {
        self.hash_consumed();
        self.hasher.finalize()
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pos == self.len {
            self.hash_consumed();
            self.len = loop {
                match self.inner.read(&mut self.buffer) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    other => break other?,
                }
            };
            self.pos = 0;
            self.hashed = 0;
        }
        let n = out.len().min(self.len - self.pos);
        out[..n].copy_from_slice(&self.buffer[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Streams the records of a snapshot. The integrity trailer is checked when
/// the last record has been read: until then, records may come from a
/// truncated or damaged file, so callers must read to the end (the iterator
/// then returns an error for a damaged file).
pub struct SnapshotReader<R: Read> {
    input: HashingReader<R>,
    codec: Codec,
    config: Digest,
    count: u64,
    /// The previous record, to check the order without cloning it.
    last_loc: String,
    last_rest: Option<(Digest, u32)>,
    done: bool,
}

impl<R: Read> SnapshotReader<R> {
    /// Reads and checks the header.
    pub fn new(inner: R) -> io::Result<Self> {
        let mut input = HashingReader::new(inner);
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
            codec: Codec::default(),
            config,
            count: 0,
            last_loc: String::new(),
            last_rest: None,
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
        match self.codec.read(&mut self.input).map_err(truncated)? {
            Some(record) => {
                if let Some((content, partition)) = self.last_rest {
                    let last = (self.last_loc.as_str(), content, partition);
                    if last > (record.loc.as_str(), record.content, record.partition) {
                        return Err(invalid("snapshot records are not sorted"));
                    }
                }
                self.count += 1;
                self.last_loc.clone_from(&record.loc);
                self.last_rest = Some((record.content, record.partition));
                Ok(Some(record))
            }
            None => {
                self.done = true;
                let mut count = [0u8; 8];
                self.input.read_exact(&mut count).map_err(truncated)?;
                // The checksum covers everything before it. The checksum
                // bytes are hashed too once read, but no longer matter.
                let expected = self.input.digest();
                let mut checksum = [0u8; 32];
                self.input.read_exact(&mut checksum).map_err(truncated)?;
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
