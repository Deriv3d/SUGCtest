# MSF music stream (`USRDIR/sounds/music/*.msf`)

Status: **understood (container)**. The decode is pending an owner listening check.

The disc has one file: `retro_dreams.msf`, 2,411,904 bytes. It's Sony's MSF stream format; this
note describes the variant on this disc.

## Header (0x40 bytes, big-endian u32)

| Offset | Field | This file |
| ---: | --- | --- |
| 0x00 | magic `MSFC` | — |
| 0x04 | codec | 3 = PlayStation ADPCM |
| 0x08 | channels | 1 |
| 0x0C | data size in bytes (data starts at 0x40) | 2,411,840 |
| 0x10 | sample rate | 44,100 |
| 0x14 | flags | 0x10 |
| 0x18 | loop start (bytes into the data) | 0 |
| 0x1C | loop length (bytes) | 2,411,840 (the whole track loops) |
| 0x20–0x3F | zero | — |

Other codec ids appear in MSF files generally (PCM big- and little-endian, ATRAC3, MPEG). Only codec 3 is on this disc.

## PlayStation ADPCM

The data is a run of 16-byte frames, interleaved per channel. Each frame holds 28 samples:
- **Byte 0:** low nibble is the shift (0–12); high nibble is the prediction filter (0–4).
- **Byte 1:** loop/end flags.
- **Bytes 2–15:** 4-bit residuals, low nibble first.

Each sample is the sign-extended residual, placed in the top 4 bits of a 16-bit value and shifted right by `shift`, plus a prediction from the previous two output samples. The prediction filter coefficients (in 1/64 units) are (0,0), (60,0), (115,−52), (98,−55) and (122,−60).

For this file that gives 150,740 frames = 4,220,720 samples = **95.7 s** at 44.1 kHz.

## Round trip and decode on the owner's disc (2026-10-06)
- `sugc-lab roundtrip`: **PASS**, byte-identical, and the payload decodes.
- `sugc-lab msf-wav`: decoded to a 16-bit WAV in the lab. No clipping; RMS about 3,800.

## Verification
- **Loopback comparison: inconclusive.** A loopback recording made 30–50 s after boot doesn't correlate with this track (0.032, the same as a no-match baseline). The game was probably playing other audio at that point.
- **Nibble order: inconclusive.** A decode with swapped nibble order also looks statistically plausible, so the layout above (the commonly documented one) still needs a listening check.
- Pending: the owner listens to the decoded WAV.

## Sound effects
The 17 sound effects in `flog_u.fpg` are plain RIFF/WAVE files: PCM, 16-bit, mono, 22,050 Hz, 12.0 s in total. They need no decoding.
