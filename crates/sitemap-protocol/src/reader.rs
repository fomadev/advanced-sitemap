use std::fmt;
use std::io::{self, BufRead, Read};

use sitemap_model::RawUrlRecord;

use crate::ProtocolError;

/// Default maximum length of a JSONL line (1 MiB).
pub const DEFAULT_MAX_LINE_BYTES: usize = 1024 * 1024;
/// Default maximum size of a JSON document loaded in memory (256 MiB).
pub const DEFAULT_MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;

/// Why a record could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Malformed JSON, wrong type or unknown field.
    Json(String),
    /// The line exceeds the configured maximum length; it was skipped.
    LineTooLong {
        /// Limit in bytes.
        limit: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Json(message) => f.write_str(message),
            DecodeError::LineTooLong { limit } => {
                write!(f, "line exceeds the {limit}-byte limit and was skipped")
            }
        }
    }
}

/// One item read from a source.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRecord {
    /// 1-based record number.
    pub record: u64,
    /// 1-based line number, for line-oriented sources.
    pub line: Option<u64>,
    /// The decoded record, or why it could not be decoded.
    pub result: Result<RawUrlRecord, DecodeError>,
}

/// Streams records from JSON Lines: one JSON object per line. Blank lines are
/// ignored, a UTF-8 byte order mark and `\r\n` line endings are accepted.
/// Memory use is bounded by the maximum line length.
pub struct JsonlReader<R> {
    reader: R,
    max_line_bytes: usize,
    line: u64,
    record: u64,
    buffer: Vec<u8>,
}

impl<R: BufRead> JsonlReader<R> {
    /// A reader with the default line limit.
    pub fn new(reader: R) -> Self {
        Self::with_max_line_bytes(reader, DEFAULT_MAX_LINE_BYTES)
    }

    /// A reader with a custom line limit.
    pub fn with_max_line_bytes(reader: R, max_line_bytes: usize) -> Self {
        Self {
            reader,
            max_line_bytes,
            line: 0,
            record: 0,
            buffer: Vec::with_capacity(4096),
        }
    }

    /// Reads the next line into `self.buffer`. Returns `None` at end of input
    /// and `Some(true)` if the line was truncated because it is too long.
    fn read_line(&mut self) -> io::Result<Option<bool>> {
        self.buffer.clear();
        let mut too_long = false;
        let mut read_any = false;
        loop {
            let available = match self.reader.fill_buf() {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if available.is_empty() {
                return Ok(read_any.then_some(too_long));
            }
            read_any = true;
            let (chunk, consumed, done) = match available.iter().position(|&b| b == b'\n') {
                Some(i) => (&available[..i], i + 1, true),
                None => (available, available.len(), false),
            };
            if !too_long {
                if self.buffer.len() + chunk.len() > self.max_line_bytes {
                    too_long = true;
                    self.buffer.clear();
                } else {
                    self.buffer.extend_from_slice(chunk);
                }
            }
            self.reader.consume(consumed);
            if done {
                return Ok(Some(too_long));
            }
        }
    }
}

impl<R: BufRead> Iterator for JsonlReader<R> {
    type Item = io::Result<SourceRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let too_long = match self.read_line() {
                Ok(Some(too_long)) => too_long,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            self.line += 1;

            if too_long {
                self.record += 1;
                return Some(Ok(SourceRecord {
                    record: self.record,
                    line: Some(self.line),
                    result: Err(DecodeError::LineTooLong {
                        limit: self.max_line_bytes,
                    }),
                }));
            }

            let mut bytes = self.buffer.as_slice();
            if self.line == 1 {
                bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
            }
            bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
            if bytes.iter().all(u8::is_ascii_whitespace) {
                continue;
            }

            self.record += 1;
            let result = serde_json::from_slice::<RawUrlRecord>(bytes)
                .map_err(|error| DecodeError::Json(error.to_string()));
            return Some(Ok(SourceRecord {
                record: self.record,
                line: Some(self.line),
                result,
            }));
        }
    }
}

/// Reads records from a JSON array held in memory. Each element is decoded
/// separately, so one malformed record does not hide the others.
pub struct JsonArrayReader {
    values: std::vec::IntoIter<serde_json::Value>,
    record: u64,
}

impl JsonArrayReader {
    /// Reads a whole JSON array document, refusing documents above `max_bytes`.
    pub fn from_reader(reader: impl Read, max_bytes: u64) -> Result<Self, ProtocolError> {
        let mut bytes = Vec::new();
        reader.take(max_bytes + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max_bytes {
            return Err(ProtocolError::TooLarge { limit: max_bytes });
        }
        let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        let values: Vec<serde_json::Value> = serde_json::from_slice(text).map_err(|e| {
            ProtocolError::InvalidDocument(format!("expected a JSON array of URL records: {e}"))
        })?;
        Ok(Self::from_values(values))
    }

    /// Wraps already-parsed values (for example the `urls` of a configuration).
    pub fn from_values(values: Vec<serde_json::Value>) -> Self {
        Self {
            values: values.into_iter(),
            record: 0,
        }
    }

    /// Number of remaining records.
    pub fn remaining(&self) -> usize {
        self.values.len()
    }
}

impl Iterator for JsonArrayReader {
    type Item = io::Result<SourceRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.values.next()?;
        self.record += 1;
        let result = serde_json::from_value::<RawUrlRecord>(value)
            .map_err(|error| DecodeError::Json(error.to_string()));
        Some(Ok(SourceRecord {
            record: self.record,
            line: None,
            result,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: &str, max: usize) -> Vec<SourceRecord> {
        JsonlReader::with_max_line_bytes(input.as_bytes(), max)
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn reads_jsonl_with_line_numbers() {
        let input = "\u{FEFF}{\"loc\":\"https://example.com/a\"}\r\n\n  \n{\"url\":\"https://example.com/b\",\"priority\":0.5}\n{\"loc\":";
        let records = collect(input, 1024);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].line, Some(1));
        assert_eq!(
            records[0].result.as_ref().unwrap().loc.as_deref(),
            Some("https://example.com/a")
        );
        assert_eq!(records[1].record, 2);
        assert_eq!(records[1].line, Some(4));
        assert_eq!(records[1].result.as_ref().unwrap().priority, Some(0.5));
        assert_eq!(records[2].line, Some(5));
        assert!(matches!(records[2].result, Err(DecodeError::Json(_))));
    }

    #[test]
    fn skips_overlong_lines_and_continues() {
        let long = format!("{{\"loc\":\"https://example.com/{}\"}}", "a".repeat(100));
        let input = format!("{long}\n{{\"loc\":\"https://example.com/ok\"}}\n");
        // A tiny BufReader capacity forces the line to span many fill_buf calls.
        let reader = io::BufReader::with_capacity(8, input.as_bytes());
        let records: Vec<_> = JsonlReader::with_max_line_bytes(reader, 64)
            .map(Result::unwrap)
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0].result,
            Err(DecodeError::LineTooLong { limit: 64 })
        );
        assert_eq!(
            records[1].result.as_ref().unwrap().loc.as_deref(),
            Some("https://example.com/ok")
        );
        assert_eq!(records[1].line, Some(2));
    }

    #[test]
    fn rejects_unknown_fields_per_record() {
        let records = collect("{\"loc\":\"https://e.com/\",\"lastMod\":\"2026\"}\n", 1024);
        let Err(DecodeError::Json(message)) = &records[0].result else {
            panic!("expected a decode error");
        };
        assert!(message.contains("lastMod"), "{message}");
    }

    #[test]
    fn json_array_decodes_each_element() {
        let doc =
            br#"[{"loc":"https://example.com/"}, {"loc": 3}, {"loc":"https://example.com/b"}]"#;
        let records: Vec<_> = JsonArrayReader::from_reader(&doc[..], 1024)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(records.len(), 3);
        assert!(records[0].result.is_ok());
        assert!(records[1].result.is_err());
        assert_eq!(records[2].record, 3);
        assert!(matches!(
            JsonArrayReader::from_reader(&doc[..], 10),
            Err(ProtocolError::TooLarge { limit: 10 })
        ));
        assert!(JsonArrayReader::from_reader(&b"{}"[..], 1024).is_err());
    }
}
