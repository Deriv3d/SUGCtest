//! Zilog Z80 interpreter with T-state counts, written from the Zilog user manual plus the
//! publicly documented undocumented behaviour (flag bits 3/5, MEMPTR/WZ, Q, block-repeat flags).

#![forbid(unsafe_code)]

/// Memory, I/O and interrupt interface seen by the CPU.
pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);
    fn port_in(&mut self, _port: u16) -> u8 {
        0xFF
    }
    fn port_out(&mut self, _port: u16, _value: u8) {}
    /// Byte placed on the data bus during interrupt acknowledge (IM 0 / IM 2).
    fn int_data(&mut self) -> u8 {
        0xFF
    }
}

pub const FC: u8 = 0x01;
pub const FN: u8 = 0x02;
pub const FP: u8 = 0x04;
pub const FX: u8 = 0x08;
pub const FH: u8 = 0x10;
pub const FY: u8 = 0x20;
pub const FZ: u8 = 0x40;
pub const FS: u8 = 0x80;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Z80 {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub af_: u16,
    pub bc_: u16,
    pub de_: u16,
    pub hl_: u16,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,
    pub r: u8,
    pub iff1: bool,
    pub iff2: bool,
    pub im: u8,
    pub halted: bool,
    /// Internal MEMPTR register.
    pub wz: u16,
    /// Flags written by the last instruction (0 if it did not write flags).
    pub q: u8,
    /// True right after EI: interrupts are not accepted before the next instruction.
    pub ei_pending: bool,
    /// Maskable interrupt line (level).
    pub int_line: bool,
    /// Total T-states run.
    pub cycles: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Idx {
    Hl,
    Ix,
    Iy,
}

fn parity(v: u8) -> bool {
    v.count_ones().is_multiple_of(2)
}

fn szp(v: u8) -> u8 {
    let mut f = v & (FS | FX | FY);
    if v == 0 {
        f |= FZ;
    }
    if parity(v) {
        f |= FP;
    }
    f
}

impl Z80 {
    pub fn new() -> Z80 {
        Z80 {
            a: 0xFF,
            f: 0xFF,
            sp: 0xFFFF,
            ..Default::default()
        }
    }

    pub fn reset(&mut self) {
        self.pc = 0;
        self.i = 0;
        self.r = 0;
        self.iff1 = false;
        self.iff2 = false;
        self.im = 0;
        self.halted = false;
        self.ei_pending = false;
    }

    // ---- register pairs ----
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }
    pub fn set_bc(&mut self, v: u16) {
        [self.b, self.c] = v.to_be_bytes();
    }
    pub fn set_de(&mut self, v: u16) {
        [self.d, self.e] = v.to_be_bytes();
    }
    pub fn set_hl(&mut self, v: u16) {
        [self.h, self.l] = v.to_be_bytes();
    }
    pub fn set_af(&mut self, v: u16) {
        [self.a, self.f] = v.to_be_bytes();
    }

    fn inc_r(&mut self) {
        self.r = (self.r & 0x80) | (self.r.wrapping_add(1) & 0x7F);
    }

    fn fetch(&mut self, bus: &mut impl Bus) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    fn fetch16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.fetch(bus);
        let hi = self.fetch(bus);
        u16::from_le_bytes([lo, hi])
    }

    fn read16(bus: &mut impl Bus, a: u16) -> u16 {
        u16::from_le_bytes([bus.read(a), bus.read(a.wrapping_add(1))])
    }

    fn write16(bus: &mut impl Bus, a: u16, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        bus.write(a, lo);
        bus.write(a.wrapping_add(1), hi);
    }

    fn push(&mut self, bus: &mut impl Bus, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, hi);
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, lo);
    }

    fn pop(&mut self, bus: &mut impl Bus) -> u16 {
        let v = Self::read16(bus, self.sp);
        self.sp = self.sp.wrapping_add(2);
        v
    }

    fn idx_reg(&self, idx: Idx) -> u16 {
        match idx {
            Idx::Hl => self.hl(),
            Idx::Ix => self.ix,
            Idx::Iy => self.iy,
        }
    }

    fn set_idx_reg(&mut self, idx: Idx, v: u16) {
        match idx {
            Idx::Hl => self.set_hl(v),
            Idx::Ix => self.ix = v,
            Idx::Iy => self.iy = v,
        }
    }

    /// 8-bit register by index (6 = not handled here). H/L map to IXH/IXL with a prefix.
    fn get_r(&self, r: u8, idx: Idx) -> u8 {
        match r {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => (self.idx_reg(idx) >> 8) as u8,
            5 => self.idx_reg(idx) as u8,
            7 => self.a,
            _ => unreachable!(),
        }
    }

    fn set_r(&mut self, r: u8, idx: Idx, v: u8) {
        match r {
            0 => self.b = v,
            1 => self.c = v,
            2 => self.d = v,
            3 => self.e = v,
            4 => {
                let x = self.idx_reg(idx);
                self.set_idx_reg(idx, (x & 0x00FF) | (u16::from(v) << 8));
            }
            5 => {
                let x = self.idx_reg(idx);
                self.set_idx_reg(idx, (x & 0xFF00) | u16::from(v));
            }
            7 => self.a = v,
            _ => unreachable!(),
        }
    }

    fn get_rp(&self, p: u8, idx: Idx) -> u16 {
        match p {
            0 => self.bc(),
            1 => self.de(),
            2 => self.idx_reg(idx),
            _ => self.sp,
        }
    }

    fn set_rp(&mut self, p: u8, idx: Idx, v: u16) {
        match p {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_idx_reg(idx, v),
            _ => self.sp = v,
        }
    }

    fn get_rp2(&self, p: u8, idx: Idx) -> u16 {
        if p == 3 {
            self.af()
        } else {
            self.get_rp(p, idx)
        }
    }

    fn set_rp2(&mut self, p: u8, idx: Idx, v: u16) {
        if p == 3 {
            self.set_af(v)
        } else {
            self.set_rp(p, idx, v)
        }
    }

    fn cond(&self, cc: u8) -> bool {
        let f = self.f;
        match cc {
            0 => f & FZ == 0,
            1 => f & FZ != 0,
            2 => f & FC == 0,
            3 => f & FC != 0,
            4 => f & FP == 0,
            5 => f & FP != 0,
            6 => f & FS == 0,
            _ => f & FS != 0,
        }
    }

    /// Address for (HL) or (IX+d)/(IY+d); fetches the displacement for indexed forms.
    fn mem_addr(&mut self, bus: &mut impl Bus, idx: Idx) -> u16 {
        match idx {
            Idx::Hl => self.hl(),
            _ => {
                let d = self.fetch(bus) as i8;
                let a = self.idx_reg(idx).wrapping_add(d as u16);
                self.wz = a;
                a
            }
        }
    }

    fn setf(&mut self, f: u8) {
        self.f = f;
        self.q = f;
    }

    // ---- ALU ----
    fn add8(&mut self, v: u8, carry: bool) {
        let c = u8::from(carry && self.f & FC != 0);
        let a = self.a;
        let res16 = u16::from(a) + u16::from(v) + u16::from(c);
        let res = res16 as u8;
        let mut f = res & (FS | FX | FY);
        if res == 0 {
            f |= FZ;
        }
        if (a & 0xF) + (v & 0xF) + c > 0xF {
            f |= FH;
        }
        if (!(a ^ v) & (a ^ res)) & 0x80 != 0 {
            f |= FP;
        }
        if res16 > 0xFF {
            f |= FC;
        }
        self.a = res;
        self.setf(f);
    }

    fn sub8(&mut self, v: u8, carry: bool, store: bool) {
        let c = u8::from(carry && self.f & FC != 0);
        let a = self.a;
        let res16 = u16::from(a)
            .wrapping_sub(u16::from(v))
            .wrapping_sub(u16::from(c));
        let res = res16 as u8;
        let mut f = FN | (res & FS);
        if store {
            f |= res & (FX | FY);
        } else {
            f |= v & (FX | FY);
        }
        if res == 0 {
            f |= FZ;
        }
        if (a & 0xF) < (v & 0xF) + c {
            f |= FH;
        }
        if ((a ^ v) & (a ^ res)) & 0x80 != 0 {
            f |= FP;
        }
        if res16 > 0xFF {
            f |= FC;
        }
        if store {
            self.a = res;
        }
        self.setf(f);
    }

    fn alu(&mut self, op: u8, v: u8) {
        match op {
            0 => self.add8(v, false),
            1 => self.add8(v, true),
            2 => self.sub8(v, false, true),
            3 => self.sub8(v, true, true),
            4 => {
                self.a &= v;
                let f = szp(self.a) | FH;
                self.setf(f);
            }
            5 => {
                self.a ^= v;
                let f = szp(self.a);
                self.setf(f);
            }
            6 => {
                self.a |= v;
                let f = szp(self.a);
                self.setf(f);
            }
            _ => self.sub8(v, false, false),
        }
    }

    fn inc8(&mut self, v: u8) -> u8 {
        let r = v.wrapping_add(1);
        let mut f = (self.f & FC) | (r & (FS | FX | FY));
        if r == 0 {
            f |= FZ;
        }
        if v & 0xF == 0xF {
            f |= FH;
        }
        if v == 0x7F {
            f |= FP;
        }
        self.setf(f);
        r
    }

    fn dec8(&mut self, v: u8) -> u8 {
        let r = v.wrapping_sub(1);
        let mut f = (self.f & FC) | FN | (r & (FS | FX | FY));
        if r == 0 {
            f |= FZ;
        }
        if v & 0xF == 0 {
            f |= FH;
        }
        if v == 0x80 {
            f |= FP;
        }
        self.setf(f);
        r
    }

    fn add16(&mut self, a: u16, b: u16) -> u16 {
        let res = u32::from(a) + u32::from(b);
        let mut f = self.f & (FS | FZ | FP);
        f |= ((res >> 8) as u8) & (FX | FY);
        if (a & 0xFFF) + (b & 0xFFF) > 0xFFF {
            f |= FH;
        }
        if res > 0xFFFF {
            f |= FC;
        }
        self.wz = a.wrapping_add(1);
        self.setf(f);
        res as u16
    }

    fn adc16(&mut self, b: u16) {
        let a = self.hl();
        let c = u32::from(self.f & FC);
        let res = u32::from(a) + u32::from(b) + c;
        let r = res as u16;
        let mut f = ((r >> 8) as u8) & (FS | FX | FY);
        if r == 0 {
            f |= FZ;
        }
        if (u32::from(a & 0xFFF) + u32::from(b & 0xFFF) + c) > 0xFFF {
            f |= FH;
        }
        if (!(a ^ b) & (a ^ r)) & 0x8000 != 0 {
            f |= FP;
        }
        if res > 0xFFFF {
            f |= FC;
        }
        self.wz = a.wrapping_add(1);
        self.set_hl(r);
        self.setf(f);
    }

    fn sbc16(&mut self, b: u16) {
        let a = self.hl();
        let c = u32::from(self.f & FC);
        let res = u32::from(a).wrapping_sub(u32::from(b)).wrapping_sub(c);
        let r = res as u16;
        let mut f = FN | (((r >> 8) as u8) & (FS | FX | FY));
        if r == 0 {
            f |= FZ;
        }
        if u32::from(a & 0xFFF) < u32::from(b & 0xFFF) + c {
            f |= FH;
        }
        if ((a ^ b) & (a ^ r)) & 0x8000 != 0 {
            f |= FP;
        }
        if res > 0xFFFF {
            f |= FC;
        }
        self.wz = a.wrapping_add(1);
        self.set_hl(r);
        self.setf(f);
    }

    /// CB-prefix rotate/shift `y` on value `v`.
    fn rot(&mut self, y: u8, v: u8) -> u8 {
        let c = self.f & FC != 0;
        let (r, out) = match y {
            0 => (v.rotate_left(1), v & 0x80 != 0),
            1 => (v.rotate_right(1), v & 1 != 0),
            2 => ((v << 1) | u8::from(c), v & 0x80 != 0),
            3 => ((v >> 1) | if c { 0x80 } else { 0 }, v & 1 != 0),
            4 => (v << 1, v & 0x80 != 0),
            5 => ((v >> 1) | (v & 0x80), v & 1 != 0),
            6 => ((v << 1) | 1, v & 0x80 != 0),
            _ => (v >> 1, v & 1 != 0),
        };
        let f = szp(r) | if out { FC } else { 0 };
        self.setf(f);
        r
    }

    fn bit(&mut self, n: u8, v: u8, xy_src: u8) {
        let set = v & (1 << n) != 0;
        let mut f = (self.f & FC) | FH | (xy_src & (FX | FY));
        if !set {
            f |= FZ | FP;
        }
        if n == 7 && set {
            f |= FS;
        }
        self.setf(f);
    }

    fn daa(&mut self) {
        let a = self.a;
        let mut corr = 0u8;
        let mut c = self.f & FC != 0;
        if self.f & FH != 0 || a & 0xF > 9 {
            corr |= 0x06;
        }
        if c || a > 0x99 {
            corr |= 0x60;
            c = true;
        }
        let n = self.f & FN != 0;
        let r = if n {
            a.wrapping_sub(corr)
        } else {
            a.wrapping_add(corr)
        };
        let h = if n {
            self.f & FH != 0 && a & 0xF < 6
        } else {
            a & 0xF > 9
        };
        let mut f = szp(r) | (self.f & FN);
        if h {
            f |= FH;
        }
        if c {
            f |= FC;
        }
        self.a = r;
        self.setf(f);
    }

    // ---- step ----

    /// Run one instruction or accept a pending interrupt. Returns T-states.
    pub fn step(&mut self, bus: &mut impl Bus) -> u32 {
        if self.int_line && self.iff1 && !self.ei_pending {
            let t = self.interrupt(bus);
            self.cycles += u64::from(t);
            return t;
        }
        self.ei_pending = false;
        let prev_q = self.q;
        self.q = 0;
        if self.halted {
            self.inc_r();
            self.cycles += 4;
            self.q = 0;
            return 4;
        }
        let t = self.exec(bus, prev_q);
        self.cycles += u64::from(t);
        t
    }

    fn interrupt(&mut self, bus: &mut impl Bus) -> u32 {
        if self.halted {
            self.halted = false;
            self.pc = self.pc.wrapping_add(1);
        }
        self.iff1 = false;
        self.iff2 = false;
        self.inc_r();
        self.q = 0;
        match self.im {
            2 => {
                let v = bus.int_data();
                self.push(bus, self.pc);
                let vec = u16::from_be_bytes([self.i, v]);
                self.pc = Self::read16(bus, vec);
                self.wz = self.pc;
                19
            }
            _ => {
                self.push(bus, self.pc);
                self.pc = 0x38;
                self.wz = 0x38;
                13
            }
        }
    }

    fn exec(&mut self, bus: &mut impl Bus, mut prev_q: u8) -> u32 {
        let mut idx = Idx::Hl;
        let mut t = 0u32;
        let mut op = self.fetch(bus);
        self.inc_r();
        loop {
            match op {
                0xDD => idx = Idx::Ix,
                0xFD => idx = Idx::Iy,
                _ => break,
            }
            t += 4;
            // A prefix byte is an instruction that leaves the flags alone, so Q reads 0 after it.
            prev_q = 0;
            op = self.fetch(bus);
            self.inc_r();
        }
        t + match op {
            0xCB => {
                if idx == Idx::Hl {
                    let op2 = self.fetch(bus);
                    self.inc_r();
                    self.exec_cb(bus, op2)
                } else {
                    let a = self.mem_addr(bus, idx);
                    let op2 = self.fetch(bus);
                    self.exec_ddcb(bus, op2, a)
                }
            }
            0xED => {
                let op2 = self.fetch(bus);
                self.inc_r();
                self.exec_ed(bus, op2)
            }
            _ => self.exec_main(bus, op, idx, prev_q),
        }
    }

    fn exec_main(&mut self, bus: &mut impl Bus, op: u8, idx: Idx, prev_q: u8) -> u32 {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let qb = y & 1;
        let ixd = idx != Idx::Hl;
        match x {
            0 => match z {
                0 => match y {
                    0 => 4,
                    1 => {
                        let af = self.af();
                        self.set_af(self.af_);
                        self.af_ = af;
                        4
                    }
                    2 => {
                        let d = self.fetch(bus) as i8;
                        self.b = self.b.wrapping_sub(1);
                        if self.b != 0 {
                            self.pc = self.pc.wrapping_add(d as u16);
                            self.wz = self.pc;
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        let d = self.fetch(bus) as i8;
                        self.pc = self.pc.wrapping_add(d as u16);
                        self.wz = self.pc;
                        12
                    }
                    _ => {
                        let d = self.fetch(bus) as i8;
                        if self.cond(y - 4) {
                            self.pc = self.pc.wrapping_add(d as u16);
                            self.wz = self.pc;
                            12
                        } else {
                            7
                        }
                    }
                },
                1 => {
                    if qb == 0 {
                        let v = self.fetch16(bus);
                        self.set_rp(p, idx, v);
                        10
                    } else {
                        let a = self.idx_reg(idx);
                        let b = self.get_rp(p, idx);
                        let r = self.add16(a, b);
                        self.set_idx_reg(idx, r);
                        11
                    }
                }
                2 => match (qb, p) {
                    (0, 0) | (0, 1) => {
                        let a = if p == 0 { self.bc() } else { self.de() };
                        bus.write(a, self.a);
                        self.wz = (u16::from(self.a) << 8) | (a.wrapping_add(1) & 0xFF);
                        7
                    }
                    (0, 2) => {
                        let a = self.fetch16(bus);
                        Self::write16(bus, a, self.idx_reg(idx));
                        self.wz = a.wrapping_add(1);
                        16
                    }
                    (0, _) => {
                        let a = self.fetch16(bus);
                        bus.write(a, self.a);
                        self.wz = (u16::from(self.a) << 8) | (a.wrapping_add(1) & 0xFF);
                        13
                    }
                    (_, 0) | (_, 1) => {
                        let a = if p == 0 { self.bc() } else { self.de() };
                        self.a = bus.read(a);
                        self.wz = a.wrapping_add(1);
                        7
                    }
                    (_, 2) => {
                        let a = self.fetch16(bus);
                        let v = Self::read16(bus, a);
                        self.set_idx_reg(idx, v);
                        self.wz = a.wrapping_add(1);
                        16
                    }
                    _ => {
                        let a = self.fetch16(bus);
                        self.a = bus.read(a);
                        self.wz = a.wrapping_add(1);
                        13
                    }
                },
                3 => {
                    let v = self.get_rp(p, idx);
                    let v = if qb == 0 {
                        v.wrapping_add(1)
                    } else {
                        v.wrapping_sub(1)
                    };
                    self.set_rp(p, idx, v);
                    6
                }
                4 | 5 => {
                    let inc = z == 4;
                    if y == 6 {
                        let a = self.mem_addr(bus, idx);
                        let v = bus.read(a);
                        let r = if inc { self.inc8(v) } else { self.dec8(v) };
                        bus.write(a, r);
                        if ixd { 19 } else { 11 }
                    } else {
                        let v = self.get_r(y, idx);
                        let r = if inc { self.inc8(v) } else { self.dec8(v) };
                        self.set_r(y, idx, r);
                        4
                    }
                }
                6 => {
                    if y == 6 {
                        let a = self.mem_addr(bus, idx);
                        let v = self.fetch(bus);
                        bus.write(a, v);
                        if ixd { 15 } else { 10 }
                    } else {
                        let v = self.fetch(bus);
                        self.set_r(y, idx, v);
                        7
                    }
                }
                _ => {
                    match y {
                        0..=3 => {
                            let a = self.a;
                            let c_in = self.f & FC != 0;
                            let (r, out) = match y {
                                0 => (a.rotate_left(1), a & 0x80 != 0),
                                1 => (a.rotate_right(1), a & 1 != 0),
                                2 => ((a << 1) | u8::from(c_in), a & 0x80 != 0),
                                _ => ((a >> 1) | if c_in { 0x80 } else { 0 }, a & 1 != 0),
                            };
                            self.a = r;
                            let f = (self.f & (FS | FZ | FP))
                                | (r & (FX | FY))
                                | if out { FC } else { 0 };
                            self.setf(f);
                        }
                        4 => self.daa(),
                        5 => {
                            self.a = !self.a;
                            let f = (self.f & (FS | FZ | FP | FC)) | FH | FN | (self.a & (FX | FY));
                            self.setf(f);
                        }
                        6 => {
                            let xy = ((prev_q ^ self.f) | self.a) & (FX | FY);
                            let f = (self.f & (FS | FZ | FP)) | FC | xy;
                            self.setf(f);
                        }
                        _ => {
                            let xy = ((prev_q ^ self.f) | self.a) & (FX | FY);
                            let c = self.f & FC != 0;
                            let f = (self.f & (FS | FZ | FP)) | xy | if c { FH } else { FC };
                            self.setf(f);
                        }
                    }
                    4
                }
            },
            1 => {
                if z == 6 && y == 6 {
                    self.halted = true;
                    return 4;
                }
                if y == 6 {
                    let a = self.mem_addr(bus, idx);
                    let v = self.get_r(z, Idx::Hl);
                    bus.write(a, v);
                    if ixd { 15 } else { 7 }
                } else if z == 6 {
                    let a = self.mem_addr(bus, idx);
                    let v = bus.read(a);
                    self.set_r(y, Idx::Hl, v);
                    if ixd { 15 } else { 7 }
                } else {
                    let v = self.get_r(z, idx);
                    self.set_r(y, idx, v);
                    4
                }
            }
            2 => {
                if z == 6 {
                    let a = self.mem_addr(bus, idx);
                    let v = bus.read(a);
                    self.alu(y, v);
                    if ixd { 15 } else { 7 }
                } else {
                    let v = self.get_r(z, idx);
                    self.alu(y, v);
                    4
                }
            }
            _ => match z {
                0 => {
                    if self.cond(y) {
                        self.pc = self.pop(bus);
                        self.wz = self.pc;
                        11
                    } else {
                        5
                    }
                }
                1 => {
                    if qb == 0 {
                        let v = self.pop(bus);
                        self.set_rp2(p, idx, v);
                        10
                    } else {
                        match p {
                            0 => {
                                self.pc = self.pop(bus);
                                self.wz = self.pc;
                                10
                            }
                            1 => {
                                let (bc, de, hl) = (self.bc(), self.de(), self.hl());
                                self.set_bc(self.bc_);
                                self.set_de(self.de_);
                                self.set_hl(self.hl_);
                                self.bc_ = bc;
                                self.de_ = de;
                                self.hl_ = hl;
                                4
                            }
                            2 => {
                                self.pc = self.idx_reg(idx);
                                4
                            }
                            _ => {
                                self.sp = self.idx_reg(idx);
                                6
                            }
                        }
                    }
                }
                2 => {
                    let a = self.fetch16(bus);
                    self.wz = a;
                    if self.cond(y) {
                        self.pc = a;
                    }
                    10
                }
                3 => match y {
                    0 => {
                        let a = self.fetch16(bus);
                        self.pc = a;
                        self.wz = a;
                        10
                    }
                    2 => {
                        let n = self.fetch(bus);
                        let port = (u16::from(self.a) << 8) | u16::from(n);
                        bus.port_out(port, self.a);
                        self.wz = (u16::from(self.a) << 8) | u16::from(n.wrapping_add(1));
                        11
                    }
                    3 => {
                        let n = self.fetch(bus);
                        let port = (u16::from(self.a) << 8) | u16::from(n);
                        self.wz = port.wrapping_add(1);
                        self.a = bus.port_in(port);
                        11
                    }
                    4 => {
                        let v = Self::read16(bus, self.sp);
                        Self::write16(bus, self.sp, self.idx_reg(idx));
                        self.set_idx_reg(idx, v);
                        self.wz = v;
                        19
                    }
                    5 => {
                        let de = self.de();
                        self.set_de(self.hl());
                        self.set_hl(de);
                        4
                    }
                    6 => {
                        self.iff1 = false;
                        self.iff2 = false;
                        4
                    }
                    7 => {
                        self.iff1 = true;
                        self.iff2 = true;
                        self.ei_pending = true;
                        4
                    }
                    _ => unreachable!("CB handled earlier"),
                },
                4 => {
                    let a = self.fetch16(bus);
                    self.wz = a;
                    if self.cond(y) {
                        self.push(bus, self.pc);
                        self.pc = a;
                        17
                    } else {
                        10
                    }
                }
                5 => {
                    if qb == 0 {
                        let v = self.get_rp2(p, idx);
                        self.push(bus, v);
                        11
                    } else {
                        // p == 0: CALL nn (DD/ED/FD prefixes are consumed earlier)
                        let a = self.fetch16(bus);
                        self.wz = a;
                        self.push(bus, self.pc);
                        self.pc = a;
                        17
                    }
                }
                6 => {
                    let v = self.fetch(bus);
                    self.alu(y, v);
                    7
                }
                _ => {
                    self.push(bus, self.pc);
                    self.pc = u16::from(y) * 8;
                    self.wz = self.pc;
                    11
                }
            },
        }
    }

    fn exec_cb(&mut self, bus: &mut impl Bus, op: u8) -> u32 {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        if z == 6 {
            let a = self.hl();
            let v = bus.read(a);
            match x {
                0 => {
                    let r = self.rot(y, v);
                    bus.write(a, r);
                    15
                }
                1 => {
                    let wzh = (self.wz >> 8) as u8;
                    self.bit(y, v, wzh);
                    12
                }
                2 => {
                    bus.write(a, v & !(1 << y));
                    15
                }
                _ => {
                    bus.write(a, v | (1 << y));
                    15
                }
            }
        } else {
            let v = self.get_r(z, Idx::Hl);
            match x {
                0 => {
                    let r = self.rot(y, v);
                    self.set_r(z, Idx::Hl, r);
                }
                1 => self.bit(y, v, v),
                2 => self.set_r(z, Idx::Hl, v & !(1 << y)),
                _ => self.set_r(z, Idx::Hl, v | (1 << y)),
            }
            8
        }
    }

    fn exec_ddcb(&mut self, bus: &mut impl Bus, op: u8, a: u16) -> u32 {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let v = bus.read(a);
        if x == 1 {
            self.bit(y, v, (a >> 8) as u8);
            return 16;
        }
        let r = match x {
            0 => self.rot(y, v),
            2 => v & !(1 << y),
            _ => v | (1 << y),
        };
        bus.write(a, r);
        if z != 6 {
            self.set_r(z, Idx::Hl, r);
        }
        19
    }

    fn exec_ed(&mut self, bus: &mut impl Bus, op: u8) -> u32 {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let qb = y & 1;
        if x == 1 {
            return match z {
                0 => {
                    let port = self.bc();
                    let v = bus.port_in(port);
                    self.wz = port.wrapping_add(1);
                    if y != 6 {
                        self.set_r(y, Idx::Hl, v);
                    }
                    let f = (self.f & FC) | szp(v);
                    self.setf(f);
                    12
                }
                1 => {
                    let port = self.bc();
                    let v = if y == 6 { 0 } else { self.get_r(y, Idx::Hl) };
                    bus.port_out(port, v);
                    self.wz = port.wrapping_add(1);
                    12
                }
                2 => {
                    let v = self.get_rp(p, Idx::Hl);
                    if qb == 0 {
                        self.sbc16(v)
                    } else {
                        self.adc16(v)
                    }
                    15
                }
                3 => {
                    let a = self.fetch16(bus);
                    if qb == 0 {
                        Self::write16(bus, a, self.get_rp(p, Idx::Hl));
                    } else {
                        let v = Self::read16(bus, a);
                        self.set_rp(p, Idx::Hl, v);
                    }
                    self.wz = a.wrapping_add(1);
                    20
                }
                4 => {
                    let v = self.a;
                    self.a = 0;
                    self.sub8(v, false, true);
                    8
                }
                5 => {
                    self.pc = self.pop(bus);
                    self.wz = self.pc;
                    self.iff1 = self.iff2;
                    14
                }
                6 => {
                    self.im = match y & 3 {
                        0 | 1 => 0,
                        2 => 1,
                        _ => 2,
                    };
                    8
                }
                _ => match y {
                    0 => {
                        self.i = self.a;
                        9
                    }
                    1 => {
                        self.r = self.a;
                        9
                    }
                    2 | 3 => {
                        self.a = if y == 2 { self.i } else { self.r };
                        let mut f = (self.f & FC) | (szp(self.a) & !FP);
                        if self.iff2 {
                            f |= FP;
                        }
                        self.setf(f);
                        9
                    }
                    4 | 5 => {
                        let a = self.hl();
                        let m = bus.read(a);
                        let (nm, na) = if y == 4 {
                            ((m >> 4) | (self.a << 4), (self.a & 0xF0) | (m & 0x0F))
                        } else {
                            ((m << 4) | (self.a & 0x0F), (self.a & 0xF0) | (m >> 4))
                        };
                        bus.write(a, nm);
                        self.a = na;
                        let f = (self.f & FC) | szp(na);
                        self.setf(f);
                        self.wz = a.wrapping_add(1);
                        18
                    }
                    _ => 8,
                },
            };
        }
        if x == 2 && y >= 4 && z <= 3 {
            return self.block(bus, y, z);
        }
        8
    }

    fn block(&mut self, bus: &mut impl Bus, y: u8, z: u8) -> u32 {
        let inc = y & 1 == 0;
        let repeat = y >= 6;
        let step = |v: u16| {
            if inc {
                v.wrapping_add(1)
            } else {
                v.wrapping_sub(1)
            }
        };
        let instr_pc = self.pc.wrapping_sub(2);
        match z {
            0 => {
                // LDI/LDD/LDIR/LDDR
                let v = bus.read(self.hl());
                bus.write(self.de(), v);
                self.set_hl(step(self.hl()));
                self.set_de(step(self.de()));
                let bc = self.bc().wrapping_sub(1);
                self.set_bc(bc);
                let n = v.wrapping_add(self.a);
                let mut f =
                    (self.f & (FS | FZ | FC)) | (n & FX) | if n & 0x02 != 0 { FY } else { 0 };
                if bc != 0 {
                    f |= FP;
                }
                if repeat && bc != 0 {
                    self.pc = instr_pc;
                    self.wz = instr_pc.wrapping_add(1);
                    f = (f & !(FX | FY)) | ((instr_pc >> 8) as u8 & (FX | FY));
                    self.setf(f);
                    return 21;
                }
                self.setf(f);
                16
            }
            1 => {
                // CPI/CPD/CPIR/CPDR
                let v = bus.read(self.hl());
                let res = self.a.wrapping_sub(v);
                let h = (self.a & 0xF) < (v & 0xF);
                self.set_hl(step(self.hl()));
                let bc = self.bc().wrapping_sub(1);
                self.set_bc(bc);
                self.wz = step(self.wz);
                let n = res.wrapping_sub(u8::from(h));
                let mut f =
                    (self.f & FC) | FN | (res & FS) | (n & FX) | if n & 0x02 != 0 { FY } else { 0 };
                if res == 0 {
                    f |= FZ;
                }
                if h {
                    f |= FH;
                }
                if bc != 0 {
                    f |= FP;
                }
                if repeat && bc != 0 && res != 0 {
                    self.pc = instr_pc;
                    self.wz = instr_pc.wrapping_add(1);
                    f = (f & !(FX | FY)) | ((instr_pc >> 8) as u8 & (FX | FY));
                    self.setf(f);
                    return 21;
                }
                self.setf(f);
                16
            }
            2 | 3 => {
                // INI/IND/INIR/INDR and OUTI/OUTD/OTIR/OTDR
                let input = z == 2;
                let (v, k) = if input {
                    let port = self.bc();
                    let v = bus.port_in(port);
                    self.wz = step(port);
                    bus.write(self.hl(), v);
                    self.b = self.b.wrapping_sub(1);
                    let c2 = if inc {
                        self.c.wrapping_add(1)
                    } else {
                        self.c.wrapping_sub(1)
                    };
                    (v, u16::from(v) + u16::from(c2))
                } else {
                    let v = bus.read(self.hl());
                    self.b = self.b.wrapping_sub(1);
                    let port = self.bc();
                    bus.port_out(port, v);
                    self.wz = step(port);
                    self.set_hl(step(self.hl()));
                    (v, u16::from(v) + u16::from(self.l))
                };
                if input {
                    self.set_hl(step(self.hl()));
                }
                let b = self.b;
                let mut f = b & (FS | FX | FY);
                if b == 0 {
                    f |= FZ;
                }
                if v & 0x80 != 0 {
                    f |= FN;
                }
                if k > 0xFF {
                    f |= FH | FC;
                }
                if parity(((k & 7) as u8) ^ b) {
                    f |= FP;
                }
                if repeat && b != 0 {
                    self.pc = instr_pc;
                    f = (f & !(FX | FY)) | ((instr_pc >> 8) as u8 & (FX | FY));
                    if f & FC != 0 {
                        if v & 0x80 != 0 {
                            if !parity(b.wrapping_sub(1) & 7) {
                                f ^= FP;
                            }
                            f = (f & !FH) | if b & 0xF == 0 { FH } else { 0 };
                        } else {
                            if !parity(b.wrapping_add(1) & 7) {
                                f ^= FP;
                            }
                            f = (f & !FH) | if b & 0xF == 0xF { FH } else { 0 };
                        }
                    } else if !parity(b & 7) {
                        f ^= FP;
                    }
                    self.setf(f);
                    return 21;
                }
                self.setf(f);
                16
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests;
