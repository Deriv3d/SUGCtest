//! Locate Sonic the Hedgehog in the user's own `flog_u.fpg` (see knowledge/formats/fpg.md).

use std::path::Path;

use sugc_formats::fpg::Archive;

/// File name the collection uses for Sonic the Hedgehog (worldwide release).
pub const SONIC1_NAME: &str = "SONIC_W.68K";

pub fn load_sonic1(usrdir: &Path) -> Result<Vec<u8>, String> {
    let path = usrdir.join("flog_u.fpg");
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let archive = Archive::parse(&bytes).map_err(|e| e.to_string())?;
    let (index, entry) = archive
        .find(SONIC1_NAME)
        .ok_or_else(|| format!("{SONIC1_NAME} not found in {}", path.display()))?;
    entry.decompress(index).map_err(|e| e.to_string())
}
