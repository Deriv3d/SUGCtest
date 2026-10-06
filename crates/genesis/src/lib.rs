//! Mega Drive / Genesis machine: 68000 + Z80 + VDP + sound, wired with the standard memory map.
//! Game-agnostic: the cartridge ROM is supplied by the caller.

#![forbid(unsafe_code)]

pub mod psg;
pub mod vdp;
pub mod ym2612;

use m68k::Cpu;
use z80::Z80;

/// Master clock (NTSC) and derived per-line budgets.
pub const LINES_PER_FRAME: u32 = 262;
const M68K_CYCLES_PER_LINE: u32 = 488;
const Z80_CYCLES_PER_LINE: u32 = 228;
/// Output sample rate for mixed audio.
pub const SAMPLE_RATE: u32 = 48_000;

/// Controller buttons (bit set = pressed).
pub mod button {
    pub const UP: u16 = 1 << 0;
    pub const DOWN: u16 = 1 << 1;
    pub const LEFT: u16 = 1 << 2;
    pub const RIGHT: u16 = 1 << 3;
    pub const B: u16 = 1 << 4;
    pub const C: u16 = 1 << 5;
    pub const A: u16 = 1 << 6;
    pub const START: u16 = 1 << 7;
}

/// Everything except the two CPUs, so each CPU can borrow it as its bus.
pub struct Hardware {
    pub rom: Vec<u8>,
    pub ram: Vec<u8>,
    pub z80_ram: Vec<u8>,
    pub vdp: vdp::Vdp,
    pub ym: ym2612::Ym2612,
    pub psg: psg::Psg,
    pub z80_busreq: bool,
    pub z80_reset: bool,
    z80_bank: u32,
    pub pad: [u16; 2],
    pad_th: [bool; 2],
    pad_ctrl: [u8; 2],
    /// 68000 cycle position within the current line (for the HV counter).
    line_cycle: u32,
    /// Optional backup RAM window (0x200000-0x203FFF style), kept simple.
    pub sram: Vec<u8>,
    /// Write hook: called for every 68000 access to watched PC-independent addresses (unused).
    pub z80_int: bool,
}

impl Hardware {
    fn new(rom: Vec<u8>) -> Hardware {
        Hardware {
            rom,
            ram: vec![0; 0x10000],
            z80_ram: vec![0; 0x2000],
            vdp: vdp::Vdp::new(),
            ym: ym2612::Ym2612::new(),
            psg: psg::Psg::new(),
            z80_busreq: false,
            z80_reset: true,
            z80_bank: 0,
            pad: [0; 2],
            pad_th: [true; 2],
            pad_ctrl: [0; 2],
            line_cycle: 0,
            sram: Vec::new(),
            z80_int: false,
        }
    }

    fn rom_byte(&self, a: u32) -> u8 {
        let a = a as usize;
        if self.rom.is_empty() {
            0xFF
        } else {
            self.rom[a % self.rom.len().max(1)]
        }
    }

    fn pad_read(&self, port: usize) -> u8 {
        let p = !self.pad[port];
        let th = self.pad_th[port];
        let v = if th {
            // TH=1: C B Right Left Down Up
            (p & 0x3F) as u8 | 0x40
        } else {
            // TH=0: Start A 0 0 Down Up
            ((p & 0x03) as u8) | (((p >> 6) & 0x03) as u8) << 4
        };
        (v & 0x7F) | 0x80
    }

    fn io_read(&self, a: u32) -> u8 {
        match a & 0x1F {
            0x00 | 0x01 => 0xA0, // overseas, NTSC, no expansion unit
            0x02 | 0x03 => self.pad_read(0),
            0x04 | 0x05 => self.pad_read(1),
            0x08 | 0x09 => self.pad_ctrl[0],
            0x0A | 0x0B => self.pad_ctrl[1],
            _ => 0,
        }
    }

    fn io_write(&mut self, a: u32, v: u8) {
        match a & 0x1F {
            0x02 | 0x03 => self.pad_th[0] = v & 0x40 != 0,
            0x04 | 0x05 => self.pad_th[1] = v & 0x40 != 0,
            0x08 | 0x09 => self.pad_ctrl[0] = v,
            0x0A | 0x0B => self.pad_ctrl[1] = v,
            _ => {}
        }
    }

    fn z80_space_read(&mut self, a: u16) -> u8 {
        match a {
            0x0000..=0x3FFF => self.z80_ram[usize::from(a & 0x1FFF)],
            0x4000..=0x5FFF => self.ym.read_status(),
            0x8000..=0xFFFF => {
                let addr = (self.z80_bank << 15) | u32::from(a & 0x7FFF);
                self.read8_68k(addr)
            }
            _ => 0xFF,
        }
    }

    fn z80_space_write(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x3FFF => self.z80_ram[usize::from(a & 0x1FFF)] = v,
            0x4000..=0x5FFF => self.ym.write(a & 3, v),
            0x6000..=0x60FF => {
                self.z80_bank = ((self.z80_bank >> 1) | (u32::from(v & 1) << 8)) & 0x1FF
            }
            0x7F11 | 0x7F13 | 0x7F15 | 0x7F17 => self.psg.write(v),
            0x8000..=0xFFFF => {
                let addr = (self.z80_bank << 15) | u32::from(a & 0x7FFF);
                self.write8_68k(addr, v);
            }
            _ => {}
        }
    }

    fn vdp_read16(&mut self, a: u32) -> u16 {
        match a & 0x1F {
            0x00..=0x03 => self.vdp.read_data(),
            0x04..=0x07 => self.vdp.read_status(),
            0x08..=0x0F => self.vdp.hv_counter(self.line_cycle),
            _ => 0xFFFF,
        }
    }

    fn vdp_write16(&mut self, a: u32, v: u16) {
        match a & 0x1F {
            0x00..=0x03 => self.vdp.write_data(v),
            0x04..=0x07 => {
                // DMA source reads go through the 68000 map (ROM and RAM only on hardware).
                let rom = &self.rom;
                let ram = &self.ram;
                let mut read = |addr: u32| -> u16 {
                    let addr = addr & 0xFF_FFFE;
                    if addr >= 0xE0_0000 {
                        let i = (addr & 0xFFFF) as usize;
                        u16::from_be_bytes([ram[i], ram[i + 1]])
                    } else if (addr as usize) + 1 < rom.len() {
                        u16::from_be_bytes([rom[addr as usize], rom[addr as usize + 1]])
                    } else {
                        0
                    }
                };
                self.vdp.write_control(v, &mut read);
            }
            0x10..=0x17 => self.psg.write(v as u8),
            _ => {}
        }
    }

    fn read8_68k(&mut self, a: u32) -> u8 {
        let a = a & 0xFF_FFFF;
        match a {
            0x00_0000..=0x3F_FFFF => self.rom_byte(a),
            0xA0_0000..=0xA0_FFFF => {
                if self.z80_busreq || self.z80_reset {
                    self.z80_space_read(a as u16)
                } else {
                    0xFF
                }
            }
            0xA1_0000..=0xA1_001F => self.io_read(a),
            0xA1_1100..=0xA1_1101 => u8::from(!self.z80_busreq) | 0x80,
            0xC0_0000..=0xDF_FFFF => {
                let w = self.vdp_read16(a & !1);
                if a & 1 == 0 { (w >> 8) as u8 } else { w as u8 }
            }
            0xE0_0000..=0xFF_FFFF => self.ram[(a & 0xFFFF) as usize],
            _ => 0xFF,
        }
    }

    fn write8_68k(&mut self, a: u32, v: u8) {
        let a = a & 0xFF_FFFF;
        match a {
            0xA0_0000..=0xA0_FFFF => self.z80_space_write(a as u16, v),
            0xA1_0000..=0xA1_001F => self.io_write(a, v),
            0xA1_1100 => self.z80_busreq = v & 1 != 0,
            0xA1_1200 => self.z80_reset = v & 1 == 0,
            0xC0_0000..=0xDF_FFFF => {
                // Byte writes to the VDP repeat the byte in both halves.
                self.vdp_write16(a & !1, u16::from_be_bytes([v, v]));
            }
            0xE0_0000..=0xFF_FFFF => self.ram[(a & 0xFFFF) as usize] = v,
            _ => {}
        }
    }
}

impl m68k::Bus for Hardware {
    fn read8(&mut self, a: u32) -> u8 {
        self.read8_68k(a)
    }
    fn read16(&mut self, a: u32) -> u16 {
        let a = a & 0xFF_FFFF;
        match a {
            0x00_0000..=0x3F_FFFF => u16::from_be_bytes([self.rom_byte(a), self.rom_byte(a + 1)]),
            0xC0_0000..=0xDF_FFFF => self.vdp_read16(a),
            0xE0_0000..=0xFF_FFFF => {
                let i = (a & 0xFFFF) as usize;
                u16::from_be_bytes([self.ram[i], self.ram[i + 1]])
            }
            _ => u16::from_be_bytes([self.read8_68k(a), self.read8_68k(a + 1)]),
        }
    }
    fn write8(&mut self, a: u32, v: u8) {
        self.write8_68k(a, v);
    }
    fn write16(&mut self, a: u32, v: u16) {
        let a = a & 0xFF_FFFF;
        match a {
            0xC0_0000..=0xDF_FFFF => self.vdp_write16(a, v),
            0xE0_0000..=0xFF_FFFF => {
                let i = (a & 0xFFFF) as usize;
                [self.ram[i], self.ram[i + 1]] = v.to_be_bytes();
            }
            0xA1_1100 => self.z80_busreq = v & 0x100 != 0,
            0xA1_1200 => self.z80_reset = v & 0x100 == 0,
            _ => {
                let [h, l] = v.to_be_bytes();
                self.write8_68k(a, h);
                self.write8_68k(a + 1, l);
            }
        }
    }
    fn int_ack(&mut self, level: u8) -> Option<u8> {
        self.vdp.ack(level);
        None
    }
}

impl z80::Bus for Hardware {
    fn read(&mut self, a: u16) -> u8 {
        self.z80_space_read(a)
    }
    fn write(&mut self, a: u16, v: u8) {
        self.z80_space_write(a, v);
    }
}

/// A PC-address hook: when the 68000 is about to execute `pc`, `callback` runs first.
pub type Hook = Box<dyn FnMut(&mut Cpu, &mut Hardware)>;

/// The whole console.
pub struct Machine {
    pub cpu: Cpu,
    pub z80: Z80,
    pub hw: Hardware,
    hooks: Vec<(u32, Hook)>,
    pub frame_count: u64,
    /// Audio produced during the last frame: interleaved stereo i16 at [`SAMPLE_RATE`].
    pub audio: Vec<i16>,
    sample_acc: f64,
}

impl Machine {
    pub fn new(rom: Vec<u8>) -> Machine {
        let mut m = Machine {
            cpu: Cpu::new(),
            z80: Z80::new(),
            hw: Hardware::new(rom),
            hooks: Vec::new(),
            frame_count: 0,
            audio: Vec::new(),
            sample_acc: 0.0,
        };
        m.cpu.reset(&mut m.hw);
        m
    }

    /// Run `callback` whenever the 68000 reaches `pc` (before executing it).
    pub fn add_hook(&mut self, pc: u32, callback: Hook) {
        self.hooks.push((pc, callback));
    }

    /// Emulate one NTSC frame (262 lines).
    pub fn run_frame(&mut self) {
        self.audio.clear();
        for line in 0..LINES_PER_FRAME {
            self.hw.vdp.begin_line(line);
            if line == vdp::HEIGHT as u32 {
                self.hw.z80_int = true;
            }
            let mut c68 = 0u32;
            let mut cz = 0u32;
            let mut hblank_set = false;
            while c68 < M68K_CYCLES_PER_LINE {
                self.hw.line_cycle = c68;
                if !hblank_set && c68 >= M68K_CYCLES_PER_LINE - 84 {
                    self.hw.vdp.set_hblank(true);
                    hblank_set = true;
                }
                self.cpu.irq_level = self.hw.vdp.irq_level();
                if !self.hooks.is_empty() {
                    let pc = self.cpu.pc;
                    for (at, f) in self.hooks.iter_mut() {
                        if *at == pc {
                            f(&mut self.cpu, &mut self.hw);
                        }
                    }
                }
                c68 += self.cpu.step(&mut self.hw);
                // Keep the Z80 roughly in step (Z80 clock = 68000 clock * 15/7 / ... ≈ 0.467x).
                let z_target = c68 * Z80_CYCLES_PER_LINE / M68K_CYCLES_PER_LINE;
                while cz < z_target {
                    if self.hw.z80_reset {
                        self.z80.reset();
                        cz = z_target;
                        break;
                    }
                    if self.hw.z80_busreq {
                        cz = z_target;
                        break;
                    }
                    self.z80.int_line = self.hw.z80_int;
                    cz += self.z80.step(&mut self.hw);
                }
            }
            if line == vdp::HEIGHT as u32 {
                self.hw.z80_int = false;
            }
            self.hw.vdp.render_line(line);
            self.mix_line();
        }
        self.frame_count += 1;
    }

    fn mix_line(&mut self) {
        // Samples per line at 48 kHz for a 59.92 Hz, 262-line frame.
        let per_line = f64::from(SAMPLE_RATE) / (59.922_743 * f64::from(LINES_PER_FRAME));
        self.sample_acc += per_line;
        while self.sample_acc >= 1.0 {
            self.sample_acc -= 1.0;
            let (l, r) = self.hw.ym.sample(SAMPLE_RATE);
            let p = self.hw.psg.sample(SAMPLE_RATE);
            let mix = |v: i32| v.clamp(-32768, 32767) as i16;
            self.audio.push(mix(l + p));
            self.audio.push(mix(r + p));
        }
    }

    /// The last frame as RGBA8, `vdp::MAX_WIDTH` × `vdp::HEIGHT`.
    pub fn frame(&self) -> &[u8] {
        &self.hw.vdp.frame
    }
}
