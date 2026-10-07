//! Localized string table (see `knowledge/formats/strings.md`): little-endian count N, then
//! N absolute u32 offsets, then NUL-terminated UTF-8 strings; the last string runs to EOF.

/// Parse a string table. Returns `None` if the layout does not fit.
pub fn parse(bytes: &[u8]) -> Option<Vec<String>> {
    let n = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    let table_end = 4usize.checked_add(n.checked_mul(4)?)?;
    if n == 0 || table_end > bytes.len() {
        return None;
    }
    let offsets: Vec<usize> = bytes[4..table_end]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c) as usize)
        .collect();
    if offsets[0] != table_end
        || offsets.windows(2).any(|w| w[1] < w[0])
        || *offsets.last()? > bytes.len()
    {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for (i, &start) in offsets.iter().enumerate() {
        let end = offsets.get(i + 1).copied().unwrap_or(bytes.len());
        let raw = &bytes[start..end];
        let raw = raw.split(|&b| b == 0).next().unwrap_or(raw);
        out.push(String::from_utf8_lossy(raw).into_owned());
    }
    Some(out)
}

/// Build a string table (used by tests and future tooling).
pub fn build(strings: &[&str]) -> Vec<u8> {
    let n = strings.len();
    let mut out = (n as u32).to_le_bytes().to_vec();
    let mut pos = 4 + 4 * n;
    for s in strings {
        out.extend_from_slice(&(pos as u32).to_le_bytes());
        pos += s.len() + 1;
    }
    for s in strings {
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let t = build(&["first", "", "zweite Zeile", "last"]);
        assert_eq!(
            parse(&t).unwrap(),
            vec!["first", "", "zweite Zeile", "last"]
        );
    }

    #[test]
    fn rejects_non_tables() {
        assert!(parse(b"").is_none());
        assert!(parse(&[0, 0, 0, 0]).is_none());
        assert!(parse(&[0xFF, 0xFF, 0, 0, 1, 2, 3, 4]).is_none());
        let mut t = build(&["a", "b"]);
        t[4] = 99; // first offset must equal the end of the table
        assert!(parse(&t).is_none());
    }
}
