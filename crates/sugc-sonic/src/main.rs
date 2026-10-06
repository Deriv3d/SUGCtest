//! Sonic the Hedgehog from the user's own SUGC disc.
//!
//! ```text
//! sugc-sonic <USRDIR>                                         play in a window
//! sugc-sonic <USRDIR> --headless <frames> [--png <out.png>]   run without a window, print stats
//! ```
//!
//! Keys: arrows = D-pad, Z/X/C = A/B/C, Enter = Start, P = pause.
//!
//! The ROM is read from the user's `flog_u.fpg` at run time; nothing from the game is bundled.

#![forbid(unsafe_code)]

mod app;
mod rom;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(dir) = args.first().map(PathBuf::from) else {
        eprintln!("usage: sugc-sonic <PS3_GAME/USRDIR> --headless <frames> [--png <out.png>]");
        return ExitCode::from(2);
    };
    let rom = match rom::load_sonic1(&dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
    };
    let frames: u64 = flag("--headless").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut m = genesis::Machine::new(rom);
    if frames == 0 {
        return match app::run(m) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    for f in 0..frames {
        m.run_frame();
        if f % 60 == 59 || f + 1 == frames {
            println!("{}", stats(&m));
        }
    }
    if let Some(Err(e)) = flag("--png").map(|out| write_png(Path::new(out), m.frame())) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Numbers only: enough to see whether the game is running without looking at it.
fn stats(m: &genesis::Machine) -> String {
    let frame = m.frame();
    let mut colours = std::collections::HashSet::new();
    let mut non_black = 0usize;
    for px in frame.as_chunks::<4>().0 {
        colours.insert([px[0], px[1], px[2]]);
        if px[0] | px[1] | px[2] != 0 {
            non_black += 1;
        }
    }
    let cram_nonzero = m.hw.vdp.cram.iter().filter(|&&c| c != 0).count();
    let vram_nonzero = m.hw.vdp.vram.iter().filter(|&&b| b != 0).count();
    format!(
        "frame {:5} | pc {:06x} | display {} | width {} | colours {:3} | non-black {:5.1}% | cram {:2}/64 | vram nonzero {:5} | z80 pc {:04x} reset {} busreq {}",
        m.frame_count,
        m.cpu.pc,
        u8::from(m.hw.vdp.display_enabled()),
        m.hw.vdp.width,
        colours.len(),
        100.0 * non_black as f64 / (frame.len() / 4) as f64,
        cram_nonzero,
        vram_nonzero,
        m.z80.pc,
        u8::from(m.hw.z80_reset),
        u8::from(m.hw.z80_busreq),
    )
}

fn write_png(path: &Path, rgba: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(f),
        genesis::vdp::MAX_WIDTH as u32,
        genesis::vdp::HEIGHT as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(rgba).map_err(|e| e.to_string())
}
