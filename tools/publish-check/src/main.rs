//! publish-check: refuse to let game-derived data into the repository.
//!
//! Modelled on universal-modder's `um publish check`, but stricter: everything that
//! `um` only warns about is a failure here, and it adds PS3 and Genesis ROM detection.
//!
//!     publish-check [--staged] [--game-dir <extracted lab dir>] [<repo root>]
//!
//! It is a lint, not legal advice. The rule it enforces is simple: the repo holds our
//! own source and documentation only. Anything binary must be on the allowlist, and
//! nothing may look like a disc image, PS3 executable, SFO, key, or ROM.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use regex::Regex;
use sha2::{Digest, Sha256};

/// Extensions that are never allowed, whatever their contents.
const BANNED_EXT: &[&str] = &[
    // disc images and PS3 containers
    "iso", "dec", "pkg", "self", "sprx", "sdat", "edat", "rap", "pup", "sfo", "trp", "psarc",
    // Genesis / Mega Drive / Master System ROM images (.md is Markdown here; a binary
    // .md ROM is still caught by its header or by the binary allowlist)
    "bin", "gen", "smd", "sms", "gg", "32x", "rom", "sgd", "mdx",
    // memory and capture dumps belong in the lab, never in the repo
    "dmp", "rdc", "wav", "flac", "ogg", "at3", "vag", "gtf", "dds",
    // Ghidra databases and exports
    "gpr", "rep", "gzf", "gar",
];

/// Binary files are refused unless their path matches one of these prefixes.
/// Keep this list short. Our own assets (icons, docs images) go under assets/own/.
const BINARY_ALLOW_PREFIX: &[&str] = &["assets/own/", "docs/img/"];

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".venv", "__pycache__"];

const TEXT_EXT: &[&str] = &[
    "rs", "toml", "md", "txt", "yml", "yaml", "json", "py", "ps1", "sh", "bat", "cfg", "ini",
    "wgsl", "glsl", "hlsl", "java", "c", "h", "s", "asm", "lock",
];

struct Finding {
    path: String,
    why: String,
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut staged = false;
    let mut game_dir: Option<PathBuf> = None;
    let mut root = PathBuf::from(".");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--staged" => staged = true,
            "--game-dir" => game_dir = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("usage: publish-check [--staged] [--game-dir DIR] [ROOT]");
                return ExitCode::SUCCESS;
            }
            other => root = PathBuf::from(other),
        }
    }

    let files = if staged {
        staged_files(&root)
    } else {
        walk(&root)
    };
    let game_index = game_dir.as_deref().map(index_game_dir);
    let mut fails = Vec::new();
    for rel in &files {
        let full = root.join(rel);
        let Ok(data) = fs::read(&full) else { continue };
        check_file(rel, &data, game_index.as_ref(), &mut fails);
    }

    for f in &fails {
        println!("FAIL  {}: {}", f.path, f.why);
    }
    println!(
        "{}: {} files checked, {} failures",
        if fails.is_empty() { "PASS" } else { "FAIL" },
        files.len(),
        fails.len()
    );
    if fails.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check_file(rel: &str, data: &[u8], game: Option<&GameIndex>, out: &mut Vec<Finding>) {
    let mut fail = |why: String| {
        out.push(Finding {
            path: rel.to_string(),
            why,
        })
    };
    let ext = Path::new(rel)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let name = Path::new(rel)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if BANNED_EXT.contains(&ext.as_str()) {
        fail(format!("banned extension .{ext}"));
    }
    if name.eq_ignore_ascii_case("EBOOT.BIN") || name.eq_ignore_ascii_case("PARAM.SFO") {
        fail("PS3 game file name".into());
    }
    if let Some(why) = sniff_magic(data) {
        fail(why);
    }
    if let Some(game) = game
        && let Some(src) = game.matches(data)
    {
        fail(format!("content matches extracted game data ({src})"));
    }

    let is_text = TEXT_EXT.contains(&ext.as_str()) || std::str::from_utf8(data).is_ok();
    if !is_text {
        if !BINARY_ALLOW_PREFIX.iter().any(|p| rel.starts_with(p)) {
            fail(format!(
                "binary file outside the allowlist ({} bytes)",
                data.len()
            ));
        }
        return;
    }
    let text = String::from_utf8_lossy(data);
    for (label, rx) in text_rules() {
        if let Some(m) = rx.find(&text) {
            fail(format!("{label}: {:?}", truncate(m.as_str(), 40)));
        }
    }
}

/// Recognize file formats we must never commit, by content rather than name.
fn sniff_magic(d: &[u8]) -> Option<String> {
    let at = |off: usize, pat: &[u8]| d.len() >= off + pat.len() && &d[off..off + pat.len()] == pat;
    if at(0, b"SCE\0") {
        return Some("PS3 SELF/SPRX header".into());
    }
    if at(0, b"\x7fPKG") {
        return Some("PS3 PKG header".into());
    }
    if at(0, b"\0PSF") {
        return Some("PSF / PARAM.SFO header".into());
    }
    if at(0, b"NPD\0") {
        return Some("NPDRM (EDAT/SDAT) header".into());
    }
    if at(0x8001, b"CD001") {
        return Some("ISO 9660 volume descriptor".into());
    }
    // Big-endian ELF for PowerPC64 (PPU) or SPU (machine 0x17).
    if at(0, b"\x7fELF") && d.len() > 0x13 && d[5] == 2 {
        let machine = u16::from_be_bytes([d[0x12], d[0x13]]);
        if machine == 0x15 || machine == 0x17 {
            return Some(format!(
                "big-endian ELF for PPU/SPU (e_machine {machine:#x})"
            ));
        }
    }
    // Mega Drive / Genesis cartridge header: system type at 0x100.
    if at(0x100, b"SEGA") {
        return Some("Mega Drive / Genesis ROM header (\"SEGA\" at 0x100)".into());
    }
    // Same header byte-swapped (some dumps are stored that way).
    if at(0x100, b"ESAG") {
        return Some("byte-swapped Mega Drive ROM header".into());
    }
    // SMD interleaved dump: 512-byte header with 0xAA 0xBB at offset 8.
    if d.len() >= 0x4200 && d.len() % 0x4000 == 0x200 && at(8, &[0xAA, 0xBB]) {
        return Some("SMD interleaved ROM dump".into());
    }
    // Master System / Game Gear header.
    for off in [0x1ff0usize, 0x3ff0, 0x7ff0] {
        if at(off, b"TMR SEGA") {
            return Some("Master System / Game Gear ROM header".into());
        }
    }
    None
}

fn text_rules() -> Vec<(&'static str, Regex)> {
    vec![
        ("Ghidra auto-name (decompiled code?)", Regex::new(r"\b(?:FUN|DAT|LAB|PTR|UNK)_[0-9a-fA-F]{6,}\b").unwrap()),
        ("IDA auto-name (decompiled code?)", Regex::new(r"\b(?:sub|loc|unk|off|dword|qword)_[0-9A-F]{5,}\b").unwrap()),
        ("decompiler output header", Regex::new(r"(?m)^\s*(?://|#)\s*(?:Decompiled with|Decompiler:|WARNING: Removing unreachable block)").unwrap()),
        ("key material", Regex::new(r"(?i)\b(?:erk|riv|klic|klicensee|idps|psid|act\.dat|rif|aes[_-]?key)\b[^\n]{0,24}[0-9a-f]{32,}").unwrap()),
        ("long hex blob (dumped data?)", Regex::new(r"(?i)(?:[0-9a-f]{2}[ ,]?){256,}").unwrap()),
        ("private key", Regex::new(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----").unwrap()),
        ("GitHub token", Regex::new(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{60,})").unwrap()),
        ("Anthropic key", Regex::new(r"sk-ant-[A-Za-z0-9_\-]{20,}").unwrap()),
        ("absolute user path", Regex::new(r#"[A-Za-z]:\\Users\\[^\\\s"']+|/home/[a-z_][a-z0-9_-]*/|/Users/[A-Za-z]+/"#).unwrap()),
    ]
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
}

/// Hashes of the locally extracted game files, whole-file and in 4 KiB blocks, so a
/// renamed or partially copied game file is caught too.
struct GameIndex {
    whole: HashMap<[u8; 32], String>,
    blocks: HashSet<[u8; 32]>,
}

const BLOCK: usize = 4096;

impl GameIndex {
    fn matches(&self, data: &[u8]) -> Option<String> {
        if let Some(src) = self.whole.get(&sha(data)) {
            return Some(src.clone());
        }
        // Aligned 4 KiB blocks; skip blocks that are all one byte value (padding).
        data.as_chunks::<BLOCK>()
            .0
            .iter()
            .filter(|c| c.iter().any(|&b| b != c[0]))
            .find(|c| self.blocks.contains(&sha(*c)))
            .map(|_| "a 4 KiB block of an extracted file".into())
    }
}

fn index_game_dir(dir: &Path) -> GameIndex {
    let mut idx = GameIndex {
        whole: HashMap::new(),
        blocks: HashSet::new(),
    };
    for rel in walk(dir) {
        let Ok(mut f) = fs::File::open(dir.join(&rel)) else {
            continue;
        };
        let mut data = Vec::new();
        if f.read_to_end(&mut data).is_err() {
            continue;
        }
        idx.whole.insert(sha(&data), rel.clone());
        for c in data.as_chunks::<BLOCK>().0 {
            if c.iter().any(|&b| b != c[0]) {
                idx.blocks.insert(sha(c));
            }
        }
    }
    idx
}

fn sha(d: &[u8]) -> [u8; 32] {
    Sha256::digest(d).into()
}

fn walk(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(p);
                }
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn staged_files(root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
        .current_dir(root)
        .output()
        .expect("git not available");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fails(rel: &str, data: &[u8]) -> Vec<String> {
        let mut v = Vec::new();
        check_file(rel, data, None, &mut v);
        v.into_iter().map(|f| f.why).collect()
    }

    /// A synthetic image with a Genesis-style header. Our own bytes, not a real ROM.
    fn fake_md_header() -> Vec<u8> {
        let mut d = vec![0u8; 0x200];
        d[0x100..0x110].copy_from_slice(b"SEGA SYNTHETIC  ");
        d
    }

    #[test]
    fn rejects_genesis_header_under_any_name() {
        assert!(!fails("tests/data/notarom.txt", &fake_md_header()).is_empty());
    }

    #[test]
    fn rejects_banned_extension() {
        assert!(
            fails("x/game.gen", b"hello")
                .iter()
                .any(|w| w.contains("banned extension"))
        );
    }

    #[test]
    fn rejects_self_and_iso() {
        assert!(!fails("a", b"SCE\0rest").is_empty());
        let mut iso = vec![0u8; 0x8010];
        iso[0x8001..0x8006].copy_from_slice(b"CD001");
        assert!(!fails("a", &iso).is_empty());
    }

    #[test]
    fn rejects_ghidra_names_in_source() {
        // Built at runtime so this file does not trip its own check.
        let src = format!("fn x() {{ {}_00012340(); }}", "FUN");
        assert!(!fails("src/lib.rs", src.as_bytes()).is_empty());
    }

    #[test]
    fn rejects_unlisted_binary() {
        assert!(!fails("src/blob", &[0xff, 0xfe, 0x00, 0x81, 0x00]).is_empty());
    }

    #[test]
    fn accepts_plain_source() {
        assert!(
            fails(
                "src/lib.rs",
                b"pub fn add(a: u32, b: u32) -> u32 { a + b }\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn game_index_catches_partial_copy() {
        let block: Vec<u8> = (0..BLOCK).map(|i| (i * 7 % 251) as u8).collect();
        let mut idx = GameIndex {
            whole: HashMap::new(),
            blocks: HashSet::new(),
        };
        idx.blocks.insert(sha(&block));
        let mut v = Vec::new();
        check_file("assets/own/pic.png", &block, Some(&idx), &mut v);
        assert!(v.iter().any(|f| f.why.contains("extracted game data")));
    }
}
