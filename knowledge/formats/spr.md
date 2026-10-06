# SPR stream container (`streams/ui.spr`, `streams/global_binary.spr`)

Status: **partial**. The compression layer, the stream roles and the texture data are understood, and
`sugc-formats` round-trips both files and decodes their textures. The scene-graph record layout is not yet.

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

## Texture data streams

A texture-data stream is a run of blocks, one per texture, **in the same order as the descriptors** in the
paired structure stream:
- a 16-byte block header: ASCII `TEXL` followed by 12 zero bytes;
- then the texture's full mip chain, top level first, padded with zeros to a 16-byte boundary.

This accounts for every byte of all three texture streams: 230 + 2 + 2 blocks. The "3,740 spare bytes" above
are the 230 block headers plus padding.

The record holding each descriptor starts 32 bytes before it. That prefix contains the texture's GPU address
(0xC0000000 + descriptor offset) and its total data size, which is how `texture::find_textures` validates a
candidate descriptor.

**Pixel layout:**
- **DXT1/3/5 blocks** use the usual little-endian colour words, as on PC.
- **Uncompressed A8R8G8B8 textures** are stored in A, R, G, B byte order.
  - When the descriptor's linear flag is clear, texels are in Morton (swizzled) order.
  - Every linear texture on the disc has a non-power-of-two size; every swizzled one is a power of two.

**Evidence the decode is right**, without looking at the art:
- **Swizzling:** all 16 swizzled ARGB textures of 8×8 or more come out smoother (lower neighbour-pixel
  difference) when deswizzled than when read linearly.
- **Colour byte order:** for 180 of 189 DXT textures of 32×32 or more, block colours are smoother read as
  little-endian. The 9 exceptions are DXT5 textures, probably mostly-flat colour with detail in alpha.

The owner has confirmed that decoded textures match the game (see Verification).

## Round trip and decode on the owner's disc (2026-10-06)
- `sugc-lab roundtrip`: `ui.spr` **PASS** (4 streams, byte-identical, 4/4 inflate); `global_binary.spr`
  **PASS** (2 streams, byte-identical, 2/2 inflate).
- `sugc-lab spr-textures`: **234 of 234** textures decoded to PNG in the lab.
- The two G8B8 textures use channel remap 0xAAFE: RGB comes from the B byte (always 255) and alpha from
  the G byte (16 levels). That makes them white luminance+alpha glyph sheets, i.e. the font atlas (512×512),
  stored once in the loading-screen stream and once in the main UI stream. All other textures use the
  identity remap 0xAAE4.

## Verification
- 2026-10-06: the owner checked ten decoded textures (the six 1024×1024 ones and four others) and reports they
  all look right: real menu, box and border art, with no scrambling or colour swaps.
- Later: a texture-for-texture comparison with a RenderDoc capture of the menu.

## Scene-graph objects (structure streams)

Partial. Evidence: lab scripts `inventory\spr_*.py`, plus the GDB heap dump from the ROM memory check.

**Load addresses.** At runtime the structure streams sit at fixed heap addresses:

| Stream | Address |
| --- | --- |
| `global_binary.spr` stream 0 | 0x80200000 |
| `ui.spr` stream 0 | 0x80400000 |
| `ui.spr` stream 2 | 0x80500000 |

They were found by matching file content against guest memory with the UI loaded. Most of each stream is
unchanged in memory, so the file is close to an exact memory image. With those bases, almost all
pointers stay inside their own stream:

| Stream | Pointers inside it | Pointers into `global_binary` stream 0 | Pointers elsewhere |
| --- | ---: | ---: | ---: |
| `ui.spr` stream 2 | 6,995 | 21 | 1,789 |
| `global_binary` stream 0 | 5,686 | — | 767 |

**Object header.** `ui.spr` stream 2 holds 903 named objects, each starting on a 16-byte boundary:

| Offset | Field |
| ---: | --- |
| +0 | class word: an address in the executable's uninitialized data, so probably a runtime class-registry entry; no static RTTI is available there |
| +4 | **name hash = CRC-32 of the upper-cased name** (902 of 903). It's the same hash as the FPG archives. |
| +8, +12 | not yet known |
| +16 | the name, NUL-padded, in a 64- or 80-byte field |

**Classes, by class word:**

| Class word | Objects | Usual size (bytes) |
| --- | ---: | --- |
| 0x2B0B98 | about 400 | 528 (324 objects); some 592, 880 or 896 |
| 0x2B0C18 | 274 | 5,616 (210 objects); some 1,360 or 1,232 |
| 0x2B0EE8 | about 150 | 352 / 384; names suggest options, events and controller items |
| 0x2B1218 | 35 | 384 |
| 0x2B0B28 | a few | large; likely screens or containers |

**Sprite elements (class 0x2B0B98, 528-byte form).** Field profile across all 324 objects:

| Offset | Field |
| ---: | --- |
| +112, +116 | x, y as floats, in screen space (x from −16 to 1,106; y from −100 to 720; the screen is 1280×720) |
| +120 | depth, as a float (0.8–40) |
| +128, +132 | width, height, as floats (8–1,290 × 20–1,024) |
| +136 | always 1.0 |
| +144–+156 | RGBA tint as floats, mostly 1.0 |
| +192–+252, +320–+328 | copies of the position/size/colour block; probably animation start or current state |
| +344 | pointer, mostly to another sprite: probably a next-sibling link |
| +356 | pointer to an earlier table |
| +364, +368 | a second size pair (4–1,080 × 4–1,024); probably the source image size |

**Texture records.** In 176 of 230 cases the texture path string (`ui\cart\…`, `ui\box\…` and so on) sits exactly 280 bytes before the texture's GPU descriptor. So a texture record is: a path field, other fields, then the 32-byte prefix and the descriptor. The path is not 16-byte aligned, so it's a field inside a larger object.

**Not known yet:** how a sprite selects its texture. It isn't a pointer to the texture record, and it isn't a name hash; both were tested. That link is what's needed to rebuild a menu from the layout records.

## Open questions
- **Header page.** Its first word of each per-stream record (2, 6, 18, 111 in `ui.spr`) has no known meaning yet.
- **Fix-up.** How pointers are fixed up if the heap base differs. Settling check: the loader near 0x0003f7e0 / 0x0003fa10, and a GDB dump of the UI after load.

## Next steps
1. Glyph metrics for the font atlas: not yet found in the structure streams.
2. Find the sprite-to-texture link (likely through the 0x2B0C18 objects, or a table referenced from field +356), then rebuild one menu.
