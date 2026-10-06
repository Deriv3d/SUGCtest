# knowledge/

Two kinds of file live here. Neither may contain game data, ROM bytes, keys, dumped
memory, or decompiled code. Describe what something does in our own words, cite where the
evidence lives in the lab (a path under the lab folder on the owner's machine), and stop.

## Field notes (`notes/NNNN-short-title.md`)

One note per non-obvious lesson (borrowed from universal-modder). Template:

```
# <lesson in one line>
Date: YYYY-MM-DD   Phase: N
What happened:
What we learned:
How to apply it next time:
```

## Claims (`claims.md`)

Every analysis claim is recorded with its evidence (borrowed from REA). Status is one of:

- `confirmed`: we observed it, evidence attached.
- `negative`: we looked and it is NOT the case, evidence attached (what we searched, how).
- `unknown`: not yet investigated, or investigated without a conclusive result.
- `hypothesis`: believed but not yet verified; must name the check that would settle it.

"Unknown" and "negative" are never merged: "we found no SPU code" is `negative` only if
we name the search that would have found it.
