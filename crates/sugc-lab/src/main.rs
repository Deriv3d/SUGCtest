//! Lab CLI. Runs against the user's own extracted disc files and writes only to a lab folder
//! outside this repository.
//!
//! ```text
//! sugc-lab roundtrip <file>              FPG or SPR: parse, re-serialize, byte-compare; PASS/FAIL
//! sugc-lab extract <archive> <outdir>    inflate every FPG entry into <outdir>
//! sugc-lab spr-textures <spr> <outdir>   decode every SPR texture to PNG in <outdir>
//! sugc-lab msf-wav <msf> <out.wav>       decode an MSF music stream to a 16-bit WAV
//! ```
//!
//! Output directories must be outside the repository.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sugc_formats::fpg::{self, Archive};
use sugc_formats::msf::{self, Msf};
use sugc_formats::spr::Container;
use sugc_formats::texture;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["roundtrip", file] => roundtrip(Path::new(file)),
        ["extract", archive, outdir] => extract(Path::new(archive), Path::new(outdir)),
        ["spr-textures", file, outdir] => spr_textures(Path::new(file), Path::new(outdir)),
        ["msf-wav", file, out] => msf_wav(Path::new(file), Path::new(out)),
        _ => Err(
            "usage: sugc-lab roundtrip <file> | extract <archive> <outdir> | \
                  spr-textures <spr> <outdir> | msf-wav <msf> <out.wav>"
                .into(),
        ),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// Parse, re-serialize and byte-compare; also inflate every entry/stream and check sizes.
/// The format is detected from the file: FPG by its magic, otherwise SPR.
fn roundtrip(file: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let (kind, rebuilt, n, inflate_ok) = if bytes.starts_with(&fpg::MAGIC) {
        let archive = Archive::parse(&bytes).map_err(|e| e.to_string())?;
        let ok = count_ok(
            archive
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| e.decompress(i).map(drop)),
        );
        ("entries", archive.to_bytes(), archive.entries.len(), ok)
    } else if bytes.starts_with(&msf::MAGIC) {
        let m = Msf::parse(&bytes).map_err(|e| e.to_string())?;
        let ok = count_ok(std::iter::once(m.decode_pcm().map(drop)));
        ("audio streams", m.to_bytes(), 1, ok)
    } else {
        let c = Container::parse(&bytes).map_err(|e| e.to_string())?;
        let ok = count_ok(
            c.streams
                .iter()
                .enumerate()
                .map(|(i, s)| s.decompress(i).map(drop)),
        );
        ("streams", c.to_bytes(), c.streams.len(), ok)
    };
    let identical = rebuilt == bytes;
    let pass = identical && inflate_ok == n;
    println!(
        "{}: {} | {kind} {n} | re-serialized {} bytes vs {} original, {} | inflate ok {inflate_ok}/{n}",
        file.display(),
        if pass { "PASS" } else { "FAIL" },
        rebuilt.len(),
        bytes.len(),
        if identical {
            "byte-identical"
        } else {
            first_diff(&rebuilt, &bytes)
        },
    );
    Ok(pass)
}

fn count_ok<E: std::fmt::Display>(results: impl Iterator<Item = Result<(), E>>) -> usize {
    results
        .filter(|r| match r {
            Ok(()) => true,
            Err(err) => {
                eprintln!("  {err}");
                false
            }
        })
        .count()
}

/// Decode every texture in an SPR container to `<outdir>/<file>_s<stream>_<index>.png`.
/// Structure stream N is paired with texture-data stream N+1.
fn spr_textures(file: &Path, outdir: &Path) -> Result<bool, String> {
    refuse_repo_path(outdir)?;
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let c = Container::parse(&bytes).map_err(|e| e.to_string())?;
    let streams: Vec<Vec<u8>> = c
        .streams
        .iter()
        .enumerate()
        .map(|(i, s)| s.decompress(i))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(outdir).map_err(|e| format!("{}: {e}", outdir.display()))?;
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("spr");
    let (mut total, mut ok) = (0usize, 0usize);
    for pair in 0..streams.len().saturating_sub(1) {
        let found = texture::find_textures(&streams[pair]);
        let textures: Vec<_> = found.iter().map(|(_, t)| *t).collect();
        let Ok(parts) = texture::split_texture_data(&streams[pair + 1], &textures) else {
            continue;
        };
        for (i, (t, data)) in textures.iter().zip(parts).enumerate() {
            total += 1;
            match texture::decode_rgba(t, data) {
                Ok(rgba) => {
                    let path = outdir.join(format!("{stem}_s{pair}_{i:03}.png"));
                    write_png(&path, u32::from(t.width), u32::from(t.height), &rgba)?;
                    ok += 1;
                }
                Err(e) => eprintln!("  s{pair} texture {i}: {e}"),
            }
        }
        println!(
            "{}: stream {pair}: {} textures paired with stream {}",
            file.display(),
            textures.len(),
            pair + 1
        );
    }
    println!(
        "{}: decoded {ok}/{total} textures to {}",
        file.display(),
        outdir.display()
    );
    Ok(total > 0 && ok == total)
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

/// Decode an MSF file to a 16-bit PCM WAV (outside the repo).
fn msf_wav(file: &Path, out: &Path) -> Result<bool, String> {
    refuse_repo_path(out)?;
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let m = Msf::parse(&bytes).map_err(|e| e.to_string())?;
    let pcm = m.decode_pcm().map_err(|e| e.to_string())?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        out,
        wav_bytes(&pcm, m.channels.max(1) as u16, m.sample_rate),
    )
    .map_err(|e| format!("{}: {e}", out.display()))?;
    let frames = pcm.len() / m.channels.max(1) as usize;
    println!(
        "{}: {:?}, {} ch, {} Hz, {} frames ({:.1} s), loop {}+{} -> {}",
        file.display(),
        m.codec,
        m.channels,
        m.sample_rate,
        frames,
        frames as f64 / f64::from(m.sample_rate.max(1)),
        m.loop_start,
        m.loop_length,
        out.display()
    );
    Ok(true)
}

/// A minimal 16-bit PCM WAV file.
fn wav_bytes(pcm: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + pcm.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    b.extend_from_slice(&(channels * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

fn first_diff(a: &[u8], b: &[u8]) -> &'static str {
    if a.len() != b.len() {
        "length differs"
    } else {
        "content differs"
    }
}

/// Inflate every entry to `<outdir>/<index>_<hash>.<ext>`. Refuses to write inside the repo.
fn extract(archive: &Path, outdir: &Path) -> Result<bool, String> {
    refuse_repo_path(outdir)?;
    let bytes = std::fs::read(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let a = Archive::parse(&bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(outdir).map_err(|e| format!("{}: {e}", outdir.display()))?;
    let mut ok = 0usize;
    for (i, e) in a.entries.iter().enumerate() {
        match e.decompress(i) {
            Ok(data) => {
                let name = format!("{i:03}_{:08x}.{}", e.hash, sniff_ext(&data));
                std::fs::write(outdir.join(name), &data).map_err(|err| err.to_string())?;
                ok += 1;
            }
            Err(err) => eprintln!("  {err}"),
        }
    }
    println!(
        "{}: extracted {ok}/{} entries to {}",
        archive.display(),
        a.entries.len(),
        outdir.display()
    );
    Ok(ok == a.entries.len())
}

/// Generic container signatures only; anything else is `.bin`.
fn sniff_ext(d: &[u8]) -> &'static str {
    if d.starts_with(b"\x89PNG") {
        "png"
    } else if d.starts_with(b"RIFF") && d.get(8..12) == Some(b"WAVE") {
        "wav"
    } else {
        "bin"
    }
}

/// The repository root this binary was built from (two levels above this crate).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn refuse_repo_path(outdir: &Path) -> Result<(), String> {
    let repo = std::fs::canonicalize(repo_root()).map_err(|e| e.to_string())?;
    // Canonicalize the nearest existing ancestor so not-yet-created folders are checked too.
    let mut probe = outdir.to_path_buf();
    let mut tail = Vec::new();
    while !probe.exists() {
        match (probe.file_name(), probe.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name.to_owned());
                probe = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut full = std::fs::canonicalize(if probe.as_os_str().is_empty() {
        Path::new(".")
    } else {
        &probe
    })
    .map_err(|e| e.to_string())?;
    full.extend(tail.iter().rev());
    if full.starts_with(&repo) {
        return Err(format!(
            "refusing to extract inside the repository ({})",
            full.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_repo_and_allows_outside() {
        assert!(refuse_repo_path(&repo_root().join("target").join("x")).is_err());
        assert!(refuse_repo_path(&repo_root().join("not-yet").join("deeper")).is_err());
        assert!(refuse_repo_path(&std::env::temp_dir().join("sugc-lab-test")).is_ok());
    }

    #[test]
    fn sniffs_generic_containers() {
        assert_eq!(sniff_ext(b"\x89PNG\r\n\x1a\n"), "png");
        assert_eq!(sniff_ext(b"RIFF\0\0\0\0WAVEfmt "), "wav");
        assert_eq!(sniff_ext(b"other"), "bin");
    }
}
