//! SN76489 programmable sound generator (register interface; synthesis in a later step).

#[derive(Debug, Clone, Default)]
pub struct Psg {
    pub regs: [u16; 8],
    latched: usize,
}

impl Psg {
    pub fn new() -> Psg {
        Psg {
            regs: [0, 0xF, 0, 0xF, 0, 0xF, 0, 0xF],
            latched: 0,
        }
    }

    pub fn write(&mut self, v: u8) {
        if v & 0x80 != 0 {
            self.latched = usize::from((v >> 4) & 7);
            let r = &mut self.regs[self.latched];
            *r = (*r & !0xF) | u16::from(v & 0xF);
        } else {
            let r = &mut self.regs[self.latched];
            if self.latched.is_multiple_of(2) && self.latched < 6 {
                *r = (*r & 0xF) | (u16::from(v & 0x3F) << 4);
            } else {
                *r = (*r & !0xF) | u16::from(v & 0xF);
            }
        }
    }

    /// One output sample at `rate` Hz.
    pub fn sample(&mut self, _rate: u32) -> i32 {
        0
    }
}
