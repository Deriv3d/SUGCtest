//! Sony MSF audio stream (the music file in `USRDIR/sounds/music`), and decoding of its
//! PlayStation ADPCM payload to 16-bit PCM.
//!
//! See `knowledge/formats/msf.md`. Header (0x40 bytes, big-endian u32): magic `MSFC`, codec,
//! channel count, data size, sample rate, flags, loop start, loop length; the rest is zero.

use std::fmt;

/// File magic.
pub const MAGIC: [u8; 4] = *b"MSFC";
/// Header size; audio data follows.
pub const HEADER_SIZE: usize = 0x40;
/// One PS-ADPCM frame: 2 header bytes + 14 bytes of 4-bit samples (28 samples).
pub const ADPCM_FRAME: usize = 16;
const SAMPLES_PER_FRAME: usize = 28;

/// Codec ids seen or documented for MSF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    PcmLe,
    PcmBe,
    PsAdpcm,
    Other(u32),
}

impl Codec {
    fn from_id(id: u32) -> Codec {
        match id {
            0 => Codec::PcmLe,
            1 => Codec::PcmBe,
            3 => Codec::PsAdpcm,
            other => Codec::Other(other),
        }
    }

    fn id(self) -> u32 {
        match self {
            Codec::PcmLe => 0,
            Codec::PcmBe => 1,
            Codec::PsAdpcm => 3,
            Codec::Other(id) => id,
        }
    }
}

/// Errors from parsing or decoding an MSF file.
#[derive(Debug, PartialEq, Eq)]
pub enum MsfError {
    TooShort,
    BadMagic,
    DataOutOfBounds,
    Unsupported(Codec),
}

impl fmt::Display for MsfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MsfError::TooShort => write!(f, "file shorter than the MSF header"),
            MsfError::BadMagic => write!(f, "not an MSF file (bad magic)"),
            MsfError::DataOutOfBounds => write!(f, "data size runs past end of file"),
            MsfError::Unsupported(c) => write!(f, "decoding {c:?} is not supported"),
        }
    }
}

impl std::error::Error for MsfError {}

/// A parsed MSF file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msf {
    pub codec: Codec,
    pub channels: u32,
    pub sample_rate: u32,
    pub flags: u32,
    pub loop_start: u32,
    pub loop_length: u32,
    /// The rest of the header after the loop fields, kept for byte-exact rebuilds.
    pub header_tail: Vec<u8>,
    pub data: Vec<u8>,
}

impl Msf {
    pub fn parse(bytes: &[u8]) -> Result<Msf, MsfError> {
        if bytes.len() < HEADER_SIZE {
            return Err(MsfError::TooShort);
        }
        if bytes[..4] != MAGIC {
            return Err(MsfError::BadMagic);
        }
        let w = |i: usize| {
            u32::from_be_bytes([
                bytes[4 * i],
                bytes[4 * i + 1],
                bytes[4 * i + 2],
                bytes[4 * i + 3],
            ])
        };
        let size = w(3) as usize;
        let data = bytes
            .get(HEADER_SIZE..HEADER_SIZE + size)
            .ok_or(MsfError::DataOutOfBounds)?
            .to_vec();
        Ok(Msf {
            codec: Codec::from_id(w(1)),
            channels: w(2),
            sample_rate: w(4),
            flags: w(5),
            loop_start: w(6),
            loop_length: w(7),
            header_tail: bytes[32..HEADER_SIZE].to_vec(),
            data,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for v in [
            self.codec.id(),
            self.channels,
            self.data.len() as u32,
            self.sample_rate,
            self.flags,
            self.loop_start,
            self.loop_length,
        ] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out.extend_from_slice(&self.header_tail);
        out.resize(HEADER_SIZE, 0);
        out.extend_from_slice(&self.data);
        out
    }

    /// Decode to interleaved signed 16-bit PCM.
    pub fn decode_pcm(&self) -> Result<Vec<i16>, MsfError> {
        match self.codec {
            Codec::PsAdpcm => Ok(decode_ps_adpcm(&self.data, self.channels.max(1) as usize)),
            Codec::PcmBe => Ok(self
                .data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| i16::from_be_bytes([c[0], c[1]]))
                .collect()),
            Codec::PcmLe => Ok(self
                .data
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect()),
            other => Err(MsfError::Unsupported(other)),
        }
    }
}

/// Prediction filters of PlayStation ADPCM (coefficients in 1/64 units).
const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// Decode PlayStation ADPCM. Channels are interleaved frame by frame.
pub fn decode_ps_adpcm(data: &[u8], channels: usize) -> Vec<i16> {
    let frames = data.len() / ADPCM_FRAME / channels;
    let mut out = vec![0i16; frames * SAMPLES_PER_FRAME * channels];
    let mut hist = vec![(0i32, 0i32); channels];
    for f in 0..frames {
        for (ch, h) in hist.iter_mut().enumerate() {
            let frame = &data[(f * channels + ch) * ADPCM_FRAME..][..ADPCM_FRAME];
            let shift = u32::from(frame[0] & 0x0F);
            let (c1, c2) = FILTERS[usize::from(frame[0] >> 4).min(4)];
            for i in 0..SAMPLES_PER_FRAME {
                let byte = frame[2 + i / 2];
                let nib = if i % 2 == 0 { byte & 0x0F } else { byte >> 4 };
                // Sign-extend the nibble into the top of a 16-bit value, then shift down.
                let raw = i32::from(((u16::from(nib) << 12) as i16) >> shift.min(12));
                let s = (raw + ((h.0 * c1 + h.1 * c2 + 32) >> 6)).clamp(-32768, 32767);
                *h = (s, h.0);
                out[(f * SAMPLES_PER_FRAME + i) * channels + ch] = s as i16;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(codec: u32, data: Vec<u8>) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        for v in [
            codec,
            1,
            data.len() as u32,
            44_100,
            0x10,
            0,
            data.len() as u32,
        ] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.resize(HEADER_SIZE, 0);
        b.extend(data);
        b
    }

    #[test]
    fn roundtrip_is_byte_exact() {
        let bytes = sample(3, vec![0x12; 64]);
        let m = Msf::parse(&bytes).unwrap();
        assert_eq!(m.codec, Codec::PsAdpcm);
        assert_eq!((m.channels, m.sample_rate), (1, 44_100));
        assert_eq!(m.to_bytes(), bytes);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Msf::parse(&[0; 8]), Err(MsfError::TooShort));
        let mut b = sample(3, vec![0; 16]);
        b[0] = b'X';
        assert_eq!(Msf::parse(&b), Err(MsfError::BadMagic));
        let mut b = sample(3, vec![0; 16]);
        b[12..16].copy_from_slice(&1000u32.to_be_bytes());
        assert_eq!(Msf::parse(&b), Err(MsfError::DataOutOfBounds));
    }

    #[test]
    fn adpcm_filter0_is_shifted_nibbles() {
        // Filter 0, shift 12: each nibble is the sample's top 4 bits shifted down by 12.
        let mut frame = [0u8; ADPCM_FRAME];
        frame[0] = 0x0C;
        frame[2] = 0x71; // samples 1 then 7
        frame[3] = 0x0F; // samples -1 then 0
        let pcm = decode_ps_adpcm(&frame, 1);
        assert_eq!(pcm.len(), 28);
        assert_eq!(&pcm[..4], &[1, 7, -1, 0]);
        // Shift 0 keeps the full 16-bit range.
        frame[0] = 0x00;
        assert_eq!(decode_ps_adpcm(&frame, 1)[1], 7 << 12);
    }

    #[test]
    fn adpcm_filter1_predicts_from_history() {
        let mut frames = [0u8; 2 * ADPCM_FRAME];
        frames[0] = 0x00; // filter 0, shift 0
        frames[2] = 0x01; // first sample 4096, rest 0
        frames[16] = 0x10; // second frame: filter 1, shift 0, all residuals 0
        let pcm = decode_ps_adpcm(&frames, 1);
        assert_eq!(pcm[0], 4096);
        assert_eq!(pcm[28], 0); // history is 0 at the end of frame 1
        // A pure filter-1 decay after a non-zero history.
        let mut f = [0u8; ADPCM_FRAME];
        f[0] = 0x10;
        let mut buf = [0u8; 2 * ADPCM_FRAME];
        buf[..16].copy_from_slice(&{
            let mut a = [0u8; ADPCM_FRAME];
            a[0] = 0x00;
            a[15] = 0x10; // last sample 4096
            a
        });
        buf[16..].copy_from_slice(&f);
        let pcm = decode_ps_adpcm(&buf, 1);
        assert_eq!(pcm[27], 4096);
        assert_eq!(i32::from(pcm[28]), (4096 * 60 + 32) >> 6);
    }

    #[test]
    fn pcm_be_passthrough() {
        let m = Msf::parse(&sample(1, vec![0x12, 0x34, 0xFF, 0xFE])).unwrap();
        assert_eq!(m.decode_pcm().unwrap(), vec![0x1234, -2]);
    }
}
