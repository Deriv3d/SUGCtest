# SPR stream container (`streams/ui.spr`, `streams/global_binary.spr`)

Status: **partial**. The compression layer is understood. What the inflated streams contain is
not analysed yet.

## Compression layer

All integers are **big-endian** u32, unlike the FPG archives.

- **Header page** (0x000–0xFFF). A small descriptor table, then per-stream records. Its fields aren't fully named yet. In `ui.spr`, four records hold (unknown small count, compressed total, uncompressed total). The uncompressed totals add up exactly to the file's total inflated size, so one record per stream is confirmed. Several header words are absolute addresses in the 0x80000000 range, which is where the game's main heap is mapped at runtime.
- **Streams.** Each stream starts on a 0x800 boundary and is a sequence of chunks.
- **Chunk.** A 16-byte header — ASCII tag `ZBLK`, chunk index within its stream (0, 1, 2, …), compressed size, uncompressed size — followed by a zlib stream.
  - Every chunk except the last in its stream inflates to exactly 65,000 bytes.
  - The next chunk starts at the next 16-byte boundary. Gaps are zero-filled.
  - The file is zero-filled to a multiple of 0x800.

| File | Size | Streams | Chunks | Inflated total |
| --- | ---: | ---: | ---: | ---: |
| `ui.spr` | 12,722,176 | 4 | 965 | 62,630,408 |
| `global_binary.spr` | 200,704 | 2 | 13 | 748,528 |

Every chunk on the owner's disc decompresses to its stated size (978 of 978).

## Working hypothesis
Both files look like serialized memory images loaded at a fixed heap address: the header holds absolute 0x80xxxxxx addresses, and the streams are large. If so, the inflated data is a pointer-bearing structure graph, probably with a relocation or fix-up list in one of the smaller streams.
- Status: hypothesis.
- Settling check: compare an inflated stream with the same region of guest memory dumped over GDB after the UI has loaded. Also find the loader via the `Streams\ui.spr` reference at 0x0003fa10 / path builder 0x0003f7e0.

## Next steps
1. Name the header fields, starting from the loader code at 0x0003f7e0.
2. Parser and writer in `sugc-formats` with a round-trip, as for FPG.
3. Identify the textures and fonts inside the UI stream.
