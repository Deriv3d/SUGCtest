//! SPR stream container: a header page followed by zlib-compressed streams split into chunks.
//!
//! See `knowledge/formats/spr.md`. Layout (all integers big-endian u32):
//!
//! - `0x0000..0x1000`: header page (kept as opaque bytes for now).
//! - From `0x1000`: streams. Each starts on a 0x800 boundary and is a run of chunks.
//! - Chunk: `ZBLK`, chunk index within its stream, compressed size, uncompressed size, then
//!   the zlib stream. The next chunk of the same stream starts at the next 16-byte boundary.
//! - The file is zero-padded to a multiple of 0x800.

use std::fmt;
use std::io::{Read, Write};

/// Size of the header page.
pub const HEADER_SIZE: usize = 0x1000;
/// Chunk tag.
pub const CHUNK_TAG: [u8; 4] = *b"ZBLK";
/// Chunk header size.
pub const CHUNK_HEADER: usize = 16;
/// Largest uncompressed chunk the original files use.
pub const CHUNK_DATA_MAX: usize = 65_000;
const STREAM_ALIGN: usize = 0x800;
const CHUNK_ALIGN: usize = 16;

/// Errors from parsing or decompressing an SPR container.
#[derive(Debug, PartialEq, Eq)]
pub enum SprError {
    TooShort,
    NoChunks,
    BadChunk {
        at: usize,
    },
    TrailingData {
        at: usize,
    },
    Decompress {
        stream: usize,
        chunk: usize,
        reason: String,
    },
    SizeMismatch {
        stream: usize,
        chunk: usize,
    },
}

impl fmt::Display for SprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SprError::TooShort => write!(f, "file shorter than the SPR header page"),
            SprError::NoChunks => write!(f, "no ZBLK chunk after the header page"),
            SprError::BadChunk { at } => write!(f, "malformed chunk at {at:#x}"),
            SprError::TrailingData { at } => write!(f, "unexpected non-zero data at {at:#x}"),
            SprError::Decompress {
                stream,
                chunk,
                reason,
            } => {
                write!(
                    f,
                    "stream {stream} chunk {chunk} does not decompress: {reason}"
                )
            }
            SprError::SizeMismatch { stream, chunk } => {
                write!(
                    f,
                    "stream {stream} chunk {chunk} decompressed to the wrong size"
                )
            }
        }
    }
}

impl std::error::Error for SprError {}

/// One chunk, kept exactly as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Uncompressed size from the chunk header.
    pub size: u32,
    /// The zlib stream (exactly `compressed size` bytes, no padding).
    pub stored: Vec<u8>,
}

/// One stream: its chunks in order (their indices are 0, 1, 2, …).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stream {
    pub chunks: Vec<Chunk>,
}

impl Stream {
    /// Compress `data` into chunks of at most [`CHUNK_DATA_MAX`] bytes.
    pub fn from_data(data: &[u8]) -> Stream {
        let chunks = data
            .chunks(CHUNK_DATA_MAX)
            .map(|part| {
                let mut enc =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                enc.write_all(part).expect("writing to a Vec cannot fail");
                Chunk {
                    size: part.len() as u32,
                    stored: enc.finish().expect("writing to a Vec cannot fail"),
                }
            })
            .collect();
        Stream { chunks }
    }

    /// Inflate and concatenate all chunks, checking each chunk's size.
    pub fn decompress(&self, stream: usize) -> Result<Vec<u8>, SprError> {
        let mut out = Vec::with_capacity(self.chunks.iter().map(|c| c.size as usize).sum());
        for (chunk, c) in self.chunks.iter().enumerate() {
            let before = out.len();
            flate2::read::ZlibDecoder::new(&c.stored[..])
                .read_to_end(&mut out)
                .map_err(|e| SprError::Decompress {
                    stream,
                    chunk,
                    reason: e.to_string(),
                })?;
            if out.len() - before != c.size as usize {
                return Err(SprError::SizeMismatch { stream, chunk });
            }
        }
        Ok(out)
    }
}

/// A parsed SPR container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// The 0x1000-byte header page, unparsed.
    pub header: Vec<u8>,
    pub streams: Vec<Stream>,
}

impl Container {
    /// Parse a container from its bytes. Chunk data is copied, not inflated.
    pub fn parse(bytes: &[u8]) -> Result<Container, SprError> {
        if bytes.len() < HEADER_SIZE {
            return Err(SprError::TooShort);
        }
        let mut streams: Vec<Stream> = Vec::new();
        let mut pos = HEADER_SIZE;
        loop {
            // A new stream starts on a 0x800 boundary with chunk index 0; the gap is zero.
            let next = pos.next_multiple_of(STREAM_ALIGN).min(bytes.len());
            if let Some(i) = bytes[pos..next].iter().position(|&b| b != 0) {
                return Err(SprError::TrailingData { at: pos + i });
            }
            pos = next;
            if pos + CHUNK_HEADER > bytes.len() || bytes[pos..pos + 4] != CHUNK_TAG {
                break;
            }
            let mut stream = Stream::default();
            loop {
                let (index, chunk, end) = read_chunk(bytes, pos)?;
                if index as usize != stream.chunks.len() {
                    return Err(SprError::BadChunk { at: pos });
                }
                stream.chunks.push(chunk);
                let next = end.next_multiple_of(CHUNK_ALIGN);
                let continues = next + CHUNK_HEADER <= bytes.len()
                    && bytes[next..next + 4] == CHUNK_TAG
                    && read_u32(bytes, next + 4) as usize == stream.chunks.len();
                pos = end;
                if !continues {
                    break;
                }
                if bytes[end..next].iter().any(|&b| b != 0) {
                    return Err(SprError::TrailingData { at: end });
                }
                pos = next;
            }
            streams.push(stream);
        }
        if streams.is_empty() {
            return Err(SprError::NoChunks);
        }
        if let Some(i) = bytes[pos..].iter().position(|&b| b != 0) {
            return Err(SprError::TrailingData { at: pos + i });
        }
        Ok(Container {
            header: bytes[..HEADER_SIZE].to_vec(),
            streams,
        })
    }

    /// Serialize with the canonical layout described in the module docs.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.header.clone();
        out.resize(HEADER_SIZE, 0);
        for stream in &self.streams {
            out.resize(out.len().next_multiple_of(STREAM_ALIGN), 0);
            for (i, c) in stream.chunks.iter().enumerate() {
                out.resize(out.len().next_multiple_of(CHUNK_ALIGN), 0);
                out.extend_from_slice(&CHUNK_TAG);
                for v in [i, c.stored.len(), c.size as usize] {
                    out.extend_from_slice(&(v as u32).to_be_bytes());
                }
                out.extend_from_slice(&c.stored);
            }
        }
        out.resize(out.len().next_multiple_of(STREAM_ALIGN), 0);
        out
    }
}

fn read_chunk(bytes: &[u8], at: usize) -> Result<(u32, Chunk, usize), SprError> {
    let bad = SprError::BadChunk { at };
    if at + CHUNK_HEADER > bytes.len() || bytes[at..at + 4] != CHUNK_TAG {
        return Err(bad);
    }
    let index = read_u32(bytes, at + 4);
    let csize = read_u32(bytes, at + 8) as usize;
    let size = read_u32(bytes, at + 12);
    let start = at + CHUNK_HEADER;
    let end = start
        .checked_add(csize)
        .filter(|&e| e <= bytes.len())
        .ok_or(bad)?;
    Ok((
        index,
        Chunk {
            size,
            stored: bytes[start..end].to_vec(),
        },
        end,
    ))
}

fn read_u32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Container {
        let mut header = vec![0u8; HEADER_SIZE];
        header[..8].copy_from_slice(b"testhead");
        let big: Vec<u8> = (0..150_000u32).map(|i| (i % 253) as u8).collect();
        Container {
            header,
            streams: vec![
                Stream::from_data(b"small stream"),
                Stream::from_data(&big),
                Stream::from_data(&[7u8; 70_000]),
            ],
        }
    }

    #[test]
    fn layout_is_canonical() {
        let bytes = sample().to_bytes();
        assert_eq!(bytes.len() % STREAM_ALIGN, 0);
        assert_eq!(&bytes[HEADER_SIZE..HEADER_SIZE + 4], b"ZBLK");
        assert_eq!(read_u32(&bytes, HEADER_SIZE + 4), 0);
        // Second stream starts on the next 0x800 boundary.
        let s1 = (HEADER_SIZE + CHUNK_HEADER + sample().streams[0].chunks[0].stored.len())
            .next_multiple_of(STREAM_ALIGN);
        assert_eq!(&bytes[s1..s1 + 4], b"ZBLK");
    }

    #[test]
    fn roundtrip_is_byte_exact() {
        let bytes = sample().to_bytes();
        let parsed = Container::parse(&bytes).unwrap();
        assert_eq!(parsed.streams.len(), 3);
        assert_eq!(parsed.streams[1].chunks.len(), 3);
        assert_eq!(parsed.to_bytes(), bytes);
    }

    #[test]
    fn streams_decompress_to_original() {
        let c = Container::parse(&sample().to_bytes()).unwrap();
        assert_eq!(c.streams[0].decompress(0).unwrap(), b"small stream");
        let big = c.streams[1].decompress(1).unwrap();
        assert_eq!(big.len(), 150_000);
        assert!(big.iter().enumerate().all(|(i, &b)| b == (i % 253) as u8));
        assert_eq!(c.streams[2].decompress(2).unwrap(), vec![7u8; 70_000]);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Container::parse(&[0u8; 16]), Err(SprError::TooShort));
        assert_eq!(
            Container::parse(&vec![0u8; 0x2000]),
            Err(SprError::NoChunks)
        );
        let mut bytes = sample().to_bytes();
        let last = bytes.len() - 1;
        bytes[last] = 1;
        assert!(matches!(
            Container::parse(&bytes),
            Err(SprError::TrailingData { .. })
        ));
        let mut bytes = sample().to_bytes();
        bytes[HEADER_SIZE + 8..HEADER_SIZE + 12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            Container::parse(&bytes),
            Err(SprError::BadChunk { .. })
        ));
    }

    #[test]
    fn detects_corrupt_chunk() {
        let mut c = Container::parse(&sample().to_bytes()).unwrap();
        c.streams[1].chunks[1].size += 1;
        assert!(matches!(
            c.streams[1].decompress(1),
            Err(SprError::SizeMismatch { .. })
        ));
    }
}
