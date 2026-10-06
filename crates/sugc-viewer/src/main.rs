//! Browse the user's own Sonic's Ultimate Genesis Collection disc.
//!
//! ```text
//! sugc-viewer <USRDIR>           open the browser window
//! sugc-viewer <USRDIR> --check   decode every asset once, print counts, no window
//! ```
//!
//! `<USRDIR>` is `PS3_GAME/USRDIR` of the user's own extracted disc. Nothing is written.

#![forbid(unsafe_code)]

mod source;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, check) = match args.as_slice() {
        [dir] => (PathBuf::from(dir), false),
        [dir, flag] if flag == "--check" => (PathBuf::from(dir), true),
        _ => {
            eprintln!("usage: sugc-viewer <PS3_GAME/USRDIR> [--check]");
            return ExitCode::from(2);
        }
    };
    let src = match source::SugcSource::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if check {
        let r = asset_browser::load_all(&src);
        println!(
            "assets {} | images {} | audio {} | text {} | info {} | failed {}",
            asset_browser::AssetSource::entries(&src).len(),
            r.images,
            r.audio,
            r.text,
            r.info,
            r.failed.len()
        );
        for (id, e) in r.failed.iter().take(10) {
            println!("  failed {id}: {e}");
        }
        return if r.failed.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    match asset_browser::run("SUGC asset viewer", Box::new(src)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
