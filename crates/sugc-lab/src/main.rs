//! Lab CLI. Runs against the user's own extracted disc files and writes only to a lab folder
//! outside this repository.
//!
//! ```text
//! sugc-lab roundtrip <file>            parse, re-serialize, byte-compare; prints PASS/FAIL
//! sugc-lab extract <archive> <outdir>  inflate every entry into <outdir> (must be outside the repo)
//! ```

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sugc_formats::fpg::Archive;

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
        _ => Err("usage: sugc-lab roundtrip <file> | sugc-lab extract <archive> <outdir>".into()),
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

/// Parse, re-serialize and byte-compare; also inflate every entry and check its size.
fn roundtrip(file: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let archive = Archive::parse(&bytes).map_err(|e| e.to_string())?;
    let rebuilt = archive.to_bytes();
    let identical = rebuilt == bytes;
    let mut inflate_ok = 0usize;
    for (i, e) in archive.entries.iter().enumerate() {
        match e.decompress(i) {
            Ok(_) => inflate_ok += 1,
            Err(err) => eprintln!("  {err}"),
        }
    }
    let n = archive.entries.len();
    let pass = identical && inflate_ok == n;
    println!(
        "{}: {} | entries {n} | re-serialized {} bytes vs {} original, {} | inflate ok {inflate_ok}/{n}",
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
