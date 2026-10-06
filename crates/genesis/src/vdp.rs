//! Mega Drive / Genesis video display processor (VDP), written from public hardware
//! documentation. Scanline renderer; DMA completes instantly.

/// Pixels in the widest display mode.
pub const MAX_WIDTH: usize = 320;
/// Visible lines (NTSC, V28).
pub const HEIGHT: usize = 224;

#[derive(Clone)]
pub struct Vdp {
    pub vram: Vec<u8>,
    pub cram: [u16; 64],
    pub vsram: [u16; 40],
    pub reg: [u8; 24],
    code: u8,
    addr: u16,
    pending: bool,
    fill_pending: bool,
    pub vint_pending: bool,
    pub hint_pending: bool,
    hint_counter: i32,
    pub line: u32,
    in_vblank: bool,
    in_hblank: bool,
    read_buffer: u16,
    /// Last rendered frame, RGBA8, `MAX_WIDTH` × `HEIGHT`. Narrow modes are centred.
    pub frame: Vec<u8>,
    /// Width of the active display for the last frame (256 or 320).
    pub width: usize,
}

impl Default for Vdp {
    fn default() -> Self {
        Self::new()
    }
}

impl Vdp {
    pub fn new() -> Vdp {
        Vdp {
            vram: vec![0; 0x10000],
            cram: [0; 64],
            vsram: [0; 40],
            reg: [0; 24],
            code: 0,
            addr: 0,
            pending: false,
            fill_pending: false,
            vint_pending: false,
            hint_pending: false,
            hint_counter: 0,
            line: 0,
            in_vblank: false,
            in_hblank: false,
            read_buffer: 0,
            frame: vec![0; MAX_WIDTH * HEIGHT * 4],
            width: 320,
        }
    }

    fn h40(&self) -> bool {
        self.reg[12] & 0x81 != 0
    }

    pub fn display_enabled(&self) -> bool {
        self.reg[1] & 0x40 != 0
    }

    fn dma_enabled(&self) -> bool {
        self.reg[1] & 0x10 != 0
    }

    fn increment(&self) -> u16 {
        u16::from(self.reg[15])
    }

    /// Interrupt level the 68000 should see (0 = none).
    pub fn irq_level(&self) -> u8 {
        if self.vint_pending && self.reg[1] & 0x20 != 0 {
            6
        } else if self.hint_pending && self.reg[0] & 0x10 != 0 {
            4
        } else {
            0
        }
    }

    /// Interrupt acknowledge from the 68000.
    pub fn ack(&mut self, level: u8) {
        if level == 6 {
            self.vint_pending = false;
        } else if level == 4 {
            self.hint_pending = false;
        }
    }

    // ---------------- ports ----------------

    pub fn read_status(&mut self) -> u16 {
        self.pending = false;
        let mut s = 0x3400 | 0x0200; // FIFO empty
        if self.in_vblank || !self.display_enabled() {
            s |= 0x0008;
        }
        if self.in_hblank {
            s |= 0x0004;
        }
        if self.vint_pending {
            s |= 0x0080;
        }
        s
    }

    pub fn hv_counter(&self, cycle_in_line: u32) -> u16 {
        let v = if self.line > 0xEA {
            self.line - 6
        } else {
            self.line
        } as u16
            & 0xFF;
        let h = if self.h40() {
            let px = (cycle_in_line * 420 / 488) as u16;
            if px < 0xB6 { px } else { px + 0xE4 - 0xB6 }
        } else {
            let px = (cycle_in_line * 342 / 488) as u16;
            if px < 0x94 { px } else { px + 0xE9 - 0x94 }
        } & 0xFF;
        (v << 8) | h
    }

    /// Write to the control port. `read68k` serves 68000-to-VDP DMA source reads.
    pub fn write_control(&mut self, value: u16, read68k: &mut dyn FnMut(u32) -> u16) {
        if !self.pending && value & 0xC000 == 0x8000 {
            let r = usize::from((value >> 8) & 0x1F);
            if r < 24 {
                self.reg[r] = value as u8;
            }
            self.code &= 0x3C;
            return;
        }
        if !self.pending {
            self.code = (self.code & 0x3C) | ((value >> 14) as u8 & 3);
            self.addr = (self.addr & 0xC000) | (value & 0x3FFF);
            self.pending = true;
            return;
        }
        self.pending = false;
        self.code = (self.code & 3) | ((value >> 2) as u8 & 0x3C);
        self.addr = (self.addr & 0x3FFF) | ((value & 3) << 14);
        if self.code & 0x20 != 0 && self.dma_enabled() {
            match self.reg[23] >> 6 {
                0 | 1 => self.dma_68k(read68k),
                2 => self.fill_pending = true,
                _ => self.dma_copy(),
            }
        }
    }

    fn dma_length(&self) -> u32 {
        let l = u32::from(self.reg[19]) | (u32::from(self.reg[20]) << 8);
        if l == 0 { 0x10000 } else { l }
    }

    fn set_dma_done(&mut self, src_words_advanced: u32) {
        // Length registers count down to zero; the source address advances.
        let src = (u32::from(self.reg[21]) | (u32::from(self.reg[22]) << 8))
            .wrapping_add(src_words_advanced);
        self.reg[21] = src as u8;
        self.reg[22] = (src >> 8) as u8;
        self.reg[19] = 0;
        self.reg[20] = 0;
        self.code &= !0x20;
    }

    fn dma_68k(&mut self, read68k: &mut dyn FnMut(u32) -> u16) {
        let len = self.dma_length();
        let high = u32::from(self.reg[23] & 0x7F) << 17;
        let mut low = (u32::from(self.reg[21]) | (u32::from(self.reg[22]) << 8)) << 1;
        for _ in 0..len {
            let word = read68k(high | (low & 0x1_FFFF));
            self.write_target(word);
            low = low.wrapping_add(2);
        }
        self.set_dma_done(len);
    }

    fn dma_copy(&mut self) {
        let len = self.dma_length();
        let mut src = u16::from(self.reg[21]) | (u16::from(self.reg[22]) << 8);
        for _ in 0..len {
            let v = self.vram[usize::from(src)];
            self.vram[usize::from(self.addr)] = v;
            src = src.wrapping_add(1);
            self.addr = self.addr.wrapping_add(self.increment());
        }
        self.set_dma_done(len);
    }

    fn write_target(&mut self, v: u16) {
        match self.code & 0x0F {
            1 => {
                let a = usize::from(self.addr & 0xFFFE);
                let [hi, lo] = v.to_be_bytes();
                if self.addr & 1 == 0 {
                    self.vram[a] = hi;
                    self.vram[a + 1] = lo;
                } else {
                    self.vram[a] = lo;
                    self.vram[a + 1] = hi;
                }
            }
            3 => self.cram[usize::from((self.addr >> 1) & 0x3F)] = v & 0x0EEE,
            5 => {
                let i = usize::from((self.addr >> 1) & 0x3F);
                if i < 40 {
                    self.vsram[i] = v & 0x07FF;
                }
            }
            _ => {}
        }
        self.addr = self.addr.wrapping_add(self.increment());
    }

    pub fn write_data(&mut self, v: u16) {
        self.pending = false;
        if self.fill_pending {
            self.fill_pending = false;
            self.write_target(v);
            let len = self.dma_length();
            let [hi, _] = v.to_be_bytes();
            for _ in 0..len {
                self.vram[usize::from(self.addr ^ 1)] = hi;
                self.addr = self.addr.wrapping_add(self.increment());
            }
            self.set_dma_done(0);
            return;
        }
        self.write_target(v);
    }

    pub fn read_data(&mut self) -> u16 {
        self.pending = false;
        let v = match self.code & 0x0F {
            0 => {
                let a = usize::from(self.addr & 0xFFFE);
                u16::from_be_bytes([self.vram[a], self.vram[a + 1]])
            }
            8 => self.cram[usize::from((self.addr >> 1) & 0x3F)],
            4 => {
                let i = usize::from((self.addr >> 1) & 0x3F);
                if i < 40 { self.vsram[i] } else { self.vsram[0] }
            }
            _ => self.read_buffer,
        };
        self.read_buffer = v;
        self.addr = self.addr.wrapping_add(self.increment());
        v
    }

    // ---------------- timing ----------------

    /// Called at the start of each line (0..262).
    pub fn begin_line(&mut self, line: u32) {
        self.line = line;
        self.in_hblank = false;
        let visible = (line as usize) < HEIGHT;
        if line == 0 {
            self.in_vblank = false;
            self.width = if self.h40() { 320 } else { 256 };
        }
        if visible {
            self.hint_counter -= 1;
            if self.hint_counter < 0 {
                self.hint_counter = i32::from(self.reg[10]);
                self.hint_pending = true;
            }
        } else {
            self.hint_counter = i32::from(self.reg[10]);
        }
        if line as usize == HEIGHT {
            self.in_vblank = true;
            self.vint_pending = true;
        }
    }

    pub fn set_hblank(&mut self, on: bool) {
        self.in_hblank = on;
    }

    // ---------------- rendering ----------------

    fn color(&self, index: usize) -> [u8; 4] {
        let c = self.cram[index & 0x3F];
        let level = |v: u16| -> u8 { [0, 52, 87, 116, 144, 172, 206, 255][usize::from(v & 7)] };
        [level(c >> 1), level(c >> 5), level(c >> 9), 255]
    }

    fn tile_pixel(&self, tile: u16, px: u32, py: u32) -> u8 {
        let base = usize::from(tile & 0x7FF) * 32 + (py as usize) * 4 + (px as usize) / 2;
        let b = self.vram[base & 0xFFFF];
        if px & 1 == 0 { b >> 4 } else { b & 0xF }
    }

    fn plane_size(&self) -> (u32, u32) {
        let dim = |v: u8| match v & 3 {
            0 => 32,
            1 => 64,
            3 => 128,
            _ => 32,
        };
        (dim(self.reg[16]), dim(self.reg[16] >> 4))
    }

    /// Pixel of a scrolled plane: (colour index 0..63 or 0 if transparent, priority).
    fn plane_pixel(&self, base: usize, w_cells: u32, h_cells: u32, x: u32, y: u32) -> (u8, bool) {
        let (cx, cy) = (x / 8, y / 8);
        let entry_addr = base + ((cy * w_cells + cx) as usize) * 2;
        let e = u16::from_be_bytes([
            self.vram[entry_addr & 0xFFFF],
            self.vram[(entry_addr + 1) & 0xFFFF],
        ]);
        let _ = h_cells;
        let mut px = x & 7;
        let mut py = y & 7;
        if e & 0x0800 != 0 {
            px = 7 - px;
        }
        if e & 0x1000 != 0 {
            py = 7 - py;
        }
        let c = self.tile_pixel(e, px, py);
        let pal = ((e >> 13) & 3) as u8;
        (if c == 0 { 0 } else { pal * 16 + c }, e & 0x8000 != 0)
    }

    /// Render one visible line into `frame`.
    pub fn render_line(&mut self, line: u32) {
        let y = line as usize;
        if y >= HEIGHT {
            return;
        }
        let width = if self.h40() { 320 } else { 256 };
        let x_off = (MAX_WIDTH - width) / 2;
        let bg = self.color(usize::from(self.reg[7] & 0x3F));
        let row_start = y * MAX_WIDTH * 4;
        if !self.display_enabled() {
            for x in 0..MAX_WIDTH {
                self.frame[row_start + x * 4..row_start + x * 4 + 4].copy_from_slice(&bg);
            }
            return;
        }
        let (pw, ph) = self.plane_size();
        let a_base = (usize::from(self.reg[2]) & 0x38) << 10;
        let b_base = (usize::from(self.reg[4]) & 0x07) << 13;
        let w_base = if self.h40() {
            (usize::from(self.reg[3]) & 0x3C) << 10
        } else {
            (usize::from(self.reg[3]) & 0x3E) << 10
        };
        let hs_base = (usize::from(self.reg[13]) & 0x3F) << 10;
        let hs_off = match self.reg[11] & 3 {
            0 => 0,
            2 => (y & !7) * 4,
            3 => y * 4,
            _ => (y & 7) * 4,
        };
        let rd =
            |a: usize| u16::from_be_bytes([self.vram[a & 0xFFFF], self.vram[(a + 1) & 0xFFFF]]);
        let hs_a = u32::from(rd(hs_base + hs_off) & 0x3FF);
        let hs_b = u32::from(rd(hs_base + hs_off + 2) & 0x3FF);
        let col_vs = self.reg[11] & 4 != 0;
        let wmask = pw * 8 - 1;
        let hmask = ph * 8 - 1;

        // Window region for this line.
        let wv = self.reg[18];
        let wh = self.reg[17];
        let line_in_window_rows = {
            let pos = u32::from(wv & 0x1F) * 8;
            if wv & 0x80 != 0 {
                line >= pos
            } else {
                line < pos
            }
        };
        let win_split = u32::from(wh & 0x1F) * 16;
        let win_right = wh & 0x80 != 0;
        let win_cells = if self.h40() { 64 } else { 32 };

        // Sprites for this line.
        let mut spr_line = [(0u8, false); MAX_WIDTH];
        self.render_sprites(line, width, &mut spr_line);

        for (x, &(s_px, s_pri)) in spr_line.iter().enumerate().take(width) {
            let xu = x as u32;
            let col = (xu / 16) as usize;
            let vs_a = u32::from(
                if col_vs {
                    self.vsram[(col * 2) % 40]
                } else {
                    self.vsram[0]
                } & 0x3FF,
            );
            let vs_b = u32::from(
                if col_vs {
                    self.vsram[(col * 2 + 1) % 40]
                } else {
                    self.vsram[1]
                } & 0x3FF,
            );
            let in_window = line_in_window_rows
                || if win_right {
                    xu >= win_split
                } else {
                    xu < win_split
                };
            let (a_px, a_pri) = if in_window
                && (wv & 0x1F != 0 || wh & 0x1F != 0 || wv & 0x80 != 0 || wh & 0x80 != 0)
            {
                self.plane_pixel(w_base, win_cells, 32, xu, line)
            } else {
                let px = (xu.wrapping_sub(hs_a)) & wmask;
                let py = (line + vs_a) & hmask;
                self.plane_pixel(a_base, pw, ph, px, py)
            };
            let px = (xu.wrapping_sub(hs_b)) & wmask;
            let py = (line + vs_b) & hmask;
            let (b_px, b_pri) = self.plane_pixel(b_base, pw, ph, px, py);
            // Priority order (front to back): S hi, A hi, B hi, S lo, A lo, B lo, backdrop.
            let layers = [(s_px, s_pri), (a_px, a_pri), (b_px, b_pri)];
            let mut chosen = None;
            for want_hi in [true, false] {
                for &(p, pri) in &layers {
                    if pri == want_hi && p != 0 {
                        chosen = Some(p);
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            let rgba = match chosen {
                Some(p) => self.color(usize::from(p)),
                None => bg,
            };
            let o = row_start + (x + x_off) * 4;
            self.frame[o..o + 4].copy_from_slice(&rgba);
        }
        for x in (0..x_off).chain(x_off + width..MAX_WIDTH) {
            let o = row_start + x * 4;
            self.frame[o..o + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }

    fn render_sprites(&self, line: u32, width: usize, out: &mut [(u8, bool); MAX_WIDTH]) {
        let h40 = self.h40();
        let base = if h40 {
            (usize::from(self.reg[5]) & 0x7E) << 9
        } else {
            (usize::from(self.reg[5]) & 0x7F) << 9
        };
        let max_total = if h40 { 80 } else { 64 };
        let max_line = if h40 { 20 } else { 16 };
        let max_pixels = if h40 { 320 } else { 256 };
        let rd =
            |a: usize| u16::from_be_bytes([self.vram[a & 0xFFFF], self.vram[(a + 1) & 0xFFFF]]);
        let mut idx = 0usize;
        let mut on_line = 0;
        let mut pixels = 0;
        let mut seen_nonzero_x = false;
        let mut masked = false;
        for _ in 0..max_total {
            let e = base + idx * 8;
            let sy = u32::from(rd(e) & 0x3FF);
            let size = self.vram[(e + 2) & 0xFFFF];
            let link = usize::from(self.vram[(e + 3) & 0xFFFF] & 0x7F);
            let attr = rd(e + 4);
            let sx = u32::from(rd(e + 6) & 0x1FF);
            let wc = u32::from((size >> 2) & 3) + 1;
            let hc = u32::from(size & 3) + 1;
            let top = sy as i32 - 128;
            let ly = line as i32 - top;
            if ly >= 0 && (ly as u32) < hc * 8 {
                on_line += 1;
                if on_line > max_line {
                    break;
                }
                if sx == 0 {
                    if seen_nonzero_x {
                        masked = true;
                    }
                } else {
                    seen_nonzero_x = true;
                }
                let mut ry = ly as u32;
                if attr & 0x1000 != 0 {
                    ry = hc * 8 - 1 - ry;
                }
                let pal = ((attr >> 13) & 3) as u8;
                let pri = attr & 0x8000 != 0;
                for dx in 0..wc * 8 {
                    if pixels >= max_pixels {
                        break;
                    }
                    pixels += 1;
                    if masked {
                        continue;
                    }
                    let sxp = sx as i32 - 128 + dx as i32;
                    if sxp < 0 || sxp as usize >= width {
                        continue;
                    }
                    let mut rx = dx;
                    if attr & 0x0800 != 0 {
                        rx = wc * 8 - 1 - rx;
                    }
                    let tile = (attr & 0x7FF).wrapping_add(((rx / 8) * hc + ry / 8) as u16);
                    let c = self.tile_pixel(tile, rx & 7, ry & 7);
                    let slot = &mut out[sxp as usize];
                    if c != 0 && slot.0 == 0 {
                        *slot = (pal * 16 + c, pri);
                    }
                }
            }
            if link == 0 || link >= max_total {
                break;
            }
            idx = link;
        }
    }
}
