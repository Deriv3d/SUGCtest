use super::*;

struct Ram(Vec<u8>, Vec<(u16, u8)>);

impl Bus for Ram {
    fn read(&mut self, a: u16) -> u8 {
        self.0[usize::from(a)]
    }
    fn write(&mut self, a: u16, v: u8) {
        self.0[usize::from(a)] = v;
    }
    fn port_in(&mut self, port: u16) -> u8 {
        self.1
            .iter()
            .find(|(p, _)| *p == port)
            .map_or(0xFF, |(_, v)| *v)
    }
}

#[test]
fn ld_add_djnz_loop() {
    // ld b,4 ; xor a ; loop: add a,3 ; djnz loop ; halt
    let mut m = Ram(vec![0; 0x10000], vec![]);
    m.0[..7].copy_from_slice(&[0x06, 0x04, 0xAF, 0xC6, 0x03, 0x10, 0xFC]);
    m.0[7] = 0x76;
    let mut c = Z80::new();
    let mut t = 0;
    while !c.halted {
        t += c.step(&mut m);
    }
    assert_eq!(c.a, 12);
    assert_eq!(c.b, 0);
    assert_eq!(t, 7 + 4 + 4 * 7 + 3 * 13 + 8 + 4);
}

#[test]
fn im1_interrupt() {
    let mut m = Ram(vec![0; 0x10000], vec![]);
    m.0[0] = 0xFB; // ei
    m.0[1] = 0x00; // nop
    let mut c = Z80::new();
    c.im = 1;
    c.sp = 0x8000;
    c.int_line = true;
    c.step(&mut m); // ei: interrupt held off for one instruction
    assert_eq!(c.pc, 1);
    c.step(&mut m); // nop
    assert_eq!(c.pc, 2);
    let t = c.step(&mut m);
    assert_eq!((c.pc, t, c.iff1), (0x38, 13, false));
}

/// Runs the public single-step Z80 vectors when `SUGC_Z80_TESTS` points at the folder of
/// `*.json` files (kept in the lab, not in this repository).
#[test]
#[ignore = "needs external test vectors; run with --ignored and SUGC_Z80_TESTS set"]
fn single_step_vectors() {
    let Ok(dir) = std::env::var("SUGC_Z80_TESTS") else {
        eprintln!("SUGC_Z80_TESTS not set; skipping");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    files.sort();
    let only = std::env::var("SUGC_Z80_ONLY").ok();
    let (mut total, mut passed, mut t_ok) = (0usize, 0usize, 0usize);
    let mut failing_files = Vec::new();
    for f in files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
    {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        if only.as_ref().is_some_and(|o| !name.starts_with(o.as_str())) {
            continue;
        }
        let tests: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let (mut t, mut p) = (0, 0);
        let mut first = None;
        for case in tests.as_array().unwrap() {
            t += 1;
            let (res, timing_ok) = run_case(case);
            t_ok += usize::from(timing_ok);
            match res {
                Ok(()) => p += 1,
                Err(e) if first.is_none() => first = Some(e),
                Err(_) => {}
            }
        }
        total += t;
        passed += p;
        if p != t {
            failing_files.push(format!(
                "{name:8} {p:5}/{t:5}  {}",
                first.unwrap_or_default()
            ));
        }
    }
    for l in &failing_files {
        println!("{l}");
    }
    println!(
        "TOTAL {passed}/{total}; T-states match {t_ok}/{total}; files with failures {}",
        failing_files.len()
    );
}

fn run_case(case: &serde_json::Value) -> (Result<(), String>, bool) {
    let i = &case["initial"];
    let f = &case["final"];
    let g = |v: &serde_json::Value, k: &str| v[k].as_u64().unwrap();
    let mut m = Ram(vec![0; 0x10000], vec![]);
    for e in i["ram"].as_array().unwrap() {
        m.0[e[0].as_u64().unwrap() as usize] = e[1].as_u64().unwrap() as u8;
    }
    if let Some(ports) = case["ports"].as_array() {
        for p in ports {
            if p[2].as_str() == Some("r") {
                m.1.push((p[0].as_u64().unwrap() as u16, p[1].as_u64().unwrap() as u8));
            }
        }
    }
    let mut c = Z80 {
        a: g(i, "a") as u8,
        f: g(i, "f") as u8,
        b: g(i, "b") as u8,
        c: g(i, "c") as u8,
        d: g(i, "d") as u8,
        e: g(i, "e") as u8,
        h: g(i, "h") as u8,
        l: g(i, "l") as u8,
        af_: g(i, "af_") as u16,
        bc_: g(i, "bc_") as u16,
        de_: g(i, "de_") as u16,
        hl_: g(i, "hl_") as u16,
        ix: g(i, "ix") as u16,
        iy: g(i, "iy") as u16,
        sp: g(i, "sp") as u16,
        pc: g(i, "pc") as u16,
        i: g(i, "i") as u8,
        r: g(i, "r") as u8,
        iff1: g(i, "iff1") != 0,
        iff2: g(i, "iff2") != 0,
        im: g(i, "im") as u8,
        wz: g(i, "wz") as u16,
        q: g(i, "q") as u8,
        ..Default::default()
    };
    let t = c.step(&mut m);
    let want_t = case["cycles"].as_array().map_or(0, Vec::len) as u32;
    let mut errs = Vec::new();
    let checks: [(&str, u64); 21] = [
        ("a", c.a.into()),
        ("f", c.f.into()),
        ("b", c.b.into()),
        ("c", c.c.into()),
        ("d", c.d.into()),
        ("e", c.e.into()),
        ("h", c.h.into()),
        ("l", c.l.into()),
        ("af_", c.af_.into()),
        ("bc_", c.bc_.into()),
        ("de_", c.de_.into()),
        ("hl_", c.hl_.into()),
        ("ix", c.ix.into()),
        ("iy", c.iy.into()),
        ("sp", c.sp.into()),
        ("pc", c.pc.into()),
        ("i", c.i.into()),
        ("r", c.r.into()),
        ("wz", c.wz.into()),
        ("iff1", u64::from(c.iff1)),
        ("iff2", u64::from(c.iff2)),
    ];
    for (k, got) in checks {
        if got != g(f, k) {
            errs.push(format!("{k} {got:x}!={:x}", g(f, k)));
        }
    }
    for e in f["ram"].as_array().unwrap() {
        let a = e[0].as_u64().unwrap() as usize;
        if u64::from(m.0[a]) != e[1].as_u64().unwrap() {
            errs.push(format!("ram[{a:04x}]"));
            break;
        }
    }
    let name = case["name"].as_str().unwrap_or("?");
    let res = if errs.is_empty() {
        Ok(())
    } else {
        Err(format!("{name}: {}", errs.join(" ")))
    };
    (res, t == want_t)
}
