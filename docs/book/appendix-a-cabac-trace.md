# Appendix A — A Complete CABAC Trace

> Chapter 10 introduced CABAC as the most complex stage of the
> decoder pipeline and promised a worked trace. Here it is. We'll
> decode several bins from a synthetic but realistic bitstream,
> tracking the arithmetic coder's state (`codIRange`, `codIOffset`)
> and the context probability state at every step. By the end you
> should be able to trace any CABAC bin in any spec — the mechanism
> is always the same.

## A.1 The decoder state

H.264 CABAC's decoder state is two registers:

- **`codIRange`** — the current range. Starts at `0x01FE` and shrinks
  as each bin is decoded, periodically renormalizing back toward
  `0x01FE`.
- **`codIOffset`** — the current "offset" into the range. Initialized
  by reading the first 9 bits of the slice's RBSP data.

These two registers are 9-bit (in the standard's formulation). The
arithmetic coder reads bits from the bitstream as needed during
renormalization.

Each context model is a pair:
- **`pStateIdx`** — index into a 64-entry probability state table.
- **`valMPS`** — which bin value (0 or 1) is currently the *More
  Probable Symbol* for this context.

The state index runs from 0 (50/50 split) to 63 (highly skewed toward
the MPS). The state table maps `pStateIdx` to the actual probability
of the LPS (Less Probable Symbol).

## A.2 The synthetic example

Imagine we're decoding the start of a slice with these conditions:

- Slice QP = 28.
- We're about to decode the first macroblock.
- The bitstream's CABAC payload starts with bytes (in hex):

```text
0x76 0x84 0x3F 0x21 0xC8 0x1B ...
```

Binary representation of the first few bits:

```text
0111 0110  1000 0100  0011 1111  0010 0001 ...
```

These are the bits we'll consume. We'll trace the decoding of the
first few bins.

## A.3 Initialization

At slice start, the arithmetic coder initializes:

```text
codIRange = 0x01FE = 510

codIOffset = read_bits(9)
           = first 9 bits of bitstream
           = 011101101 (binary)
           = 0xED  (3 high bits) | 1
           = 237  (decimal)
```

So after initialization: `codIRange = 510`, `codIOffset = 237`.

Each context is initialized per the H.264 spec, based on slice QP.
For QP=28 and context `MB_SKIP_FLAG_CTX0`, the spec's initialization
table gives:

```text
pStateIdx = 17     ← skewed toward MPS, but not extremely
valMPS    = 1      ← skip-flag = 1 (skipped) is the MPS
```

This means: the encoder expects most blocks to be marked "skip"
(true), and the decoder starts with that belief.

## A.4 Decoding the first `mb_skip_flag` bin

The first thing we'll decode in this slice's first macroblock is the
`mb_skip_flag` bin.

### Step 1 — pick the context

For `mb_skip_flag`, the spec's context-selection rule is:

```text
ctxIdx = 0   (base, slice type-dependent)
if left neighbour exists and its mb_skip_flag was 0:  ctxIdx += 1
if above neighbour exists and its mb_skip_flag was 0: ctxIdx += 1
```

For the first macroblock of the slice, there are no neighbours, so
both conditions are false. `ctxIdx = 0`. This is context
`MB_SKIP_FLAG_CTX0`.

Current state: `pStateIdx = 17, valMPS = 1`.

### Step 2 — find the range subdivision

The probability of the LPS (the less probable symbol, here 0) at state
17 is, from the spec's `rangeTabLPS` table:

```text
codIRangeLPS = rangeTabLPS[pStateIdx][slot]

where slot is derived from the high bits of codIRange.
```

For our `codIRange = 510`:

```text
slot = (codIRange >> 6) & 3 = (510 >> 6) & 3 = 7 & 3 = 3

rangeTabLPS[17][3] = 113  (from the spec's table)
```

So:
```text
codIRangeLPS = 113
codIRangeMPS = codIRange - codIRangeLPS = 510 - 113 = 397
```

The interval `[0, codIRange)` is split into:
- MPS sub-interval: `[0, 397)` → if `codIOffset < 397`, bin = MPS = 1
- LPS sub-interval: `[397, 510)` → if `codIOffset ≥ 397`, bin = LPS = 0

### Step 3 — decode the bin

Our `codIOffset = 237 < 397`. So we're in the MPS sub-interval.

**Decoded bin = valMPS = 1.**

This means: `mb_skip_flag = 1`. The first macroblock is skipped.

### Step 4 — update state

Since we got the MPS:

```text
codIRange = codIRangeMPS = 397    (no need to update codIOffset)
```

The context's `pStateIdx` updates per the spec's `transIdxMPS` table:

```text
pStateIdx_new = transIdxMPS[17] = 18
```

The state index increases by 1 (gentle update — we observed what the
context predicted). `valMPS` doesn't change. The context is now
slightly more confident that the MPS is value 1.

### Step 5 — renormalize

`codIRange = 397`. Since the high bit of codIRange (the 9th bit, bit
8) is 1 (`397 = 0b110001101`, bit 8 = 1), we don't need to
renormalize yet — the range is still in the valid range
`[256, 512)`.

If `codIRange` had dropped below 256, we'd shift `codIRange` left and
read another bit from the bitstream into `codIOffset`. We don't need
to here.

### State summary after first bin

```text
codIRange  = 397
codIOffset = 237
pStateIdx (MB_SKIP_FLAG_CTX0) = 18
valMPS    (MB_SKIP_FLAG_CTX0) = 1
```

That's *one bin* decoded. Note how much state changed for one bit of
output.

## A.5 Decoding a few more bins

Let's keep going. The next macroblock starts. We decode its
`mb_skip_flag` again.

### Setup for the second macroblock's skip bin

For the second macroblock, the left neighbour (the previous block)
has `mb_skip_flag = 1` (which equals MPS, so was the MPS, value 1 in
this case). The neighbour test "was it 0?" is false.

So `ctxIdx = 0` again. Same context as before, now updated state.

Context state: `pStateIdx = 18, valMPS = 1`.

### Decode

For `codIRange = 397`:

```text
slot = (397 >> 6) & 3 = 6 & 3 = 2

rangeTabLPS[18][2] = 81  (from spec table)
```

So:
```text
codIRangeLPS = 81
codIRangeMPS = 397 - 81 = 316

MPS sub-interval: [0, 316)
LPS sub-interval: [316, 397)
```

Our `codIOffset = 237 < 316`. **Decoded bin = MPS = 1**. Second block
is also skipped.

State update:
```text
codIRange = 316
pStateIdx = transIdxMPS[18] = 19
```

Renormalize? `codIRange = 316`, high bit (bit 8) is 1. Still in
range; no renormalization needed.

State after second bin:
```text
codIRange  = 316
codIOffset = 237
pStateIdx (MB_SKIP_FLAG_CTX0) = 19
```

### Decoding a non-MPS bin

Let's invent a third macroblock where the skip flag is 0 — this would
require `codIOffset ≥ codIRangeMPS` after the next step.

For `codIRange = 316`:

```text
slot = (316 >> 6) & 3 = 4 & 3 = 0

rangeTabLPS[19][0] = 18  (smaller LPS range — state 19 is more skewed)

codIRangeLPS = 18
codIRangeMPS = 316 - 18 = 298
```

If `codIOffset = 237 < 298`: MPS again. Bin = 1.

If `codIOffset = 305` (somewhere above 298): LPS! Bin = 0 (the LPS).

Suppose `codIOffset = 305`. Then we entered the LPS sub-interval:

```text
codIRange = codIRangeLPS = 18
codIOffset = codIOffset - codIRangeMPS = 305 - 298 = 7
```

State update for an LPS event:

```text
pStateIdx_new = transIdxLPS[19] = 13   (large drop — observation
                                         pulled us toward 50/50)

if pStateIdx == 0 before this LPS event:
    valMPS flips
else:
    valMPS unchanged
```

State 19 → 13 (significant drop). `valMPS` stays 1 because previous
state wasn't 0.

### Renormalization after LPS

`codIRange = 18`, which is now below 256. We need to renormalize.

Renormalization loop:

```text
while codIRange < 256:
    codIRange <<= 1                    // shift range left
    codIOffset <<= 1                   // shift offset left
    codIOffset |= read_bit()           // read a new bit into LSB
```

Step 1: `codIRange = 36, codIOffset = 14, read bit = 0` (next bit from
bitstream). `codIOffset = 14 << 1 | 0 = 28`.

Wait, let me re-check. After the LPS, `codIOffset = 7`. We shift left
and read a bit:

```
codIRange = 18 → 36
codIOffset = 7 → 14, then OR'd with next bitstream bit
```

Suppose the next bit is 1: `codIOffset = 14 | 1 = 15`.

Still `codIRange = 36 < 256`. Loop again:

```
codIRange = 36 → 72
codIOffset = 15 → 30, OR next bit (say 0): codIOffset = 30
```

Still under 256. Again:

```
codIRange = 72 → 144
codIOffset = 30 → 60, OR next bit (say 1): codIOffset = 61
```

Again:

```
codIRange = 144 → 288   ← now ≥ 256, exit loop
codIOffset = 61 → 122, OR next bit (say 0): codIOffset = 122
```

State after renormalization:
```text
codIRange  = 288
codIOffset = 122
```

We consumed 3 new bits during the renormalization. The CABAC's
amortized cost is "less than 1 bit per bin" on average — when contexts
predict well, many bins decode without renormalizing. When contexts
guess wrong (LPS), renormalization happens.

## A.6 A few notes for implementers

**State table layout.** The H.264 spec provides three tables you'll
need:

- `rangeTabLPS[pStateIdx][slot]` — 64 × 4 = 256 entries.
- `transIdxLPS[pStateIdx]` — 64 entries.
- `transIdxMPS[pStateIdx]` — 64 entries.

Plus the per-context initialization formula based on slice QP.

**Bypass bins.** Some bins are decoded in *bypass mode* — no context,
50/50 probability. For bypass:

```text
codIOffset <<= 1
codIOffset |= read_bit()    // always read a new bit
if codIOffset >= codIRange:
    bin = 1
    codIOffset -= codIRange
else:
    bin = 0
```

No probability state update. Faster than normal bins; used for sign
bits and exp-Golomb suffixes of large magnitudes.

**Termination bin.** At slice end, a special "termination" bin
indicates "no more bins." This uses a fixed probability and triggers
specific behavior.

## A.7 What this teaches you

After tracing this, you should appreciate:

1. **Most of the work is bookkeeping.** The actual arithmetic is
   a couple of integer ops; the work is updating state and
   reading lookup tables.

2. **Renormalization is the expensive part.** Each renormalization
   reads bits and shifts state. SIMD implementations work hard to
   handle multiple bins without renormalizing.

3. **Context selection is the biggest difference between codecs.**
   The arithmetic coder is the same in H.264, HEVC, VVC — the contexts
   are what changes.

4. **The state machine is local.** No global state changes per bin
   except (codIRange, codIOffset, one context). Easy to debug — print
   these four values at each step.

## A.8 Where this lives in the workspace

A full CABAC decoder for the workspace would live in:

```text
crates/oximedia-codec/src/h264/cabac.rs
```

The skeleton:

```rust
pub struct CabacDecoder<'a> {
    range: u16,
    offset: u16,
    bit_reader: BitReader<'a>,
    contexts: [Context; 460],  // H.264 has ~460 contexts
}

impl CabacDecoder<'_> {
    pub fn decode_bin(&mut self, ctx_idx: u16) -> u8 {
        // Standard CABAC decoding as in this appendix.
    }

    pub fn decode_bypass(&mut self) -> u8 {
        // 50/50 bypass bin.
    }
}
```

Each spec-defined "decode this field" wraps this primitive: read the
binarization pattern, dispatch to the right contexts, assemble the
final value.

For studying real production CABAC decoders, FFmpeg's
`libavcodec/h264_cabac.c` and x264's `common/cabac.c` are the
canonical implementations. Read after this appendix; you'll recognize
the structure.

## A.9 Further reading

- **[Marpe2003]** — the original CABAC paper. The author of the
  algorithm, writing about its design. Read this before opening the
  H.264 spec on CABAC.
- **H.264 / H.265 specs** §§7.3.x and §§9.3 — the CABAC syntax and
  decoding procedures, exhaustively specified.
- **[Sayood2017]** Chapter 5 — arithmetic coding theory from first
  principles, useful before reading CABAC's adaptations.

This appendix is the worked counterpart to Chapter 10. Together they
should be enough to implement CABAC from scratch.
