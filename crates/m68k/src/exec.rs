//! Instruction decode and execution.

use crate::{Bus, Cpu, Fault, Res, Size, sr};

/// A resolved effective address.
#[derive(Debug, Clone, Copy)]
enum Ea {
    D(usize),
    A(usize),
    Mem(u32),
    Imm(u32),
}

fn sext(v: u32, size: Size) -> u32 {
    match size {
        Size::Byte => v as u8 as i8 as i32 as u32,
        Size::Word => v as u16 as i16 as i32 as u32,
        Size::Long => v,
    }
}

fn size_bits(bits: u16) -> Option<Size> {
    match bits & 3 {
        0 => Some(Size::Byte),
        1 => Some(Size::Word),
        2 => Some(Size::Long),
        _ => None,
    }
}

impl Cpu {
    // ---------- memory ----------

    pub(crate) fn fetch16(&mut self, bus: &mut impl Bus) -> Res<u16> {
        let pc = self.pc;
        if pc & 1 != 0 {
            return Err(Fault::Address {
                addr: pc,
                write: false,
                instruction: true,
            });
        }
        self.pc = pc.wrapping_add(2);
        self.cycles += 4;
        Ok(bus.read16(pc & 0xFF_FFFF))
    }

    fn fetch32(&mut self, bus: &mut impl Bus) -> Res<u32> {
        let hi = u32::from(self.fetch16(bus)?);
        let lo = u32::from(self.fetch16(bus)?);
        Ok((hi << 16) | lo)
    }

    fn read(&mut self, bus: &mut impl Bus, addr: u32, size: Size) -> Res<u32> {
        let a = addr & 0xFF_FFFF;
        if size != Size::Byte && addr & 1 != 0 {
            return Err(Fault::Address {
                addr,
                write: false,
                instruction: false,
            });
        }
        self.cycles += 4 * (size.bytes().max(2) / 2) as u64;
        Ok(match size {
            Size::Byte => u32::from(bus.read8(a)),
            Size::Word => u32::from(bus.read16(a)),
            Size::Long => crate::read32(bus, a),
        })
    }

    fn write(&mut self, bus: &mut impl Bus, addr: u32, size: Size, v: u32) -> Res<()> {
        let a = addr & 0xFF_FFFF;
        if size != Size::Byte && addr & 1 != 0 {
            return Err(Fault::Address {
                addr,
                write: true,
                instruction: false,
            });
        }
        self.cycles += 4 * (size.bytes().max(2) / 2) as u64;
        match size {
            Size::Byte => bus.write8(a, v as u8),
            Size::Word => bus.write16(a, v as u16),
            Size::Long => crate::write32(bus, a, v),
        }
        Ok(())
    }

    fn push32(&mut self, bus: &mut impl Bus, v: u32) -> Res<()> {
        self.a[7] = self.a[7].wrapping_sub(4);
        self.write(bus, self.a[7], Size::Long, v)
    }

    fn pop(&mut self, bus: &mut impl Bus, size: Size) -> Res<u32> {
        let v = self.read(bus, self.a[7], size)?;
        self.a[7] = self.a[7].wrapping_add(size.bytes());
        Ok(v)
    }

    // ---------- effective addresses ----------

    fn index(&mut self, bus: &mut impl Bus, base: u32) -> Res<u32> {
        let ext = self.fetch16(bus)?;
        let r = usize::from((ext >> 12) & 7);
        let reg = if ext & 0x8000 != 0 {
            self.a[r]
        } else {
            self.d[r]
        };
        let idx = if ext & 0x0800 != 0 {
            reg
        } else {
            sext(reg, Size::Word)
        };
        self.cycles += 2;
        Ok(base
            .wrapping_add(idx)
            .wrapping_add(sext(u32::from(ext & 0xFF), Size::Byte)))
    }

    fn ea(&mut self, bus: &mut impl Bus, mode: u16, reg: u16, size: Size) -> Res<Ea> {
        let r = usize::from(reg & 7);
        Ok(match mode & 7 {
            0 => Ea::D(r),
            1 => Ea::A(r),
            2 => Ea::Mem(self.a[r]),
            3 => {
                let a = self.a[r];
                let inc = if r == 7 && size == Size::Byte {
                    2
                } else {
                    size.bytes()
                };
                self.a[r] = a.wrapping_add(inc);
                Ea::Mem(a)
            }
            4 => {
                let dec = if r == 7 && size == Size::Byte {
                    2
                } else {
                    size.bytes()
                };
                self.a[r] = self.a[r].wrapping_sub(dec);
                self.cycles += 2;
                Ea::Mem(self.a[r])
            }
            5 => {
                let d = sext(u32::from(self.fetch16(bus)?), Size::Word);
                Ea::Mem(self.a[r].wrapping_add(d))
            }
            6 => {
                let base = self.a[r];
                Ea::Mem(self.index(bus, base)?)
            }
            _ => match r {
                0 => Ea::Mem(sext(u32::from(self.fetch16(bus)?), Size::Word)),
                1 => Ea::Mem(self.fetch32(bus)?),
                2 => {
                    let base = self.pc;
                    let d = sext(u32::from(self.fetch16(bus)?), Size::Word);
                    Ea::Mem(base.wrapping_add(d))
                }
                3 => {
                    let base = self.pc;
                    Ea::Mem(self.index(bus, base)?)
                }
                4 => Ea::Imm(match size {
                    Size::Byte => u32::from(self.fetch16(bus)?) & 0xFF,
                    Size::Word => u32::from(self.fetch16(bus)?),
                    Size::Long => self.fetch32(bus)?,
                }),
                _ => return Err(self.illegal()),
            },
        })
    }

    fn get(&mut self, bus: &mut impl Bus, ea: Ea, size: Size) -> Res<u32> {
        Ok(match ea {
            Ea::D(r) => self.d[r] & size.mask(),
            Ea::A(r) => self.a[r] & size.mask(),
            Ea::Mem(a) => self.read(bus, a, size)?,
            Ea::Imm(v) => v & size.mask(),
        })
    }

    fn set(&mut self, bus: &mut impl Bus, ea: Ea, size: Size, v: u32) -> Res<()> {
        match ea {
            Ea::D(r) => self.d[r] = (self.d[r] & !size.mask()) | (v & size.mask()),
            Ea::A(r) => self.a[r] = v,
            Ea::Mem(a) => self.write(bus, a, size, v)?,
            Ea::Imm(_) => return Err(self.illegal()),
        }
        Ok(())
    }

    /// Resolve the effective address field (low 6 bits) of `op`.
    fn ea_op(&mut self, bus: &mut impl Bus, op: u16, size: Size) -> Res<Ea> {
        self.ea(bus, (op >> 3) & 7, op & 7, size)
    }

    /// Control addressing (no register, postinc, predec, immediate): returns the address.
    fn control_addr(&mut self, bus: &mut impl Bus, op: u16) -> Res<u32> {
        let mode = (op >> 3) & 7;
        let reg = op & 7;
        if matches!(mode, 0 | 1 | 3 | 4) || (mode == 7 && reg > 3) {
            return Err(self.illegal());
        }
        match self.ea(bus, mode, reg, Size::Long)? {
            Ea::Mem(a) => Ok(a),
            _ => Err(self.illegal()),
        }
    }

    // ---------- flags ----------

    fn flag(&mut self, f: u16, on: bool) {
        if on {
            self.sr |= f;
        } else {
            self.sr &= !f;
        }
    }

    fn nz(&mut self, v: u32, size: Size) {
        let v = v & size.mask();
        self.flag(sr::N, v & size.msb() != 0);
        self.flag(sr::Z, v == 0);
    }

    fn logic_flags(&mut self, v: u32, size: Size) {
        self.nz(v, size);
        self.sr &= !(sr::V | sr::C);
    }

    fn add_core(&mut self, d: u32, s: u32, x: u32, size: Size, keep_z: bool, set_x: bool) -> u32 {
        let m = size.mask();
        let (d, s) = (d & m, s & m);
        let res = d.wrapping_add(s).wrapping_add(x) & m;
        let msb = size.msb();
        let c = ((s & d) | (!res & (s | d))) & msb != 0;
        let v = ((s ^ res) & (d ^ res)) & msb != 0;
        self.flag(sr::C, c);
        self.flag(sr::V, v);
        self.flag(sr::N, res & msb != 0);
        if keep_z {
            if res != 0 {
                self.sr &= !sr::Z;
            }
        } else {
            self.flag(sr::Z, res == 0);
        }
        if set_x {
            self.flag(sr::X, c);
        }
        res
    }

    fn sub_core(&mut self, d: u32, s: u32, x: u32, size: Size, keep_z: bool, set_x: bool) -> u32 {
        let m = size.mask();
        let (d, s) = (d & m, s & m);
        let res = d.wrapping_sub(s).wrapping_sub(x) & m;
        let msb = size.msb();
        let c = ((s & !d) | (res & !d) | (s & res)) & msb != 0;
        let v = ((s ^ d) & (res ^ d)) & msb != 0;
        self.flag(sr::C, c);
        self.flag(sr::V, v);
        self.flag(sr::N, res & msb != 0);
        if keep_z {
            if res != 0 {
                self.sr &= !sr::Z;
            }
        } else {
            self.flag(sr::Z, res == 0);
        }
        if set_x {
            self.flag(sr::X, c);
        }
        res
    }

    fn cond(&self, cc: u16) -> bool {
        let s = self.sr;
        let (c, v, z, n) = (
            s & sr::C != 0,
            s & sr::V != 0,
            s & sr::Z != 0,
            s & sr::N != 0,
        );
        match cc & 0xF {
            0 => true,
            1 => false,
            2 => !c && !z,
            3 => c || z,
            4 => !c,
            5 => c,
            6 => !z,
            7 => z,
            8 => !v,
            9 => v,
            10 => !n,
            11 => n,
            12 => n == v,
            13 => n != v,
            14 => !z && n == v,
            _ => z || n != v,
        }
    }

    // ---------- exceptions ----------

    fn illegal(&self) -> Fault {
        Fault::Trap {
            vector: 4,
            pc: self.instr_pc,
        }
    }

    fn privileged(&self) -> Res<()> {
        if self.supervisor() {
            Ok(())
        } else {
            Err(Fault::Trap {
                vector: 8,
                pc: self.instr_pc,
            })
        }
    }

    fn trap_next(&self, vector: u32) -> Fault {
        Fault::Trap {
            vector,
            pc: self.pc,
        }
    }

    // ---------- dispatch ----------

    pub(crate) fn execute(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        match op >> 12 {
            0x0 => self.group0(bus, op),
            0x1 => self.op_move(bus, op, Size::Byte),
            0x2 => self.op_move(bus, op, Size::Long),
            0x3 => self.op_move(bus, op, Size::Word),
            0x4 => self.group4(bus, op),
            0x5 => self.group5(bus, op),
            0x6 => self.op_branch(bus, op),
            0x7 => {
                if op & 0x100 != 0 {
                    return Err(self.illegal());
                }
                let v = sext(u32::from(op & 0xFF), Size::Byte);
                self.d[usize::from((op >> 9) & 7)] = v;
                self.logic_flags(v, Size::Long);
                Ok(())
            }
            0x8 => self.group8(bus, op),
            0x9 => self.add_sub(bus, op, true),
            0xA => Err(Fault::Trap {
                vector: 10,
                pc: self.instr_pc,
            }),
            0xB => self.group_b(bus, op),
            0xC => self.group_c(bus, op),
            0xD => self.add_sub(bus, op, false),
            0xE => self.shifts(bus, op),
            _ => Err(Fault::Trap {
                vector: 11,
                pc: self.instr_pc,
            }),
        }
    }

    // ---------- group 0: immediates, bit ops, MOVEP ----------

    fn group0(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        if op & 0x0138 == 0x0108 {
            return self.movep(bus, op);
        }
        if op & 0x0100 != 0 {
            let bit = self.d[usize::from((op >> 9) & 7)];
            return self.bit_op(bus, op, bit);
        }
        let kind = (op >> 9) & 7;
        if kind == 4 {
            let bit = u32::from(self.fetch16(bus)? & 0xFF);
            return self.bit_op(bus, op, bit);
        }
        if kind == 7 {
            return Err(self.illegal());
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        // ORI/ANDI/EORI to CCR or SR
        if op & 0x3F == 0x3C && matches!(kind, 0 | 1 | 5) {
            let imm = self.fetch16(bus)?;
            match size {
                Size::Byte => {
                    let ccr = self.sr & 0xFF;
                    let v = match kind {
                        0 => ccr | imm,
                        1 => ccr & imm,
                        _ => ccr ^ imm,
                    } & 0x1F;
                    self.sr = (self.sr & 0xFF00) | v;
                }
                Size::Word => {
                    self.privileged()?;
                    let v = match kind {
                        0 => self.sr | imm,
                        1 => self.sr & imm,
                        _ => self.sr ^ imm,
                    };
                    self.set_sr(v);
                }
                Size::Long => return Err(self.illegal()),
            }
            return Ok(());
        }
        let imm = match size {
            Size::Long => self.fetch32(bus)?,
            _ => u32::from(self.fetch16(bus)?) & size.mask(),
        };
        let ea = self.ea_op(bus, op, size)?;
        if matches!(ea, Ea::A(_)) {
            return Err(self.illegal());
        }
        let d = self.get(bus, ea, size)?;
        match kind {
            0 => {
                let r = d | imm;
                self.logic_flags(r, size);
                self.set(bus, ea, size, r)
            }
            1 => {
                let r = d & imm;
                self.logic_flags(r, size);
                self.set(bus, ea, size, r)
            }
            2 => {
                let r = self.sub_core(d, imm, 0, size, false, true);
                self.set(bus, ea, size, r)
            }
            3 => {
                let r = self.add_core(d, imm, 0, size, false, true);
                self.set(bus, ea, size, r)
            }
            5 => {
                let r = d ^ imm;
                self.logic_flags(r, size);
                self.set(bus, ea, size, r)
            }
            _ => {
                self.sub_core(d, imm, 0, size, false, false);
                Ok(())
            }
        }
    }

    fn bit_op(&mut self, bus: &mut impl Bus, op: u16, bit: u32) -> Res<()> {
        let kind = (op >> 6) & 3;
        let mode = (op >> 3) & 7;
        if mode == 0 {
            let r = usize::from(op & 7);
            let mask = 1u32 << (bit & 31);
            self.flag(sr::Z, self.d[r] & mask == 0);
            match kind {
                1 => self.d[r] ^= mask,
                2 => self.d[r] &= !mask,
                3 => self.d[r] |= mask,
                _ => {}
            }
            return Ok(());
        }
        if mode == 1 {
            return Err(self.illegal());
        }
        let ea = self.ea_op(bus, op, Size::Byte)?;
        let v = self.get(bus, ea, Size::Byte)?;
        let mask = 1u32 << (bit & 7);
        self.flag(sr::Z, v & mask == 0);
        let nv = match kind {
            1 => v ^ mask,
            2 => v & !mask,
            3 => v | mask,
            _ => return Ok(()),
        };
        self.set(bus, ea, Size::Byte, nv)
    }

    fn movep(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let dr = usize::from((op >> 9) & 7);
        let ar = usize::from(op & 7);
        let disp = sext(u32::from(self.fetch16(bus)?), Size::Word);
        let addr = self.a[ar].wrapping_add(disp);
        let n = if op & 0x40 != 0 { 4 } else { 2 };
        if op & 0x80 != 0 {
            // register to memory
            for i in 0..n {
                let byte = (self.d[dr] >> (8 * (n - 1 - i))) as u8;
                self.write(bus, addr.wrapping_add(2 * i), Size::Byte, u32::from(byte))?;
            }
        } else {
            let mut v = 0u32;
            for i in 0..n {
                v = (v << 8) | self.read(bus, addr.wrapping_add(2 * i), Size::Byte)?;
            }
            if n == 2 {
                self.d[dr] = (self.d[dr] & 0xFFFF_0000) | v;
            } else {
                self.d[dr] = v;
            }
        }
        Ok(())
    }

    // ---------- MOVE / MOVEA ----------

    fn op_move(&mut self, bus: &mut impl Bus, op: u16, size: Size) -> Res<()> {
        let src = self.ea_op(bus, op, size)?;
        if size == Size::Byte && matches!(src, Ea::A(_)) {
            return Err(self.illegal());
        }
        let v = self.get(bus, src, size)?;
        let dmode = (op >> 6) & 7;
        let dreg = (op >> 9) & 7;
        if dmode == 1 {
            if size == Size::Byte {
                return Err(self.illegal());
            }
            self.a[usize::from(dreg)] = sext(v, size);
            return Ok(());
        }
        if dmode == 7 && dreg > 1 {
            return Err(self.illegal());
        }
        let dst = self.ea(bus, dmode, dreg, size)?;
        self.logic_flags(v, size);
        self.set(bus, dst, size, v)
    }

    // ---------- group 4: miscellaneous ----------

    fn group4(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        match op {
            0x4AFC => return Err(self.illegal()),
            0x4E70 => {
                self.privileged()?;
                self.cycles += 128;
                return Ok(());
            }
            0x4E71 => return Ok(()),
            0x4E72 => {
                self.privileged()?;
                let v = self.fetch16(bus)?;
                self.set_sr(v);
                self.stopped = true;
                return Ok(());
            }
            0x4E73 => {
                self.privileged()?;
                let new_sr = self.pop(bus, Size::Word)? as u16;
                let pc = self.pop(bus, Size::Long)?;
                self.set_sr(new_sr);
                self.pc = pc;
                return Ok(());
            }
            0x4E75 => {
                self.pc = self.pop(bus, Size::Long)?;
                return Ok(());
            }
            0x4E76 => {
                if self.sr & sr::V != 0 {
                    return Err(self.trap_next(7));
                }
                return Ok(());
            }
            0x4E77 => {
                let ccr = self.pop(bus, Size::Word)? as u16;
                self.sr = (self.sr & 0xFF00) | (ccr & 0x1F);
                self.pc = self.pop(bus, Size::Long)?;
                return Ok(());
            }
            _ => {}
        }
        if op & 0xF1C0 == 0x41C0 {
            let a = self.control_addr(bus, op)?;
            self.a[usize::from((op >> 9) & 7)] = a;
            return Ok(());
        }
        if op & 0xF1C0 == 0x4180 {
            return self.chk(bus, op);
        }
        if op & 0xF1C0 == 0x4100 {
            return Err(self.illegal());
        }
        match op & 0xFFC0 {
            0x40C0 => {
                let ea = self.ea_op(bus, op, Size::Word)?;
                if matches!(ea, Ea::A(_) | Ea::Imm(_)) {
                    return Err(self.illegal());
                }
                if let Ea::Mem(a) = ea {
                    // The 68000 reads before writing.
                    self.read(bus, a, Size::Word)?;
                }
                let v = u32::from(self.sr);
                return self.set(bus, ea, Size::Word, v);
            }
            0x44C0 => {
                let ea = self.ea_op(bus, op, Size::Word)?;
                if matches!(ea, Ea::A(_)) {
                    return Err(self.illegal());
                }
                let v = self.get(bus, ea, Size::Word)? as u16;
                self.sr = (self.sr & 0xFF00) | (v & 0x1F);
                return Ok(());
            }
            0x46C0 => {
                self.privileged()?;
                let ea = self.ea_op(bus, op, Size::Word)?;
                if matches!(ea, Ea::A(_)) {
                    return Err(self.illegal());
                }
                let v = self.get(bus, ea, Size::Word)? as u16;
                self.set_sr(v);
                return Ok(());
            }
            0x4800 => return self.nbcd(bus, op),
            0x4840 => {
                if op & 0x38 == 0 {
                    let r = usize::from(op & 7);
                    let v = self.d[r].rotate_left(16);
                    self.d[r] = v;
                    self.logic_flags(v, Size::Long);
                    return Ok(());
                }
                let a = self.control_addr(bus, op)?;
                return self.push32(bus, a);
            }
            0x4880 | 0x48C0 => {
                if op & 0x38 == 0 {
                    let r = usize::from(op & 7);
                    if op & 0x40 == 0 {
                        let v = sext(self.d[r], Size::Byte) & 0xFFFF;
                        self.d[r] = (self.d[r] & 0xFFFF_0000) | v;
                        self.logic_flags(v, Size::Word);
                    } else {
                        let v = sext(self.d[r], Size::Word);
                        self.d[r] = v;
                        self.logic_flags(v, Size::Long);
                    }
                    return Ok(());
                }
                return self.movem(bus, op, false);
            }
            0x4CC0 | 0x4C80 => return self.movem(bus, op, true),
            0x4AC0 => {
                if op & 0x3F == 0x3C {
                    return Err(self.illegal());
                }
                let ea = self.ea_op(bus, op, Size::Byte)?;
                if matches!(ea, Ea::A(_)) {
                    return Err(self.illegal());
                }
                let v = self.get(bus, ea, Size::Byte)?;
                self.logic_flags(v, Size::Byte);
                return self.set(bus, ea, Size::Byte, v | 0x80);
            }
            0x4E80 => {
                let a = self.control_addr(bus, op)?;
                let ret = self.pc;
                self.push32(bus, ret)?;
                self.pc = a;
                return Ok(());
            }
            0x4EC0 => {
                self.pc = self.control_addr(bus, op)?;
                return Ok(());
            }
            _ => {}
        }
        match op & 0xFFF0 {
            0x4E40 => return Err(self.trap_next(32 + u32::from(op & 0xF))),
            0x4E50 => {
                let r = usize::from(op & 7);
                if op & 8 == 0 {
                    let disp = sext(u32::from(self.fetch16(bus)?), Size::Word);
                    let v = self.a[r];
                    self.a[7] = self.a[7].wrapping_sub(4);
                    let sp = self.a[7];
                    self.write(bus, sp, Size::Long, if r == 7 { sp } else { v })?;
                    self.a[r] = self.a[7];
                    self.a[7] = self.a[7].wrapping_add(disp);
                } else {
                    self.a[7] = self.a[r];
                    let v = self.pop(bus, Size::Long)?;
                    self.a[r] = v;
                }
                return Ok(());
            }
            0x4E60 => {
                self.privileged()?;
                let r = usize::from(op & 7);
                if op & 8 == 0 {
                    self.usp = self.a[r];
                } else {
                    self.a[r] = self.usp;
                }
                return Ok(());
            }
            _ => {}
        }
        // NEGX / CLR / NEG / NOT / TST
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        let kind = (op >> 8) & 0xF;
        let ea = self.ea_op(bus, op, size)?;
        if matches!(ea, Ea::A(_)) && !(kind == 0xA && size != Size::Byte) {
            return Err(self.illegal());
        }
        match kind {
            0x0 => {
                let d = self.get(bus, ea, size)?;
                let x = u32::from(self.sr & sr::X != 0);
                let r = self.sub_core(0, d, x, size, true, true);
                self.set(bus, ea, size, r)
            }
            0x2 => {
                if let Ea::Mem(a) = ea {
                    self.read(bus, a, size)?;
                }
                self.sr = (self.sr & !(sr::N | sr::V | sr::C)) | sr::Z;
                self.set(bus, ea, size, 0)
            }
            0x4 => {
                let d = self.get(bus, ea, size)?;
                let r = self.sub_core(0, d, 0, size, false, true);
                self.set(bus, ea, size, r)
            }
            0x6 => {
                let d = self.get(bus, ea, size)?;
                let r = !d & size.mask();
                self.logic_flags(r, size);
                self.set(bus, ea, size, r)
            }
            0xA => {
                let d = self.get(bus, ea, size)?;
                self.logic_flags(d, size);
                Ok(())
            }
            _ => Err(self.illegal()),
        }
    }

    fn chk(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let ea = self.ea_op(bus, op, Size::Word)?;
        if matches!(ea, Ea::A(_)) {
            return Err(self.illegal());
        }
        let bound = self.get(bus, ea, Size::Word)? as u16 as i16;
        let dn = self.d[usize::from((op >> 9) & 7)] as u16 as i16;
        self.flag(sr::Z, dn == 0);
        self.sr &= !(sr::V | sr::C);
        if dn < 0 {
            self.sr |= sr::N;
            return Err(self.trap_next(6));
        }
        if dn > bound {
            self.sr &= !sr::N;
            return Err(self.trap_next(6));
        }
        Ok(())
    }

    fn movem(&mut self, bus: &mut impl Bus, op: u16, to_regs: bool) -> Res<()> {
        let mask = self.fetch16(bus)?;
        let size = if op & 0x40 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mode = (op >> 3) & 7;
        let reg = usize::from(op & 7);
        if to_regs {
            if mode == 0 || mode == 1 || mode == 4 || (mode == 7 && (op & 7) > 3) {
                return Err(self.illegal());
            }
            let mut addr = if mode == 3 {
                self.a[reg]
            } else {
                match self.ea(bus, mode, op & 7, size)? {
                    Ea::Mem(a) => a,
                    _ => return Err(self.illegal()),
                }
            };
            for i in 0..16 {
                if mask & (1 << i) != 0 {
                    let v = sext(self.read(bus, addr, size)?, size);
                    if i < 8 {
                        self.d[i] = v;
                    } else {
                        self.a[i - 8] = v;
                    }
                    addr = addr.wrapping_add(size.bytes());
                }
            }
            if mode == 3 {
                self.a[reg] = addr;
            }
            // The 68000 performs one extra (discarded) word read.
            self.read(bus, addr, Size::Word)?;
        } else if mode == 4 {
            let mut addr = self.a[reg];
            for i in 0..16 {
                if mask & (1 << i) != 0 {
                    addr = addr.wrapping_sub(size.bytes());
                    let r = 15 - i;
                    let v = if r < 8 { self.d[r] } else { self.a[r - 8] };
                    self.write(bus, addr, size, v)?;
                }
            }
            self.a[reg] = addr;
        } else {
            if mode == 0 || mode == 1 || mode == 3 || (mode == 7 && (op & 7) > 1) {
                return Err(self.illegal());
            }
            let mut addr = match self.ea(bus, mode, op & 7, size)? {
                Ea::Mem(a) => a,
                _ => return Err(self.illegal()),
            };
            for i in 0..16 {
                if mask & (1 << i) != 0 {
                    let v = if i < 8 { self.d[i] } else { self.a[i - 8] };
                    self.write(bus, addr, size, v)?;
                    addr = addr.wrapping_add(size.bytes());
                }
            }
        }
        Ok(())
    }

    // ---------- BCD ----------

    fn bcd_add(&mut self, src: u32, dst: u32) -> u32 {
        let x = u32::from(self.sr & sr::X != 0);
        let ss = src.wrapping_add(dst).wrapping_add(x);
        let bc = ((src & dst) | (!ss & (src | dst))) & 0x88;
        let dc = ((ss.wrapping_add(0x66) ^ ss) & 0x110) >> 1;
        let corf = (bc | dc) - ((bc | dc) >> 2);
        let res = ss.wrapping_add(corf);
        let c = (bc | (ss & !res)) & 0x80 != 0;
        self.flag(sr::X, c);
        self.flag(sr::C, c);
        self.flag(sr::V, (!ss & res) & 0x80 != 0);
        self.flag(sr::N, res & 0x80 != 0);
        if res & 0xFF != 0 {
            self.sr &= !sr::Z;
        }
        res & 0xFF
    }

    fn bcd_sub(&mut self, src: u32, dst: u32) -> u32 {
        let x = u32::from(self.sr & sr::X != 0);
        let dd = dst.wrapping_sub(src).wrapping_sub(x);
        let bc = ((!dst & src) | (dd & !(dst ^ src))) & 0x88;
        let corf = bc - (bc >> 2);
        let res = dd.wrapping_sub(corf);
        let c = (bc | (!dd & res)) & 0x80 != 0;
        self.flag(sr::X, c);
        self.flag(sr::C, c);
        self.flag(sr::V, (dd & !res) & 0x80 != 0);
        self.flag(sr::N, res & 0x80 != 0);
        if res & 0xFF != 0 {
            self.sr &= !sr::Z;
        }
        res & 0xFF
    }

    fn nbcd(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let ea = self.ea_op(bus, op, Size::Byte)?;
        if matches!(ea, Ea::A(_) | Ea::Imm(_)) {
            return Err(self.illegal());
        }
        let d = self.get(bus, ea, Size::Byte)?;
        let r = self.bcd_sub(d, 0);
        self.set(bus, ea, Size::Byte, r)
    }

    /// ABCD/SBCD/ADDX/SUBX register or -(An),-(An) forms.
    fn extended(
        &mut self,
        bus: &mut impl Bus,
        op: u16,
        size: Size,
        f: fn(&mut Cpu, u32, u32) -> u32,
    ) -> Res<()> {
        let rx = usize::from((op >> 9) & 7);
        let ry = usize::from(op & 7);
        if op & 8 == 0 {
            let r = f(self, self.d[ry] & size.mask(), self.d[rx] & size.mask());
            self.d[rx] = (self.d[rx] & !size.mask()) | (r & size.mask());
        } else {
            let sy = self.ea(bus, 4, ry as u16, size)?;
            let s = self.get(bus, sy, size)?;
            let dx = self.ea(bus, 4, rx as u16, size)?;
            let d = self.get(bus, dx, size)?;
            let r = f(self, s, d);
            self.set(bus, dx, size, r)?;
        }
        Ok(())
    }

    // ---------- group 5: ADDQ/SUBQ/Scc/DBcc ----------

    fn group5(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let cc = (op >> 8) & 0xF;
        if (op >> 6) & 3 == 3 {
            if (op >> 3) & 7 == 1 {
                let disp = sext(u32::from(self.fetch16(bus)?), Size::Word);
                if !self.cond(cc) {
                    let r = usize::from(op & 7);
                    let cnt = (self.d[r] as u16).wrapping_sub(1);
                    self.d[r] = (self.d[r] & 0xFFFF_0000) | u32::from(cnt);
                    if cnt != 0xFFFF {
                        self.pc = self.instr_pc.wrapping_add(2).wrapping_add(disp);
                    }
                }
                return Ok(());
            }
            let ea = self.ea_op(bus, op, Size::Byte)?;
            if matches!(ea, Ea::A(_) | Ea::Imm(_)) {
                return Err(self.illegal());
            }
            if let Ea::Mem(a) = ea {
                self.read(bus, a, Size::Byte)?;
            }
            let v = if self.cond(cc) { 0xFF } else { 0 };
            return self.set(bus, ea, Size::Byte, v);
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        let data = match (op >> 9) & 7 {
            0 => 8,
            n => u32::from(n),
        };
        let ea = self.ea_op(bus, op, size)?;
        if let Ea::A(r) = ea {
            if size == Size::Byte {
                return Err(self.illegal());
            }
            self.a[r] = if op & 0x100 != 0 {
                self.a[r].wrapping_sub(data)
            } else {
                self.a[r].wrapping_add(data)
            };
            return Ok(());
        }
        if matches!(ea, Ea::Imm(_)) {
            return Err(self.illegal());
        }
        let d = self.get(bus, ea, size)?;
        let r = if op & 0x100 != 0 {
            self.sub_core(d, data, 0, size, false, true)
        } else {
            self.add_core(d, data, 0, size, false, true)
        };
        self.set(bus, ea, size, r)
    }

    // ---------- group 6: branches ----------

    fn op_branch(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let cc = (op >> 8) & 0xF;
        let base = self.pc;
        let disp = match op & 0xFF {
            0 => sext(u32::from(self.fetch16(bus)?), Size::Word),
            d => sext(u32::from(d), Size::Byte),
        };
        let target = base.wrapping_add(disp);
        if cc == 1 {
            let ret = self.pc;
            self.push32(bus, ret)?;
            self.pc = target;
        } else if self.cond(cc) {
            self.pc = target;
        }
        Ok(())
    }

    // ---------- group 8: OR / DIV / SBCD ----------

    fn group8(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let rn = usize::from((op >> 9) & 7);
        match op & 0x1C0 {
            0x0C0 => return self.div(bus, op, false),
            0x1C0 => return self.div(bus, op, true),
            _ => {}
        }
        if op & 0x1F0 == 0x100 {
            return self.extended(bus, op, Size::Byte, |c, s, d| c.bcd_sub(s, d));
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        let ea = self.ea_op(bus, op, size)?;
        if matches!(ea, Ea::A(_)) {
            return Err(self.illegal());
        }
        let s = self.get(bus, ea, size)?;
        let r = s | (self.d[rn] & size.mask());
        self.logic_flags(r, size);
        if op & 0x100 != 0 {
            self.set(bus, ea, size, r)
        } else {
            self.d[rn] = (self.d[rn] & !size.mask()) | r;
            Ok(())
        }
    }

    fn div(&mut self, bus: &mut impl Bus, op: u16, signed: bool) -> Res<()> {
        let ea = self.ea_op(bus, op, Size::Word)?;
        if matches!(ea, Ea::A(_)) {
            return Err(self.illegal());
        }
        let src = self.get(bus, ea, Size::Word)?;
        let rn = usize::from((op >> 9) & 7);
        let dividend = self.d[rn];
        self.cycles += if signed { 158 } else { 140 };
        if src == 0 {
            self.sr &= !(sr::C | sr::V | sr::Z | sr::N);
            if signed {
                self.flag(sr::Z, true);
            } else {
                self.flag(sr::N, dividend & 0x8000_0000 != 0);
                self.flag(sr::Z, dividend == 0);
            }
            return Err(self.trap_next(5));
        }
        self.sr &= !sr::C;
        if signed {
            let n = dividend as i32 as i64;
            let d = src as u16 as i16 as i64;
            let q = n / d;
            let r = n % d;
            if q > i64::from(i16::MAX) || q < i64::from(i16::MIN) {
                // Overflow: V set, C clear, N and Z left as they were.
                self.sr |= sr::V;
                return Ok(());
            }
            let qv = (q as i16) as u16 as u32;
            self.d[rn] = ((r as i16 as u16 as u32) << 16) | qv;
            self.nz(qv, Size::Word);
            self.sr &= !sr::V;
        } else {
            let q = dividend / src;
            let r = dividend % src;
            if q > 0xFFFF {
                // Overflow: V set, C clear, N and Z left as they were.
                self.sr |= sr::V;
                return Ok(());
            }
            self.d[rn] = (r << 16) | q;
            self.nz(q, Size::Word);
            self.sr &= !sr::V;
        }
        Ok(())
    }

    // ---------- group 9/D: SUB/ADD family ----------

    fn add_sub(&mut self, bus: &mut impl Bus, op: u16, sub: bool) -> Res<()> {
        let rn = usize::from((op >> 9) & 7);
        if op & 0xC0 == 0xC0 {
            // ADDA / SUBA
            let size = if op & 0x100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            let ea = self.ea_op(bus, op, size)?;
            let s = sext(self.get(bus, ea, size)?, size);
            self.a[rn] = if sub {
                self.a[rn].wrapping_sub(s)
            } else {
                self.a[rn].wrapping_add(s)
            };
            return Ok(());
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        if op & 0x130 == 0x100 {
            // ADDX / SUBX
            let f: fn(&mut Cpu, u32, u32) -> u32 = match (sub, size) {
                (false, Size::Byte) => |c, s, d| c.addx(s, d, Size::Byte),
                (false, Size::Word) => |c, s, d| c.addx(s, d, Size::Word),
                (false, Size::Long) => |c, s, d| c.addx(s, d, Size::Long),
                (true, Size::Byte) => |c, s, d| c.subx(s, d, Size::Byte),
                (true, Size::Word) => |c, s, d| c.subx(s, d, Size::Word),
                (true, Size::Long) => |c, s, d| c.subx(s, d, Size::Long),
            };
            return self.extended(bus, op, size, f);
        }
        let ea = self.ea_op(bus, op, size)?;
        if size == Size::Byte && matches!(ea, Ea::A(_)) {
            return Err(self.illegal());
        }
        if op & 0x100 != 0 {
            // Dn op <ea> -> <ea>
            if matches!(ea, Ea::D(_) | Ea::A(_) | Ea::Imm(_)) {
                return Err(self.illegal());
            }
            let d = self.get(bus, ea, size)?;
            let s = self.d[rn];
            let r = if sub {
                self.sub_core(d, s, 0, size, false, true)
            } else {
                self.add_core(d, s, 0, size, false, true)
            };
            self.set(bus, ea, size, r)
        } else {
            let s = self.get(bus, ea, size)?;
            let d = self.d[rn];
            let r = if sub {
                self.sub_core(d, s, 0, size, false, true)
            } else {
                self.add_core(d, s, 0, size, false, true)
            };
            self.d[rn] = (self.d[rn] & !size.mask()) | r;
            Ok(())
        }
    }

    fn addx(&mut self, s: u32, d: u32, size: Size) -> u32 {
        let x = u32::from(self.sr & sr::X != 0);
        self.add_core(d, s, x, size, true, true)
    }

    fn subx(&mut self, s: u32, d: u32, size: Size) -> u32 {
        let x = u32::from(self.sr & sr::X != 0);
        self.sub_core(d, s, x, size, true, true)
    }

    // ---------- group B: CMP/CMPA/CMPM/EOR ----------

    fn group_b(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let rn = usize::from((op >> 9) & 7);
        if op & 0xC0 == 0xC0 {
            let size = if op & 0x100 != 0 {
                Size::Long
            } else {
                Size::Word
            };
            let ea = self.ea_op(bus, op, size)?;
            let s = sext(self.get(bus, ea, size)?, size);
            let d = self.a[rn];
            self.sub_core(d, s, 0, Size::Long, false, false);
            return Ok(());
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        if op & 0x100 == 0 {
            let ea = self.ea_op(bus, op, size)?;
            if size == Size::Byte && matches!(ea, Ea::A(_)) {
                return Err(self.illegal());
            }
            let s = self.get(bus, ea, size)?;
            let d = self.d[rn];
            self.sub_core(d, s, 0, size, false, false);
            return Ok(());
        }
        if op & 0x38 == 0x08 {
            // CMPM (Ay)+,(Ax)+
            let sy = self.ea(bus, 3, op & 7, size)?;
            let s = self.get(bus, sy, size)?;
            let dx = self.ea(bus, 3, rn as u16, size)?;
            let d = self.get(bus, dx, size)?;
            self.sub_core(d, s, 0, size, false, false);
            return Ok(());
        }
        let ea = self.ea_op(bus, op, size)?;
        if matches!(ea, Ea::A(_) | Ea::Imm(_)) {
            return Err(self.illegal());
        }
        let d = self.get(bus, ea, size)?;
        let r = d ^ (self.d[rn] & size.mask());
        self.logic_flags(r, size);
        self.set(bus, ea, size, r)
    }

    // ---------- group C: AND/MUL/ABCD/EXG ----------

    fn group_c(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        let rn = usize::from((op >> 9) & 7);
        match op & 0x1C0 {
            0x0C0 | 0x1C0 => {
                let ea = self.ea_op(bus, op, Size::Word)?;
                if matches!(ea, Ea::A(_)) {
                    return Err(self.illegal());
                }
                let s = self.get(bus, ea, Size::Word)?;
                let r = if op & 0x100 != 0 {
                    ((s as u16 as i16 as i32) * (self.d[rn] as u16 as i16 as i32)) as u32
                } else {
                    s * (self.d[rn] & 0xFFFF)
                };
                self.d[rn] = r;
                self.logic_flags(r, Size::Long);
                self.cycles += 66;
                return Ok(());
            }
            _ => {}
        }
        match op & 0x1F8 {
            0x100 | 0x108 => return self.extended(bus, op, Size::Byte, |c, s, d| c.bcd_add(s, d)),
            0x140 => {
                let ry = usize::from(op & 7);
                self.d.swap(rn, ry);
                return Ok(());
            }
            0x148 => {
                let ry = usize::from(op & 7);
                self.a.swap(rn, ry);
                return Ok(());
            }
            0x188 => {
                let ry = usize::from(op & 7);
                std::mem::swap(&mut self.d[rn], &mut self.a[ry]);
                return Ok(());
            }
            _ => {}
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        let ea = self.ea_op(bus, op, size)?;
        if matches!(ea, Ea::A(_)) {
            return Err(self.illegal());
        }
        let s = self.get(bus, ea, size)?;
        let r = s & self.d[rn] & size.mask();
        self.logic_flags(r, size);
        if op & 0x100 != 0 {
            self.set(bus, ea, size, r)
        } else {
            self.d[rn] = (self.d[rn] & !size.mask()) | r;
            Ok(())
        }
    }

    // ---------- group E: shifts and rotates ----------

    fn shifts(&mut self, bus: &mut impl Bus, op: u16) -> Res<()> {
        if op & 0xC0 == 0xC0 {
            if op & 0x0800 != 0 {
                return Err(self.illegal());
            }
            let ea = self.ea_op(bus, op, Size::Word)?;
            if !matches!(ea, Ea::Mem(_)) {
                return Err(self.illegal());
            }
            let v = self.get(bus, ea, Size::Word)?;
            let r = self.shift(v, 1, (op >> 9) & 3, op & 0x100 != 0, Size::Word);
            return self.set(bus, ea, Size::Word, r);
        }
        let size = size_bits(op >> 6).ok_or_else(|| self.illegal())?;
        let r = usize::from(op & 7);
        let cnt_field = (op >> 9) & 7;
        let count = if op & 0x20 != 0 {
            self.d[usize::from(cnt_field)] % 64
        } else if cnt_field == 0 {
            8
        } else {
            u32::from(cnt_field)
        };
        self.cycles += 2 * u64::from(count);
        let v = self.d[r] & size.mask();
        let res = self.shift(v, count, (op >> 3) & 3, op & 0x100 != 0, size);
        self.d[r] = (self.d[r] & !size.mask()) | res;
        Ok(())
    }

    /// kind: 0 AS, 1 LS, 2 ROX, 3 RO. `left` selects direction.
    fn shift(&mut self, v: u32, count: u32, kind: u16, left: bool, size: Size) -> u32 {
        let msb = size.msb();
        let mask = size.mask();
        let mut v = v & mask;
        let mut x = self.sr & sr::X != 0;
        let mut c = false;
        let mut overflow = false;
        for _ in 0..count {
            match (kind, left) {
                (0, true) | (1, true) => {
                    c = v & msb != 0;
                    let nv = (v << 1) & mask;
                    if kind == 0 && (nv & msb != 0) != (v & msb != 0) {
                        overflow = true;
                    }
                    v = nv;
                    x = c;
                }
                (0, false) => {
                    c = v & 1 != 0;
                    v = (v >> 1) | (v & msb);
                    x = c;
                }
                (1, false) => {
                    c = v & 1 != 0;
                    v >>= 1;
                    x = c;
                }
                (2, true) => {
                    c = v & msb != 0;
                    v = ((v << 1) & mask) | u32::from(x);
                    x = c;
                }
                (2, false) => {
                    c = v & 1 != 0;
                    v = (v >> 1) | if x { msb } else { 0 };
                    x = c;
                }
                (_, true) => {
                    c = v & msb != 0;
                    v = ((v << 1) & mask) | u32::from(c);
                }
                (_, false) => {
                    c = v & 1 != 0;
                    v = (v >> 1) | if c { msb } else { 0 };
                }
            }
        }
        if count == 0 {
            c = if kind == 2 { x } else { false };
        }
        if kind == 0 && !left && count > size.bits() {
            // Matches the reference vectors: shifting past the operand width leaves C and X clear.
            c = false;
            x = false;
        }
        self.nz(v, size);
        self.flag(sr::V, overflow);
        self.flag(sr::C, c);
        if count > 0 && kind != 3 {
            self.flag(sr::X, x);
        }
        v
    }
}
