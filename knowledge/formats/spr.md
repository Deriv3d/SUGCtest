# SPR stream container (`streams/ui.spr`, `streams/global_binary.spr`)

Status: **partial**. The compression layer and the stream roles are understood. The scene-graph
record layout is not yet.

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

## What the streams contain

Streams come in pairs: a **structure stream**, which holds pointers, names and a scene graph,
and a **texture-data stream** that matches the textures that structure stream describes.

| File | Stream | Size | Contents |
| --- | ---: | ---: | --- |
| `ui.spr` | 0 | 40,528 | Loading-screen subset: a few UI elements, legal text, one font name, 2 texture descriptors |
| `ui.spr` | 1 | 524,336 | Pixel data for stream 0's main texture (512×512, 2 bytes/pixel = 524,288) plus 48 bytes |
| `ui.spr` | 2 | 1,877,528 | Main UI scene graph: named elements (menus, boxes, buttons, highlights), per-game art paths (`ui\cart\…`, `ui\box\…`, `ui\bord\…`), text labels, and **230 texture descriptors** |
| `ui.spr` | 3 | 60,188,016 | Pixel data for those 230 textures, packed back to back. The descriptors' mip chains add up to 60,184,276 bytes, 3,740 bytes short of the stream size. |
| `global_binary.spr` | 0 | 355,280 | Global data: about 2,600 distinct strings (menu, button and museum labels, movie names, text-table names), 2 texture descriptors |
| `global_binary.spr` | 1 | 393,248 | Pixel data for those 2 textures (256×512 + 512×512 DXT5 = 393,216 bytes) plus 32 bytes |

**Texture descriptors** have the PS3 GPU's standard 24-byte texture layout (format, mip count, dimension, cube flag, remap, width, height, depth, location, pitch, offset). In `ui.spr` stream 2 they are:
- 95 DXT1, 109 DXT5, 25 A8R8G8B8 and 1 G8B8;
- 59 marked linear;
- sizes from 4×4 up to 1024×1024.

All of them point into GPU local memory between 0x0B000000 and 0x0F8B0000. Those offsets are where the textures are placed at runtime; in the file they're packed without the gaps.

Structure streams hold many absolute addresses in the 0x80000000 range (about 1.4–2% of their words). That fits a memory image built to load at a fixed heap address.

## Open questions
- **Header page.** Its first word of each per-stream record (2, 6, 18, 111 in `ui.spr`) has no known meaning yet.
- **Texture order.** The order of textures inside the texture-data stream (descriptor order vs. GPU-offset order), and what the 3,740 spare bytes are.
- **Fix-up.** How pointers are fixed up if the heap base differs. Settling check: the loader near 0x0003f7e0 / 0x0003fa10, and a GDB dump of the UI after load.

## Next steps
1. Parser and writer in `sugc-formats` with a round-trip, as for FPG.
2. Decode the 230 textures to RGBA, then compare one with a RenderDoc capture of the menu that uses it.
3. Map the scene-graph records (element names, positions, texture references) for the viewer.
