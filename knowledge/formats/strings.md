# Localized string tables (9 entries in `flog_c.fpg`)

Status: **understood (layout)**. Names unknown.

Nine of the twelve `flog_c.fpg` entries (65–74 KB each) are localized UI string tables. Earlier notes guessed they were fonts; they are not. All integers are **little-endian**, like the FPG header.

| Offset | Field |
| ---: | --- |
| 0x0 | entry count *N* (1,022 in every table) |
| 0x4 | *N* × u32: the absolute offset of each string. The first one equals 4 + 4*N, so there is no end sentinel. |
| … | the strings: UTF-8, NUL-terminated, back to back; the last runs to end of file |

Every table has the same 1,022 string ids, so a string id indexes the same message in every language. 57 of the strings are empty in each table.

Language of each table, from common-word counts:

| Language | Tables |
| --- | ---: |
| English | 3 (one also has about 2,000 CJK characters, probably the Asia-region table) |
| French | 2 |
| Spanish | 2 |
| German | 1 |
| Italian | 1 |

That's consistent with per-region variants of the same language.

Open questions:
- The file names. They aren't recovered by `STR_%s_%s.TXT`-style guesses.
- How the game picks a table: region global vs. system language.
- The tenth text entry, `SGC2_FW.SR` (118,912 bytes, plain text), is probably the string source or a firmware-text table. Not analysed yet.
