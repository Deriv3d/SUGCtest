//! PS3 GPU (GCM) texture descriptors as stored in SPR structure streams, the matching
//! `TEXL` texture-data stream, and decoding of the top mip level to RGBA8.
//!
//! See `knowledge/formats/spr.md`.

use std::fmt;

/// Size of a GCM texture descriptor.
pub const DESC_SIZE: usize = 24;
/// Bytes of the texture record that precede the descriptor (they carry the GPU address and
/// the data size, which we use to validate a candidate descriptor).
pub const RECORD_PREFIX: usize = 32;
/// Tag that starts every block in a texture-data stream.
pub const TEXL_TAG: [u8; 4] = *b"TEXL";
const TEXL_HEADER: usize = 16;
/// GPU address of local memory offset 0.
const LOCAL_MEMORY_BASE: u32 = 0xC000_0000;

/// Pixel formats seen so far (the base format, without the swizzle/normalize flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    B8,
    A1R5G5B5,
    A4R4G4B4,
    R5G6B5,
    A8R8G8B8,
    Dxt1,
    Dxt3,
    Dxt5,
    G8B8,
    D8R8G8B8,
}

impl Format {
    fn from_gcm(base: u8) -> Option<Format> {
        Some(match base {
            0x81 => Format::B8,
            0x82 => Format::A1R5G5B5,
            0x83 => Format::A4R4G4B4,
            0x84 => Format::R5G6B5,
            0x85 => Format::A8R8G8B8,
            0x86 => Format::Dxt1,
            0x87 => Format::Dxt3,
            0x88 => Format::Dxt5,
            0x8B => Format::G8B8,
            0x9E => Format::D8R8G8B8,
            _ => return None,
        })
    }

    /// Bytes for one mip level of `w`×`h`.
    fn level_size(self, w: u32, h: u32) -> usize {
        let (w, h) = (w as usize, h as usize);
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        match self {
            Format::Dxt1 => blocks * 8,
            Format::Dxt3 | Format::Dxt5 => blocks * 16,
            Format::B8 => w * h,
            Format::A1R5G5B5 | Format::A4R4G4B4 | Format::R5G6B5 | Format::G8B8 => w * h * 2,
            Format::A8R8G8B8 | Format::D8R8G8B8 => w * h * 4,
        }
    }
}

/// A decoded GCM texture descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Texture {
    pub format: Format,
    /// True if stored linearly (row by row); false means swizzled for uncompressed formats.
    pub linear: bool,
    pub mipmaps: u8,
    pub width: u16,
    pub height: u16,
    pub pitch: u32,
    /// Offset in GPU local memory at runtime.
    pub offset: u32,
}

impl Texture {
    /// Parse a 24-byte big-endian GCM texture descriptor (2D, non-cube, depth 1 only).
    pub fn parse(d: &[u8]) -> Option<Texture> {
        if d.len() < DESC_SIZE {
            return None;
        }
        let raw = d[0];
        let format = Format::from_gcm(raw & !0x60)?;
        let (mipmaps, dimension, cubemap) = (d[1], d[2], d[3]);
        let width = u16::from_be_bytes([d[8], d[9]]);
        let height = u16::from_be_bytes([d[10], d[11]]);
        let depth = u16::from_be_bytes([d[12], d[13]]);
        let location = d[14];
        if !(1..=13).contains(&mipmaps)
            || dimension != 2
            || cubemap != 0
            || depth != 1
            || location > 1
            || !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
        {
            return None;
        }
        Some(Texture {
            format,
            linear: raw & 0x20 != 0,
            mipmaps,
            width,
            height,
            pitch: read_u32(d, 16),
            offset: read_u32(d, 20),
        })
    }

    /// Size in bytes of the full mip chain.
    pub fn data_size(&self) -> usize {
        let (mut w, mut h) = (u32::from(self.width), u32::from(self.height));
        let mut total = 0;
        for _ in 0..self.mipmaps {
            total += self.format.level_size(w, h);
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        total
    }
}

/// Find texture records in a structure stream. A candidate descriptor is accepted only if the
/// record prefix holds its GPU address and exactly its mip-chain size.
pub fn find_textures(structure: &[u8]) -> Vec<(usize, Texture)> {
    let mut found = Vec::new();
    let mut at = RECORD_PREFIX;
    while at + DESC_SIZE <= structure.len() {
        if let Some(t) = Texture::parse(&structure[at..]) {
            let gpu_addr = read_u32(structure, at - 20);
            let size = read_u32(structure, at - 12) as usize;
            if gpu_addr == LOCAL_MEMORY_BASE.wrapping_add(t.offset) && size == t.data_size() {
                found.push((at, t));
                at += DESC_SIZE;
                continue;
            }
        }
        at += 4;
    }
    found
}

/// Errors from splitting a texture-data stream or decoding a texture.
#[derive(Debug, PartialEq, Eq)]
pub enum TextureError {
    MissingTag { index: usize, at: usize },
    Truncated { index: usize },
    TrailingData,
    Unsupported(Format),
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureError::MissingTag { index, at } => {
                write!(f, "texture {index}: no TEXL block at {at:#x}")
            }
            TextureError::Truncated { index } => write!(f, "texture {index}: data truncated"),
            TextureError::TrailingData => write!(f, "texture data stream longer than expected"),
            TextureError::Unsupported(fmt) => write!(f, "decoding {fmt:?} is not supported yet"),
        }
    }
}

impl std::error::Error for TextureError {}

/// Split a texture-data stream into per-texture slices. Blocks are `TEXL` + 12 zero bytes,
/// then the data, padded to 16 bytes, in descriptor order.
pub fn split_texture_data<'a>(
    data: &'a [u8],
    textures: &[Texture],
) -> Result<Vec<&'a [u8]>, TextureError> {
    let mut out = Vec::with_capacity(textures.len());
    let mut pos = 0;
    for (index, t) in textures.iter().enumerate() {
        let header_ok = data
            .get(pos..pos + TEXL_HEADER)
            .is_some_and(|h| h[..4] == TEXL_TAG && h[4..].iter().all(|&b| b == 0));
        if !header_ok {
            return Err(TextureError::MissingTag { index, at: pos });
        }
        let start = pos + TEXL_HEADER;
        let end = start + t.data_size();
        out.push(
            data.get(start..end)
                .ok_or(TextureError::Truncated { index })?,
        );
        pos = end.next_multiple_of(16);
    }
    if pos < data.len() {
        return Err(TextureError::TrailingData);
    }
    Ok(out)
}

/// Decode the top mip level to tightly packed RGBA8.
pub fn decode_rgba(t: &Texture, data: &[u8]) -> Result<Vec<u8>, TextureError> {
    let (w, h) = (usize::from(t.width), usize::from(t.height));
    let mut out = vec![0u8; w * h * 4];
    match t.format {
        Format::Dxt1 | Format::Dxt3 | Format::Dxt5 => {
            let bsize = if t.format == Format::Dxt1 { 8 } else { 16 };
            let bw = w.div_ceil(4);
            for (bi, block) in data
                .chunks_exact(bsize)
                .take(bw * h.div_ceil(4))
                .enumerate()
            {
                let px = decode_block(t.format, block);
                let (bx, by) = (bi % bw * 4, bi / bw * 4);
                for (i, p) in px.iter().enumerate() {
                    let (x, y) = (bx + i % 4, by + i / 4);
                    if x < w && y < h {
                        out[(y * w + x) * 4..][..4].copy_from_slice(p);
                    }
                }
            }
        }
        Format::A8R8G8B8 | Format::D8R8G8B8 => {
            let pitch = if t.linear && t.pitch as usize >= w * 4 {
                t.pitch as usize
            } else {
                w * 4
            };
            for y in 0..h {
                for x in 0..w {
                    let src = if t.linear {
                        y * pitch + x * 4
                    } else {
                        swizzle(x, y, w, h) * 4
                    };
                    let s = data
                        .get(src..src + 4)
                        .ok_or(TextureError::Truncated { index: 0 })?;
                    let a = if t.format == Format::D8R8G8B8 {
                        255
                    } else {
                        s[0]
                    };
                    out[(y * w + x) * 4..][..4].copy_from_slice(&[s[1], s[2], s[3], a]);
                }
            }
        }
        other => return Err(TextureError::Unsupported(other)),
    }
    Ok(out)
}

/// Index of texel (x, y) in a swizzled (Morton-ordered) texture of size w×h.
pub fn swizzle(x: usize, y: usize, w: usize, h: usize) -> usize {
    let (lw, lh) = (
        w.next_power_of_two().trailing_zeros(),
        h.next_power_of_two().trailing_zeros(),
    );
    let (mut out, mut bit, mut i) = (0usize, 0u32, 0u32);
    while i < lw.max(lh) {
        if i < lw {
            out |= ((x >> i) & 1) << bit;
            bit += 1;
        }
        if i < lh {
            out |= ((y >> i) & 1) << bit;
            bit += 1;
        }
        i += 1;
    }
    out
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = (c >> 11) & 0x1F;
    let g = (c >> 5) & 0x3F;
    let b = c & 0x1F;
    [
        (r * 255 / 31) as u8,
        (g * 255 / 63) as u8,
        (b * 255 / 31) as u8,
    ]
}

/// Decode one 4×4 block (BC1/BC2/BC3 layout, little-endian colour words) to 16 RGBA pixels.
fn decode_block(format: Format, b: &[u8]) -> [[u8; 4]; 16] {
    let colour = if format == Format::Dxt1 { b } else { &b[8..] };
    let c0 = u16::from_le_bytes([colour[0], colour[1]]);
    let c1 = u16::from_le_bytes([colour[2], colour[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u16, wb: u16| {
        ((u16::from(a) * wa + u16::from(b) * wb) / (wa + wb)) as u8
    };
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || format != Format::Dxt1 {
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 2, 1);
            pal[3][k] = mix(p0[k], p1[k], 1, 2);
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for k in 0..3 {
            pal[2][k] = mix(p0[k], p1[k], 1, 1);
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let idx = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]);
    let mut px = [[0u8; 4]; 16];
    for (i, p) in px.iter_mut().enumerate() {
        *p = pal[((idx >> (2 * i)) & 3) as usize];
    }
    match format {
        Format::Dxt3 => {
            for (i, p) in px.iter_mut().enumerate() {
                let nib = (b[i / 2] >> ((i % 2) * 4)) & 0xF;
                p[3] = nib * 17;
            }
        }
        Format::Dxt5 => {
            let (a0, a1) = (b[0], b[1]);
            let mut alpha = [0u8; 8];
            alpha[0] = a0;
            alpha[1] = a1;
            for k in 1..7u16 {
                alpha[k as usize + 1] = if a0 > a1 {
                    ((u16::from(a0) * (7 - k) + u16::from(a1) * k) / 7) as u8
                } else if k < 5 {
                    ((u16::from(a0) * (5 - k) + u16::from(a1) * k) / 5) as u8
                } else if k == 5 {
                    0
                } else {
                    255
                };
            }
            let bits = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], 0, 0]);
            for (i, p) in px.iter_mut().enumerate() {
                p[3] = alpha[((bits >> (3 * i)) & 7) as usize];
            }
        }
        _ => {}
    }
    px
}

fn read_u32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(fmt: u8, mips: u8, w: u16, h: u16, offset: u32) -> [u8; DESC_SIZE] {
        let mut d = [0u8; DESC_SIZE];
        d[0] = fmt;
        d[1] = mips;
        d[2] = 2;
        d[8..10].copy_from_slice(&w.to_be_bytes());
        d[10..12].copy_from_slice(&h.to_be_bytes());
        d[12..14].copy_from_slice(&1u16.to_be_bytes());
        d[20..24].copy_from_slice(&offset.to_be_bytes());
        d
    }

    /// A synthetic structure stream: filler, then a record prefix + descriptor.
    fn record(t: &[u8; DESC_SIZE], size: u32, offset: u32) -> Vec<u8> {
        let mut r = vec![0u8; RECORD_PREFIX];
        r[12..16].copy_from_slice(&(LOCAL_MEMORY_BASE + offset).to_be_bytes());
        r[20..24].copy_from_slice(&size.to_be_bytes());
        r.extend_from_slice(t);
        r
    }

    #[test]
    fn mip_chain_sizes() {
        let t = Texture::parse(&desc(0x86, 3, 16, 8, 0)).unwrap();
        // 16x8 + 8x4 + 4x2 DXT1 blocks: 8 + 2 + 1 blocks of 8 bytes.
        assert_eq!(t.data_size(), 88);
        let t = Texture::parse(&desc(0xA5, 1, 3, 5, 0)).unwrap();
        assert!(t.linear);
        assert_eq!(t.format, Format::A8R8G8B8);
        assert_eq!(t.data_size(), 60);
        assert!(Texture::parse(&desc(0x10, 1, 4, 4, 0)).is_none());
    }

    #[test]
    fn finds_only_validated_records() {
        let good = desc(0x88, 1, 8, 8, 0x1000);
        let mut s = vec![0xEEu8; 12];
        s.extend(record(&good, 64, 0x1000));
        s.extend(record(&desc(0x86, 1, 8, 8, 0x2000), 999, 0x2000)); // wrong size: rejected
        s.extend(vec![0u8; 8]);
        let found = find_textures(&s);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1.format, Format::Dxt5);
        assert_eq!(found[0].1.offset, 0x1000);
    }

    #[test]
    fn splits_texl_stream() {
        let a = Texture::parse(&desc(0x86, 1, 4, 4, 0)).unwrap(); // 8 bytes
        let b = Texture::parse(&desc(0xA5, 1, 2, 2, 0)).unwrap(); // 16 bytes
        let mut s = b"TEXL".to_vec();
        s.extend([0u8; 12]);
        s.extend([1u8; 8]);
        s.extend([0u8; 8]);
        s.extend(b"TEXL");
        s.extend([0u8; 12]);
        s.extend([2u8; 16]);
        let parts = split_texture_data(&s, &[a, b]).unwrap();
        assert_eq!(parts[0], &[1u8; 8]);
        assert_eq!(parts[1], &[2u8; 16]);
        s[16] = b'X';
        assert!(split_texture_data(&s[..20], &[a, b]).is_err());
    }

    #[test]
    fn decodes_dxt1_solid_block() {
        let t = Texture::parse(&desc(0x86, 1, 4, 4, 0)).unwrap();
        // c0 = pure red (0xF800), c1 = black, all indices 0.
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let rgba = decode_rgba(&t, &block).unwrap();
        assert!(rgba.chunks(4).all(|p| p == [255, 0, 0, 255]));
    }

    #[test]
    fn decodes_dxt5_alpha() {
        let t = Texture::parse(&desc(0x88, 1, 4, 4, 0)).unwrap();
        let mut block = [0u8; 16];
        block[0] = 128; // alpha0, all alpha indices 0
        block[8..10].copy_from_slice(&0x001Fu16.to_le_bytes()); // blue
        let rgba = decode_rgba(&t, &block).unwrap();
        assert!(rgba.chunks(4).all(|p| p == [0, 0, 255, 128]));
    }

    #[test]
    fn swizzle_is_morton_order() {
        assert_eq!(swizzle(0, 0, 4, 4), 0);
        assert_eq!(swizzle(1, 0, 4, 4), 1);
        assert_eq!(swizzle(0, 1, 4, 4), 2);
        assert_eq!(swizzle(1, 1, 4, 4), 3);
        assert_eq!(swizzle(2, 0, 4, 4), 4);
        // Rectangular: the extra width bits follow the interleaved ones.
        assert_eq!(swizzle(3, 0, 4, 1), 3);
        let all: std::collections::HashSet<usize> = (0..8)
            .flat_map(|y| (0..4).map(move |x| swizzle(x, y, 4, 8)))
            .collect();
        assert_eq!(all.len(), 32);
        assert!(all.iter().all(|&i| i < 32));
    }

    #[test]
    fn decodes_argb_linear_and_swizzled() {
        let lin = Texture::parse(&desc(0xA5, 1, 2, 2, 0)).unwrap();
        let data: Vec<u8> = (0..4u8).flat_map(|i| [200, i, 10 * i, 20 * i]).collect();
        let rgba = decode_rgba(&lin, &data).unwrap();
        assert_eq!(&rgba[4..8], &[1, 10, 20, 200]);
        let sw = Texture::parse(&desc(0x85, 1, 2, 2, 0)).unwrap();
        let rgba = decode_rgba(&sw, &data).unwrap();
        // Texel (0,1) is stored at Morton index 2.
        assert_eq!(&rgba[8..12], &[2, 20, 40, 200]);
    }
}
