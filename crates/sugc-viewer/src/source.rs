//! SUGC-specific asset source: reads the user's own `USRDIR` with `sugc-formats` and maps each
//! asset to a generic [`Asset`].

use std::path::Path;

use asset_browser::{Asset, AssetEntry, AssetSource};
use sugc_formats::{fpg, msf::Msf, spr, strtab, texture};

enum Item {
    Archive { archive: usize, entry: usize },
    Texture { file: usize, index: usize },
    Music { file: usize },
}

struct SprTextures {
    textures: Vec<(texture::Texture, Vec<u8>)>,
}

pub struct SugcSource {
    archives: Vec<fpg::Archive>,
    sprs: Vec<SprTextures>,
    music: Vec<Msf>,
    items: Vec<Item>,
    entries: Vec<AssetEntry>,
}

const ARCHIVES: [&str; 2] = ["flog_u.fpg", "flog_c.fpg"];
const SPRS: [&str; 2] = ["streams/ui.spr", "streams/global_binary.spr"];
const MUSIC_DIR: &str = "sounds/music";

impl SugcSource {
    pub fn open(usrdir: &Path) -> Result<SugcSource, String> {
        let read = |rel: &str| std::fs::read(usrdir.join(rel)).map_err(|e| format!("{rel}: {e}"));
        let mut s = SugcSource {
            archives: Vec::new(),
            sprs: Vec::new(),
            music: Vec::new(),
            items: Vec::new(),
            entries: Vec::new(),
        };
        for name in ARCHIVES {
            let a = fpg::Archive::parse(&read(name)?).map_err(|e| format!("{name}: {e}"))?;
            let ai = s.archives.len();
            for (i, e) in a.entries.iter().enumerate() {
                let kind = e.decompress(i).map(|d| kind_of(&d)).unwrap_or("unreadable");
                s.push(
                    name,
                    format!("{i:03}  {:08x}  {kind}", e.hash),
                    Item::Archive {
                        archive: ai,
                        entry: i,
                    },
                );
            }
            s.archives.push(a);
        }
        for name in SPRS {
            let c = spr::Container::parse(&read(name)?).map_err(|e| format!("{name}: {e}"))?;
            let streams: Vec<Vec<u8>> = c
                .streams
                .iter()
                .enumerate()
                .map(|(i, st)| st.decompress(i))
                .collect::<Result<_, _>>()
                .map_err(|e| format!("{name}: {e}"))?;
            let mut textures = Vec::new();
            for pair in 0..streams.len().saturating_sub(1) {
                let found: Vec<_> = texture::find_textures(&streams[pair])
                    .into_iter()
                    .map(|(_, t)| t)
                    .collect();
                if let Ok(parts) = texture::split_texture_data(&streams[pair + 1], &found) {
                    textures.extend(found.into_iter().zip(parts.into_iter().map(<[u8]>::to_vec)));
                }
            }
            let fi = s.sprs.len();
            for (i, (t, _)) in textures.iter().enumerate() {
                let label = format!("{i:03}  {}×{}  {:?}", t.width, t.height, t.format);
                s.push(name, label, Item::Texture { file: fi, index: i });
            }
            s.sprs.push(SprTextures { textures });
        }
        if let Ok(dir) = std::fs::read_dir(usrdir.join(MUSIC_DIR)) {
            let mut files: Vec<_> = dir.filter_map(Result::ok).map(|d| d.path()).collect();
            files.sort();
            for path in files
                .into_iter()
                .filter(|p| p.extension().is_some_and(|e| e == "msf"))
            {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let m = Msf::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                let label = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let fi = s.music.len();
                s.push(MUSIC_DIR, label, Item::Music { file: fi });
                s.music.push(m);
            }
        }
        Ok(s)
    }

    fn push(&mut self, group: &str, label: String, item: Item) {
        self.entries.push(AssetEntry {
            id: self.items.len(),
            group: group.to_string(),
            label,
        });
        self.items.push(item);
    }
}

impl AssetSource for SugcSource {
    fn entries(&self) -> &[AssetEntry] {
        &self.entries
    }

    fn load(&self, id: usize) -> Result<Asset, String> {
        match self.items.get(id).ok_or("no such asset")? {
            Item::Archive { archive, entry } => {
                let e = &self.archives[*archive].entries[*entry];
                let data = e.decompress(*entry).map_err(|e| e.to_string())?;
                Ok(decode_entry(&data))
            }
            Item::Texture { file, index } => {
                let (t, data) = &self.sprs[*file].textures[*index];
                let rgba = texture::decode_rgba(t, data).map_err(|e| e.to_string())?;
                Ok(Asset::Image {
                    width: u32::from(t.width),
                    height: u32::from(t.height),
                    rgba,
                })
            }
            Item::Music { file } => {
                let m = &self.music[*file];
                let pcm = m.decode_pcm().map_err(|e| e.to_string())?;
                Ok(Asset::Audio {
                    channels: m.channels.max(1) as u16,
                    sample_rate: m.sample_rate,
                    pcm,
                })
            }
        }
    }
}

/// Short kind label for the list (container signatures and our own format checks only).
fn kind_of(d: &[u8]) -> &'static str {
    if d.starts_with(b"\x89PNG") {
        "image"
    } else if d.starts_with(b"RIFF") && d.get(8..12) == Some(b"WAVE") {
        "sound"
    } else if is_genesis_rom(d) {
        "Genesis ROM"
    } else if is_sms_rom(d) {
        "Master System / Game Gear ROM"
    } else if strtab::parse(d).is_some() {
        "string table"
    } else if d
        .iter()
        .take(4096)
        .all(|&b| b == b'\n' || b == b'\r' || b == b'\t' || (0x20..0x7F).contains(&b))
    {
        "text"
    } else {
        "binary"
    }
}

fn is_genesis_rom(d: &[u8]) -> bool {
    d.get(0x100..0x104) == Some(b"SEGA")
}

fn is_sms_rom(d: &[u8]) -> bool {
    [0x1FF0, 0x3FF0, 0x7FF0]
        .iter()
        .any(|&o| d.get(o..o + 8) == Some(b"TMR SEGA"))
}

fn decode_entry(d: &[u8]) -> Asset {
    if d.starts_with(b"\x89PNG") {
        return decode_png(d)
            .unwrap_or_else(|e| Asset::Info(vec![format!("PNG decode failed: {e}")]));
    }
    if let Some(a) = decode_wav(d) {
        return a;
    }
    if let Some(lines) = strtab::parse(d) {
        return Asset::Text(lines);
    }
    match kind_of(d) {
        "text" => Asset::Text(
            String::from_utf8_lossy(d)
                .lines()
                .map(str::to_string)
                .collect(),
        ),
        kind => {
            let mut info = vec![format!("{kind}, {} bytes", d.len())];
            if kind == "Genesis ROM" {
                info.push(format!(
                    "header checksum {}",
                    if genesis_checksum_ok(d) {
                        "valid"
                    } else {
                        "does not match"
                    }
                ));
            }
            Asset::Info(info)
        }
    }
}

fn genesis_checksum_ok(d: &[u8]) -> bool {
    let Some(stored) = d.get(0x18E..0x190) else {
        return false;
    };
    let sum = d[0x200.min(d.len())..].chunks(2).fold(0u16, |acc, w| {
        acc.wrapping_add(u16::from_be_bytes([w[0], *w.get(1).unwrap_or(&0)]))
    });
    sum == u16::from_be_bytes([stored[0], stored[1]])
}

fn decode_png(d: &[u8]) -> Result<Asset, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(d));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px
            .chunks(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    Ok(Asset::Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// Minimal RIFF/WAVE reader for 16-bit PCM.
fn decode_wav(d: &[u8]) -> Option<Asset> {
    if !(d.starts_with(b"RIFF") && d.get(8..12) == Some(b"WAVE")) {
        return None;
    }
    let (mut pos, mut fmt, mut data) = (12usize, None, None);
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let len = u32::from_le_bytes(d[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = d.get(pos + 8..pos + 8 + len)?;
        match id {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            _ => {}
        }
        pos += 8 + len + (len & 1);
    }
    let fmt = fmt?;
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let sample_rate = u32::from_le_bytes(fmt[4..8].try_into().ok()?);
    let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
    if u16::from_le_bytes([fmt[0], fmt[1]]) != 1 || bits != 16 {
        return Some(Asset::Info(vec![format!("WAV, {bits}-bit, not decoded")]));
    }
    let pcm = data?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect();
    Some(Asset::Audio {
        channels,
        sample_rate,
        pcm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_reader() {
        let mut w = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        w.extend(16u32.to_le_bytes());
        w.extend([1, 0, 1, 0]);
        w.extend(22_050u32.to_le_bytes());
        w.extend(44_100u32.to_le_bytes());
        w.extend([2, 0, 16, 0]);
        w.extend(b"data");
        w.extend(4u32.to_le_bytes());
        w.extend([0x34, 0x12, 0xFF, 0xFF]);
        assert_eq!(
            decode_wav(&w),
            Some(Asset::Audio {
                channels: 1,
                sample_rate: 22_050,
                pcm: vec![0x1234, -1]
            })
        );
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(b"\x89PNG\r\n\x1a\n"), "image");
        assert_eq!(kind_of(b"plain words\n"), "text");
        assert_eq!(kind_of(&strtab::build(&["a", "b"])), "string table");
        assert_eq!(kind_of(&[0xF3, 0x00, 0x01]), "binary");
    }
}
