use std::collections::HashMap;

use super::*;

/// Sparse test memory.
#[derive(Default)]
pub(crate) struct Ram(pub HashMap<u32, u8>);

impl Bus for Ram {
    fn read8(&mut self, a: u32) -> u8 {
        *self.0.get(&(a & 0xFF_FFFF)).unwrap_or(&0)
    }
    fn read16(&mut self, a: u32) -> u16 {
        u16::from_be_bytes([self.read8(a), self.read8(a + 1)])
    }
    fn write8(&mut self, a: u32, v: u8) {
        self.0.insert(a & 0xFF_FFFF, v);
    }
    fn write16(&mut self, a: u32, v: u16) {
        let [h, l] = v.to_be_bytes();
        self.write8(a, h);
        self.write8(a + 1, l);
    }
}

fn load(ram: &mut Ram, at: u32, words: &[u16]) {
    for (i, w) in words.iter().enumerate() {
        ram.write16(at + 2 * i as u32, *w);
    }
}

fn cpu_at(pc: u32) -> Cpu {
    let mut c = Cpu::new();
    c.pc = pc;
    c.a[7] = 0x1000;
    c
}

#[test]
fn moveq_addq_dbra_loop() {
    // moveq #3,d0 ; moveq #0,d1 ; loop: addq.l #2,d1 ; dbra d0,loop
    let mut ram = Ram::default();
    load(&mut ram, 0x100, &[0x7003, 0x7200, 0x5481, 0x51C8, 0xFFFC]);
    let mut c = cpu_at(0x100);
    for _ in 0..2 + 4 * 2 {
        c.step(&mut ram);
    }
    assert_eq!(c.d[1], 8);
    assert_eq!(c.d[0] & 0xFFFF, 0xFFFF);
    assert_eq!(c.pc, 0x10A);
}

#[test]
fn bsr_rts_and_stack() {
    // bsr.s +2 ; nop ; (target) rts
    let mut ram = Ram::default();
    load(&mut ram, 0x200, &[0x6102, 0x4E71, 0x4E75]);
    let mut c = cpu_at(0x200);
    c.step(&mut ram);
    assert_eq!(c.pc, 0x204);
    assert_eq!(c.a[7], 0x0FFC);
    c.step(&mut ram);
    assert_eq!(c.pc, 0x202);
    assert_eq!(c.a[7], 0x1000);
}

#[test]
fn abcd_and_flags() {
    // abcd d1,d0 with d0=0x19, d1=0x23, X=0 -> 0x42
    let mut ram = Ram::default();
    load(&mut ram, 0x100, &[0xC101]);
    let mut c = cpu_at(0x100);
    c.d[0] = 0x19;
    c.d[1] = 0x23;
    c.sr |= sr::Z;
    c.step(&mut ram);
    assert_eq!(c.d[0] & 0xFF, 0x42);
    assert_eq!(c.sr & (sr::C | sr::X | sr::Z), 0);
}

#[test]
fn interrupt_autovector() {
    let mut ram = Ram::default();
    ram.write16(0x70, 0); // vector 28 (level 4) -> 0x0000_0400
    ram.write16(0x72, 0x400);
    load(&mut ram, 0x100, &[0x4E71]);
    let mut c = cpu_at(0x100);
    c.sr = sr::S | 0x0300;
    c.irq_level = 4;
    c.step(&mut ram);
    assert_eq!(c.pc, 0x400);
    assert_eq!((c.sr >> 8) & 7, 4);
    assert_eq!(c.a[7], 0x1000 - 6);
}

#[test]
fn divu_by_zero_traps() {
    let mut ram = Ram::default();
    ram.write16(0x14, 0);
    ram.write16(0x16, 0x500);
    load(&mut ram, 0x100, &[0x80C1]); // divu d1,d0
    let mut c = cpu_at(0x100);
    c.d[0] = 10;
    c.step(&mut ram);
    assert_eq!(c.pc, 0x500);
}

/// Runs the public single-step test vectors when `SUGC_M68K_TESTS` points at a folder of
/// `*.json.gz` files (kept in the lab, not in this repository). Prints per-file pass rates.
#[test]
#[ignore = "needs external test vectors; run with --ignored and SUGC_M68K_TESTS set"]
fn single_step_vectors() {
    use std::io::Read;
    let Ok(dir) = std::env::var("SUGC_M68K_TESTS") else {
        eprintln!("SUGC_M68K_TESTS not set; skipping");
        return;
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    files.sort();
    let only = std::env::var("SUGC_M68K_ONLY").ok();
    let (mut total, mut passed, mut ae_total, mut ae_passed) = (0usize, 0usize, 0usize, 0usize);
    let mut report = Vec::new();
    for f in files
        .iter()
        .filter(|p| p.to_string_lossy().ends_with(".json.gz"))
    {
        let name = f
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".json.gz", "");
        if only.as_ref().is_some_and(|o| !name.starts_with(o.as_str())) {
            continue;
        }
        let mut s = String::new();
        flate2::read::GzDecoder::new(std::fs::File::open(f).unwrap())
            .read_to_string(&mut s)
            .unwrap();
        let tests: serde_json::Value = serde_json::from_str(&s).unwrap();
        let (mut t, mut p, mut ae_t, mut ae_p) = (0, 0, 0, 0);
        let mut first_fail = None;
        for case in tests.as_array().unwrap() {
            let ae = expects_address_error(case);
            let ok = run_case(case);
            if ae {
                ae_t += 1;
                ae_p += usize::from(ok.is_ok());
                continue;
            }
            t += 1;
            match ok {
                Ok(()) => p += 1,
                Err(e) => {
                    if first_fail.is_none() {
                        first_fail = Some(format!("{}: {e}", case["name"].as_str().unwrap_or("?")));
                    }
                }
            }
        }
        total += t;
        passed += p;
        ae_total += ae_t;
        ae_passed += ae_p;
        report.push(format!(
            "{name:12} {p:5}/{t:5}  (addr-error cases {ae_p}/{ae_t})  {}",
            first_fail.unwrap_or_default()
        ));
    }
    for line in &report {
        println!("{line}");
    }
    println!(
        "TOTAL (excluding address-error cases) {passed}/{total}; address-error cases {ae_passed}/{ae_total}"
    );
}

/// True when the expected outcome is an address-error exception (PC ends at vector 3).
fn expects_address_error(case: &serde_json::Value) -> bool {
    let mut v = [0u8; 4];
    for e in case["initial"]["ram"].as_array().unwrap() {
        let a = e[0].as_u64().unwrap();
        if (12..16).contains(&a) {
            v[(a - 12) as usize] = e[1].as_u64().unwrap() as u8;
        }
    }
    let vec3 = u32::from_be_bytes(v);
    case["final"]["pc"].as_u64().unwrap() as u32 == vec3 && vec3 != 0
}

fn run_case(case: &serde_json::Value) -> Result<(), String> {
    let init = &case["initial"];
    let fin = &case["final"];
    let g = |v: &serde_json::Value, k: &str| v[k].as_u64().unwrap() as u32;
    let mut ram = Ram::default();
    for e in init["ram"].as_array().unwrap() {
        ram.write8(e[0].as_u64().unwrap() as u32, e[1].as_u64().unwrap() as u8);
    }
    let pc = g(init, "pc");
    let pf = init["prefetch"].as_array().unwrap();
    ram.write16(pc, pf[0].as_u64().unwrap() as u16);
    ram.write16(pc + 2, pf[1].as_u64().unwrap() as u16);
    let mut c = Cpu::new();
    for i in 0..8 {
        c.d[i] = g(init, &format!("d{i}"));
    }
    for i in 0..7 {
        c.a[i] = g(init, &format!("a{i}"));
    }
    c.sr = g(init, "sr") as u16;
    c.usp = g(init, "usp");
    c.ssp = g(init, "ssp");
    c.a[7] = if c.supervisor() { c.ssp } else { c.usp };
    c.pc = pc;
    c.step(&mut ram);
    let mut errs = Vec::new();
    for i in 0..8 {
        let want = g(fin, &format!("d{i}"));
        if c.d[i] != want {
            errs.push(format!("d{i} {:08x}!={want:08x}", c.d[i]));
        }
    }
    for i in 0..7 {
        let want = g(fin, &format!("a{i}"));
        if c.a[i] != want {
            errs.push(format!("a{i} {:08x}!={want:08x}", c.a[i]));
        }
    }
    if c.user_sp() != g(fin, "usp") {
        errs.push(format!("usp {:08x}!={:08x}", c.user_sp(), g(fin, "usp")));
    }
    if c.supervisor_sp() != g(fin, "ssp") {
        errs.push(format!(
            "ssp {:08x}!={:08x}",
            c.supervisor_sp(),
            g(fin, "ssp")
        ));
    }
    if u32::from(c.sr) != g(fin, "sr") {
        errs.push(format!("sr {:04x}!={:04x}", c.sr, g(fin, "sr")));
    }
    if c.pc != g(fin, "pc") {
        errs.push(format!("pc {:06x}!={:06x}", c.pc, g(fin, "pc")));
    }
    for e in fin["ram"].as_array().unwrap() {
        let a = e[0].as_u64().unwrap() as u32;
        let want = e[1].as_u64().unwrap() as u8;
        let got = ram.read8(a);
        if got != want {
            errs.push(format!("ram[{a:06x}] {got:02x}!={want:02x}"));
            break;
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join(" "))
    }
}
