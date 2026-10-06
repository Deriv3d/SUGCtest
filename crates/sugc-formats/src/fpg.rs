//! FPG archive: a flat, hash-addressed archive of zlib-compressed files.
//!
//! See `knowledge/formats/fpg.md`. Layout summary (all integers little-endian u32):
//!
//! - `0x000`: magic `30GF`, then the entry count, then zeros up to `0x800`.
//! - `0x800`: entry table, 16 bytes per entry: name hash, data offset, stored size,
//!   uncompressed size.
//! - Data area from the next 0x800 boundary: each entry's zlib stream, zero-padded to a
//!   multiple of 0x800, stored back to back in table order.

use std::fmt;
use std::io::{Read, Write};

/// File magic: the ASCII characters `3`, `0`, `G`, `F`.
pub const MAGIC: [u8; 4] = *b"30GF";
/// Offset of the entry table.
pub const TABLE_OFFSET: usize = 0x800;
/// Size of one entry table record.
pub const ENTRY_SIZE: usize = 16;
/// Alignment of the data area, of every entry offset and of every stored size.
pub const ALIGN: usize = 0x800;
/// zlib level used by the original archives (stock zlib reproduces their streams at this level).
pub const ZLIB_LEVEL: u32 = 6;

/// Errors from parsing or decompressing an FPG archive.
#[derive(Debug, PartialEq, Eq)]
pub enum FpgError {
    TooShort,
    BadMagic,
    TableOutOfBounds {
        count: u32,
    },
    EntryOutOfBounds {
        index: usize,
    },
    Decompress {
        index: usize,
        reason: String,
    },
    SizeMismatch {
        index: usize,
        expected: u32,
        actual: usize,
    },
}

impl fmt::Display for FpgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FpgError::TooShort => write!(f, "file shorter than the FPG header"),
            FpgError::BadMagic => write!(f, "not an FPG archive (bad magic)"),
            FpgError::TableOutOfBounds { count } => {
                write!(f, "entry table for {count} entries runs past end of file")
            }
            FpgError::EntryOutOfBounds { index } => {
                write!(f, "entry {index} data runs past end of file")
            }
            FpgError::Decompress { index, reason } => {
                write!(f, "entry {index} does not decompress: {reason}")
            }
            FpgError::SizeMismatch {
                index,
                expected,
                actual,
            } => write!(
                f,
                "entry {index} decompressed to {actual} bytes, table says {expected}"
            ),
        }
    }
}

impl std::error::Error for FpgError {}

/// One archive entry, kept exactly as stored so the archive can be rebuilt byte for byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// [`hash_name`] of the entry's file name. Names themselves are not stored.
    pub hash: u32,
    /// Uncompressed size from the table.
    pub size: u32,
    /// Offset the entry was found at (informational; the writer recomputes offsets).
    pub offset: u32,
    /// Stored bytes: the zlib stream plus its zero padding, exactly `stored size` long.
    pub stored: Vec<u8>,
}

impl Entry {
    /// Compress `data` into a new entry for the file `name`. Only the file name is hashed;
    /// directories are dropped, as the game does before every lookup.
    pub fn from_data(name: &str, data: &[u8]) -> Entry {
        Entry::from_data_with_hash(hash_name(base_name(name)), data)
    }

    /// Compress `data` into a new entry with an explicit hash.
    pub fn from_data_with_hash(hash: u32, data: &[u8]) -> Entry {
        let mut enc =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(ZLIB_LEVEL));
        enc.write_all(data).expect("writing to a Vec cannot fail");
        let mut stored = enc.finish().expect("writing to a Vec cannot fail");
        stored.resize(align_up(stored.len()), 0);
        Entry {
            hash,
            size: u32::try_from(data.len()).expect("entry larger than 4 GiB"),
            offset: 0,
            stored,
        }
    }

    /// Inflate the entry and check the result against the table's uncompressed size.
    pub fn decompress(&self, index: usize) -> Result<Vec<u8>, FpgError> {
        let mut out = Vec::with_capacity(self.size as usize);
        flate2::read::ZlibDecoder::new(&self.stored[..])
            .read_to_end(&mut out)
            .map_err(|e| FpgError::Decompress {
                index,
                reason: e.to_string(),
            })?;
        if out.len() != self.size as usize {
            return Err(FpgError::SizeMismatch {
                index,
                expected: self.size,
                actual: out.len(),
            });
        }
        Ok(out)
    }
}

/// A parsed FPG archive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Archive {
    pub entries: Vec<Entry>,
}

impl Archive {
    /// Parse an archive from its bytes. Entry data is copied, not inflated.
    pub fn parse(bytes: &[u8]) -> Result<Archive, FpgError> {
        if bytes.len() < TABLE_OFFSET {
            return Err(FpgError::TooShort);
        }
        if bytes[0..4] != MAGIC {
            return Err(FpgError::BadMagic);
        }
        let count = read_u32(bytes, 4);
        let table_end = (count as usize)
            .checked_mul(ENTRY_SIZE)
            .and_then(|n| n.checked_add(TABLE_OFFSET))
            .filter(|&end| end <= bytes.len())
            .ok_or(FpgError::TableOutOfBounds { count })?;
        let mut entries = Vec::with_capacity(count as usize);
        for (index, rec) in bytes[TABLE_OFFSET..table_end]
            .as_chunks::<ENTRY_SIZE>()
            .0
            .iter()
            .enumerate()
        {
            let hash = read_u32(rec, 0);
            let offset = read_u32(rec, 4);
            let stored_size = read_u32(rec, 8);
            let size = read_u32(rec, 12);
            let start = offset as usize;
            let stored = start
                .checked_add(stored_size as usize)
                .filter(|&end| end <= bytes.len())
                .map(|end| bytes[start..end].to_vec())
                .ok_or(FpgError::EntryOutOfBounds { index })?;
            entries.push(Entry {
                hash,
                size,
                offset,
                stored,
            });
        }
        Ok(Archive { entries })
    }

    /// Serialize with the canonical layout: table at 0x800, data from the next 0x800 boundary,
    /// entries in table order, each padded to 0x800.
    pub fn to_bytes(&self) -> Vec<u8> {
        let count = self.entries.len();
        let data_start = align_up(TABLE_OFFSET + count * ENTRY_SIZE);
        let total = data_start
            + self
                .entries
                .iter()
                .map(|e| align_up(e.stored.len()))
                .sum::<usize>();
        let mut out = vec![0u8; total];
        out[0..4].copy_from_slice(&MAGIC);
        put_u32(&mut out, 4, count);
        let mut pos = data_start;
        for (i, e) in self.entries.iter().enumerate() {
            let rec = TABLE_OFFSET + i * ENTRY_SIZE;
            let padded = align_up(e.stored.len());
            put_u32(&mut out, rec, e.hash as usize);
            put_u32(&mut out, rec + 4, pos);
            put_u32(&mut out, rec + 8, padded);
            put_u32(&mut out, rec + 12, e.size as usize);
            out[pos..pos + e.stored.len()].copy_from_slice(&e.stored);
            pos += padded;
        }
        out
    }

    /// Find an entry by file name (directories are ignored, as the game does).
    pub fn find(&self, name: &str) -> Option<(usize, &Entry)> {
        let h = hash_name(base_name(name));
        self.entries.iter().enumerate().find(|(_, e)| e.hash == h)
    }
}

/// The file-name part of a path (`/` or `\` separated).
pub fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Name hash used by the table: CRC-32 (zlib's) of the ASCII upper-cased name with `\`
/// replaced by `/`.
pub fn hash_name(name: &str) -> u32 {
    let norm: Vec<u8> = name
        .bytes()
        .map(|b| {
            if b == b'\\' {
                b'/'
            } else {
                b.to_ascii_uppercase()
            }
        })
        .collect();
    crc32fast::hash(&norm)
}

fn align_up(n: usize) -> usize {
    n.div_ceil(ALIGN) * ALIGN
}

fn read_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn put_u32(b: &mut [u8], at: usize, v: usize) {
    let v = u32::try_from(v).expect("FPG field larger than 4 GiB");
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic payloads only: patterned bytes and text written for the test.
    fn sample() -> Archive {
        let big: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        Archive {
            entries: vec![
                Entry::from_data("hello.txt", b"hello, archive"),
                Entry::from_data("data/pattern.bin", &big),
                Entry::from_data("empty.bin", b""),
            ],
        }
    }

    #[test]
    fn hash_is_crc32_of_uppercase() {
        // Standard CRC-32 check value; digits are unaffected by upper-casing.
        assert_eq!(hash_name("123456789"), 0xCBF4_3926);
        assert_eq!(hash_name("abc.txt"), hash_name("ABC.TXT"));
        assert_eq!(hash_name("a\\b.txt"), hash_name("A/B.TXT"));
    }

    #[test]
    fn layout_is_canonical() {
        let bytes = sample().to_bytes();
        assert_eq!(&bytes[0..4], b"30GF");
        assert_eq!(read_u32(&bytes, 4), 3);
        assert!(bytes[8..TABLE_OFFSET].iter().all(|&b| b == 0));
        assert_eq!(bytes.len() % ALIGN, 0);
        // Data starts at the first 0x800 boundary after the table.
        assert_eq!(read_u32(&bytes, TABLE_OFFSET + 4) as usize, 0x1000);
        let mut expected = 0x1000;
        for i in 0..3 {
            let rec = TABLE_OFFSET + i * ENTRY_SIZE;
            assert_eq!(read_u32(&bytes, rec + 4) as usize, expected);
            let stored = read_u32(&bytes, rec + 8) as usize;
            assert_eq!(stored % ALIGN, 0);
            expected += stored;
        }
        assert_eq!(expected, bytes.len());
    }

    #[test]
    fn roundtrip_is_byte_exact() {
        let bytes = sample().to_bytes();
        let parsed = Archive::parse(&bytes).unwrap();
        assert_eq!(parsed.to_bytes(), bytes);
    }

    #[test]
    fn entries_decompress_to_original() {
        let bytes = sample().to_bytes();
        let a = Archive::parse(&bytes).unwrap();
        let (i, e) = a.find("some/dir/hello.txt").unwrap();
        assert_eq!(e.decompress(i).unwrap(), b"hello, archive");
        let (i, e) = a.find("PATTERN.BIN").unwrap();
        assert_eq!(e.decompress(i).unwrap().len(), 10_000);
        let (i, e) = a.find("empty.bin").unwrap();
        assert!(e.decompress(i).unwrap().is_empty());
        assert!(a.find("missing.bin").is_none());
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Archive::parse(&[0u8; 16]), Err(FpgError::TooShort));
        let mut bytes = sample().to_bytes();
        bytes[0] = b'X';
        assert_eq!(Archive::parse(&bytes), Err(FpgError::BadMagic));

        let mut bytes = sample().to_bytes();
        bytes[4..8].copy_from_slice(&1_000_000u32.to_le_bytes());
        assert!(matches!(
            Archive::parse(&bytes),
            Err(FpgError::TableOutOfBounds { .. })
        ));

        let mut bytes = sample().to_bytes();
        let len = bytes.len() as u32;
        bytes[TABLE_OFFSET + 4..TABLE_OFFSET + 8].copy_from_slice(&len.to_le_bytes());
        assert_eq!(
            Archive::parse(&bytes),
            Err(FpgError::EntryOutOfBounds { index: 0 })
        );
    }

    #[test]
    fn detects_size_mismatch_and_corruption() {
        let mut a = Archive::parse(&sample().to_bytes()).unwrap();
        a.entries[0].size += 1;
        assert!(matches!(
            a.entries[0].decompress(0),
            Err(FpgError::SizeMismatch { .. })
        ));
        a.entries[1].stored[0] ^= 0xFF;
        assert!(matches!(
            a.entries[1].decompress(1),
            Err(FpgError::Decompress { .. })
        ));
    }
}
