//! Motorola 68000 interpreter, written from the published programmer's reference.
//!
//! The CPU talks to the outside world only through [`Bus`]. Memory is big-endian with a
//! 24-bit address bus. Long accesses are two word accesses (high word first).
//!
//! Timing is approximate (a fixed per-instruction estimate) for now.

#![forbid(unsafe_code)]

mod exec;

/// Memory and interrupt interface seen by the CPU.
pub trait Bus {
    fn read8(&mut self, addr: u32) -> u8;
    /// `addr` is always even.
    fn read16(&mut self, addr: u32) -> u16;
    fn write8(&mut self, addr: u32, value: u8);
    /// `addr` is always even.
    fn write16(&mut self, addr: u32, value: u16);
    /// Interrupt acknowledge for `level`. `None` means use the autovector (24 + level).
    fn int_ack(&mut self, _level: u8) -> Option<u8> {
        None
    }
}

/// Status register bits.
pub mod sr {
    pub const C: u16 = 0x0001;
    pub const V: u16 = 0x0002;
    pub const Z: u16 = 0x0004;
    pub const N: u16 = 0x0008;
    pub const X: u16 = 0x0010;
    pub const S: u16 = 0x2000;
    pub const T: u16 = 0x8000;
    pub const MASK: u16 = 0xA71F;
}

/// Operand size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Byte,
    Word,
    Long,
}

impl Size {
    pub fn bytes(self) -> u32 {
        match self {
            Size::Byte => 1,
            Size::Word => 2,
            Size::Long => 4,
        }
    }
    pub fn mask(self) -> u32 {
        match self {
            Size::Byte => 0xFF,
            Size::Word => 0xFFFF,
            Size::Long => 0xFFFF_FFFF,
        }
    }
    pub fn msb(self) -> u32 {
        match self {
            Size::Byte => 0x80,
            Size::Word => 0x8000,
            Size::Long => 0x8000_0000,
        }
    }
    pub fn bits(self) -> u32 {
        self.bytes() * 8
    }
}

/// CPU state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    /// Data registers.
    pub d: [u32; 8],
    /// Address registers; `a[7]` is the active stack pointer.
    pub a: [u32; 8],
    /// Saved user stack pointer (valid while in supervisor mode).
    pub usp: u32,
    /// Saved supervisor stack pointer (valid while in user mode).
    pub ssp: u32,
    pub sr: u16,
    pub pc: u32,
    /// Set by STOP until an interrupt arrives.
    pub stopped: bool,
    /// Set after a double bus/address fault.
    pub halted: bool,
    /// Pending interrupt level (0 = none), sampled before each instruction.
    pub irq_level: u8,
    /// Opcode of the instruction being executed.
    pub ir: u16,
    /// Total cycles run (approximate).
    pub cycles: u64,
    /// Address of the instruction being executed.
    pub(crate) instr_pc: u32,
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu {
            d: [0; 8],
            a: [0; 8],
            usp: 0,
            ssp: 0,
            sr: sr::S | 0x0700,
            pc: 0,
            stopped: false,
            halted: false,
            irq_level: 0,
            ir: 0,
            cycles: 0,
            instr_pc: 0,
        }
    }
}

/// Internal fault raised during an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    /// Word/long access to an odd address.
    Address {
        addr: u32,
        write: bool,
        instruction: bool,
    },
    /// Exception with this vector number; `pc` is the value to stack.
    Trap { vector: u32, pc: u32 },
}

pub(crate) type Res<T> = Result<T, Fault>;

impl Cpu {
    pub fn new() -> Cpu {
        Cpu::default()
    }

    /// Hardware reset: load SSP and PC from vectors 0 and 1.
    pub fn reset(&mut self, bus: &mut impl Bus) {
        self.sr = sr::S | 0x0700;
        self.stopped = false;
        self.halted = false;
        self.a[7] = read32(bus, 0);
        self.ssp = self.a[7];
        self.pc = read32(bus, 4);
    }

    pub fn supervisor(&self) -> bool {
        self.sr & sr::S != 0
    }

    /// Set SR, swapping stack pointers when the S bit changes.
    pub fn set_sr(&mut self, value: u16) {
        let value = value & sr::MASK;
        let was = self.supervisor();
        let now = value & sr::S != 0;
        if was && !now {
            self.ssp = self.a[7];
            self.a[7] = self.usp;
        } else if !was && now {
            self.usp = self.a[7];
            self.a[7] = self.ssp;
        }
        self.sr = value;
    }

    /// The user stack pointer, wherever it currently lives.
    pub fn user_sp(&self) -> u32 {
        if self.supervisor() {
            self.usp
        } else {
            self.a[7]
        }
    }

    /// The supervisor stack pointer, wherever it currently lives.
    pub fn supervisor_sp(&self) -> u32 {
        if self.supervisor() {
            self.a[7]
        } else {
            self.ssp
        }
    }

    /// Run one instruction (or service an interrupt). Returns approximate cycles used.
    pub fn step(&mut self, bus: &mut impl Bus) -> u32 {
        if self.halted {
            return 4;
        }
        let level = self.irq_level & 7;
        let mask = ((self.sr >> 8) & 7) as u8;
        if level > 0 && (level == 7 || level > mask) {
            self.stopped = false;
            let vector = bus.int_ack(level).map_or(24 + u32::from(level), u32::from);
            let old = self.sr;
            self.enter_exception(bus, vector, self.pc, old);
            self.sr = (self.sr & !0x0700) | (u16::from(level) << 8);
            self.cycles += 44;
            return 44;
        }
        if self.stopped {
            self.cycles += 4;
            return 4;
        }
        self.instr_pc = self.pc;
        let before = self.cycles;
        let result = self.fetch16(bus).and_then(|op| {
            self.ir = op;
            self.execute(bus, op)
        });
        // The next opcode is prefetched as part of this instruction, so a jump to an odd
        // address faults here.
        let result = match result {
            Ok(()) if self.pc & 1 != 0 && !self.stopped => Err(Fault::Address {
                addr: self.pc,
                write: false,
                instruction: true,
            }),
            other => other,
        };
        if let Err(f) = result {
            self.fault(bus, f);
        }
        let used = (self.cycles - before) as u32;
        let used = used.max(4);
        self.cycles = before + u64::from(used);
        used
    }

    fn fault(&mut self, bus: &mut impl Bus, f: Fault) {
        match f {
            Fault::Trap { vector, pc } => {
                let old = self.sr;
                self.enter_exception(bus, vector, pc, old);
                self.cycles += 34;
            }
            Fault::Address {
                addr,
                write,
                instruction,
            } => {
                let old = self.sr;
                self.set_sr((self.sr | sr::S) & !sr::T);
                let fc: u16 = match (old & sr::S != 0, instruction) {
                    (false, false) => 1,
                    (false, true) => 2,
                    (true, false) => 5,
                    (true, true) => 6,
                };
                let status = fc | if write { 0 } else { 0x10 } | if instruction { 0x08 } else { 0 };
                let status = status | (self.ir & !0x1F);
                let sp = self.a[7].wrapping_sub(14);
                self.a[7] = sp;
                let pc = self.pc;
                if sp & 1 != 0 {
                    self.halted = true;
                    return;
                }
                bus.write16(sp & 0xFF_FFFF, status);
                write32(bus, sp.wrapping_add(2), addr);
                bus.write16(sp.wrapping_add(6) & 0xFF_FFFF, self.ir);
                bus.write16(sp.wrapping_add(8) & 0xFF_FFFF, old);
                write32(bus, sp.wrapping_add(10), pc);
                self.pc = read32(bus, 3 * 4);
                self.cycles += 50;
            }
        }
    }

    pub(crate) fn enter_exception(
        &mut self,
        bus: &mut impl Bus,
        vector: u32,
        pc: u32,
        old_sr: u16,
    ) {
        self.set_sr((self.sr | sr::S) & !sr::T);
        let sp = self.a[7].wrapping_sub(6);
        self.a[7] = sp;
        if sp & 1 != 0 {
            self.halted = true;
            return;
        }
        bus.write16(sp & 0xFF_FFFF, old_sr);
        write32(bus, sp.wrapping_add(2), pc);
        self.pc = read32(bus, vector * 4);
    }
}

pub(crate) fn read32(bus: &mut impl Bus, addr: u32) -> u32 {
    let hi = u32::from(bus.read16(addr & 0xFF_FFFE));
    let lo = u32::from(bus.read16(addr.wrapping_add(2) & 0xFF_FFFE));
    (hi << 16) | lo
}

pub(crate) fn write32(bus: &mut impl Bus, addr: u32, v: u32) {
    bus.write16(addr & 0xFF_FFFE, (v >> 16) as u16);
    bus.write16(addr.wrapping_add(2) & 0xFF_FFFE, v as u16);
}

#[cfg(test)]
mod tests;
