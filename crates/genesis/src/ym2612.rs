//! YM2612 FM synthesizer (register interface; synthesis in a later step).

#[derive(Debug, Clone)]
pub struct Ym2612 {
    pub regs: [[u8; 256]; 2],
    addr: [u8; 2],
    port: usize,
}

impl Default for Ym2612 {
    fn default() -> Self {
        Self::new()
    }
}

impl Ym2612 {
    pub fn new() -> Ym2612 {
        Ym2612 {
            regs: [[0; 256]; 2],
            addr: [0; 2],
            port: 0,
        }
    }

    /// `a` is the low two address bits: 0/2 select the register in part I/II, 1/3 write data.
    pub fn write(&mut self, a: u16, v: u8) {
        let part = usize::from((a >> 1) & 1);
        if a & 1 == 0 {
            self.addr[part] = v;
            self.port = part;
        } else {
            self.regs[part][usize::from(self.addr[part])] = v;
        }
    }

    /// Status: busy flag and timer overflow flags (never busy here).
    pub fn read_status(&self) -> u8 {
        0
    }

    /// One stereo output sample at `rate` Hz.
    pub fn sample(&mut self, _rate: u32) -> (i32, i32) {
        (0, 0)
    }
}
