# Chapter 10 — Entropy Coding

> **Engineering takeaway:** Entropy coding is the *final* squeeze.
> Quantization produced a sparse vector of small integers; entropy
> coding maps that vector to bits, getting close to the theoretical
> entropy of the symbol distribution. There are three flavors you'll
> meet: **variable-length codes** (Huffman / CAVLC — simple lookup,
> H.264 baseline), **CABAC** (context-modeled arithmetic coding — H.264
> high profile, HEVC), and **range coding** (VP9, AV1 — mathematically
> similar to CABAC, different mechanics). All three exist for the same
> reason: turn a sparse integer sequence into bits at near-optimal
> rate. **CABAC is the hardest topic in the book**; this chapter walks
> through it with a worked trace.

This is the most complex stage of the decoder pipeline and the one
where decoder code spends the most lines. The other stages (transform,
quantization) are mostly bit-exact integer math you transcribe. Entropy
coding has *state* — context models that update as you read bits —
and getting the state machine right is what separates a conformant
decoder from a broken one.

## 10.1 What entropy coding is for

Recall from Chapter 1 (or Chapter 23 if you're reading optional theory)
that the entropy H(X) of a symbol distribution is the theoretical
minimum bits/symbol for any lossless code. Real entropy coders try to
achieve this bound:

```text
For a symbol X with probability p(X):
  Optimal codeword length = -log₂ p(X) bits.

So:  symbol with p = 0.5  →  1 bit  (a fair coin's outcome)
     symbol with p = 0.25 →  2 bits
     symbol with p = 0.99 →  0.015 bits (near-free if known to be skewed)
```

Quantization produced a sparse integer sequence — many zeros, some
small magnitudes, rare large ones. **The distribution is highly
skewed.** Entropy coding compresses skewed distributions; that's its
whole job.

The catch: entropy coding only works if you have a *model* of the
distribution. Different parts of the bitstream have different
statistics (DC coefficient distribution ≠ high-frequency coefficient
distribution ≠ motion vector distribution), so codecs maintain
*multiple* models (called **contexts**) and switch between them based
on which symbol they're about to code.

## 10.2 Variable-length codes (Huffman / CAVLC)

The simplest entropy coder is a **codebook**: a fixed table mapping
symbols to bitstrings, where short bitstrings go to frequent symbols.

```text
Symbol  Codeword (bits)
   0    `1`
   1    `01`
   2    `001`
   3    `0001`
   4    `00001`
   ...
```

This is **prefix-free** (no codeword is the prefix of another), so the
decoder can read bits one at a time until it has a complete codeword,
no boundary signaling needed.

The downside: codewords are integer numbers of bits. A symbol with
p = 0.7 has ideal length 0.5 bits, but you can't have a half-bit
codeword. So fixed Huffman codes lose 5–20% efficiency on highly skewed
distributions.

### H.264 CAVLC

CAVLC (Context-Adaptive Variable Length Coding) extends Huffman with
**multiple codebooks** that switch based on local context. For decoding
the residual coefficients of a 4×4 block, CAVLC parses five separate
fields:

1. **TotalCoeff** — how many non-zero coefficients in this block (0–16).
2. **TrailingOnes** — how many of the trailing non-zeros are ±1.
3. **Sign of TrailingOnes** — bit per trailing one.
4. **Levels** — the magnitudes of the remaining non-zero coefficients.
5. **TotalZeros** — total number of zeros before the last non-zero.
6. **Run before** — for each non-zero, how many zeros precede it.

Each field uses a different VLC table, selected based on the value of
previously-decoded fields (the "context-adaptive" part). The tables
themselves are specified in the H.264 standard.

This sounds elaborate, but each field's VLC table is small (a few
hundred entries), and the codebooks are deterministic — both encoder
and decoder use the same tables.

For implementation, the H.264 spec provides these tables explicitly.
You transcribe them. The CAVLC decoder for a 4×4 block is ~200 lines
of patient table lookups.

### Where CAVLC is used

H.264 Baseline / Main profiles use CAVLC. It's simpler than CABAC,
gives ~5–10% worse compression, and is easier to implement and faster
to decode.

## 10.3 Arithmetic coding (the underlying idea)

**Arithmetic coding** breaks the "integer bits per symbol" constraint.
Instead of mapping each symbol to a fixed codeword, it represents the
*entire sequence* as a single number in a sub-interval of [0, 1).

Conceptually:

```text
Start with the interval [0, 1).
For each symbol to encode:
  Look up the probability ranges for all possible symbols in this
  context (cumulative distribution).
  Narrow the current interval to the sub-range corresponding to
  this symbol.
```

After all symbols are encoded, the interval has some width;
transmitting any number inside the interval (typically the binary
representation of the midpoint) reconstructs the sequence.

```text
Example: 3 symbols A, B, C with probabilities 0.5, 0.3, 0.2.

Initial interval: [0, 1)
Symbol A:  narrow to [0.0, 0.5)        (the first 50% of the range)
Symbol B:  narrow to [0.25, 0.40)      (within the previous range,
                                         the 50%-80% sub-range)
Symbol C:  narrow to [0.37, 0.40)      (the 80%-100% sub-range)
Symbol A:  narrow to [0.370, 0.385)    (50% of 0.37-0.40)
...
```

The width of the final interval is the product of the symbol
probabilities. The number of bits to specify any number in that
interval equals `-log₂(width) = sum of -log₂(p_i)` — which is exactly
the entropy. **Arithmetic coding achieves the entropy bound** (asymptotically).

The decoder reverses this: receives the number, computes which
sub-range it falls into, that's the first symbol, narrow the model,
repeat.

### The implementation challenge

The straightforward description above uses arbitrary-precision floats
— impractical. Real implementations use **finite-precision integer
arithmetic**, periodically *renormalizing* the interval to keep it
within usable bit precision. The bits shifted out during renormalization
form the output bitstream.

The state machine is:

- Maintain `low` and `high` (the interval bounds, as 16- or 32-bit
  integers).
- When the high bit of `low` matches the high bit of `high`, both
  bits are settled; shift them out into the output.
- After enough symbols, the interval is small but bits are flowing.

This is conceptually simple but full of edge cases. **You do not write
your own arithmetic coder.** Every codec specifies its arithmetic coder
exactly, including the renormalization rules, the integer widths, and
the carry-propagation. You transcribe.

## 10.4 CABAC — Context-based Adaptive Binary Arithmetic Coding

H.264 / HEVC use **CABAC**, which is arithmetic coding with three
specific design choices:

1. **Binary.** Each pass codes one bit (a "bin"), not a multi-valued
   symbol. Multi-valued symbols are *binarized* first.
2. **Context-adaptive.** Each bin is coded with a probability model
   selected from a set of contexts. The context is chosen based on
   the bin's position and the values of surrounding bins.
3. **Adaptive.** Each context's probability is updated as bins are
   coded — slowly converging toward the true distribution.

### Binarization

A multi-valued symbol gets converted to a sequence of bins first:

```text
For a TransformCoeffLevel (a coefficient magnitude):
  Bin 0 (significant_coeff_flag): is this position non-zero?
  Bin 1 (last_significant_coeff_flag): is it the last non-zero?
  Bin 2 (coeff_abs_level_greater1_flag): is |coeff| > 1?
  Bin 3 (coeff_abs_level_greater2_flag): is |coeff| > 2?
  Bins 4+: exp-Golomb-coded suffix for |coeff| > 3
  Last bin: sign
```

Each of these bins gets its own context model (i.e., its own
probability estimate). The decoder reads bins one at a time, picking
the right context each time.

### Probability models and updates

Each context stores a probability state — typically a 6-bit integer
representing "this context expects bin=0 with this probability." After
each bin is coded:

- The arithmetic coder narrows the interval according to the context's
  probability.
- The context's probability state is updated toward the observed bin's
  value (so if bin=0, the "bin=0" probability is nudged up).

The probability state updates are specified by a lookup table:

```text
state_after_LPS[curr_state] = ...     // when the less probable symbol came
state_after_MPS[curr_state] = ...     // when the more probable symbol came
```

The table converges the model toward the true probability over time.

### Context selection

The trickiest part of CABAC is *which context to use for which bin*.
The H.264 spec has ~100 contexts total, and for each bin to be coded,
there's a deterministic rule mapping (bin type, current state of
neighbouring blocks, current state of just-decoded bins in this block)
→ a specific context index.

For example, when decoding `mb_skip_flag`:

```text
ctx = 0;
if (left neighbour's mb_skip_flag was 0):  ctx += 1;
if (above neighbour's mb_skip_flag was 0): ctx += 1;
```

This gives a context selected from {0, 1, 2} based on neighbour
behavior. The bin's probability model for this context has adapted
over the slice's history, so the context with "neighbour was 0"
tends to predict "this one is also 0" (and vice versa).

### A worked CABAC trace (small example)

Suppose we have a single context with state representing p(bin=0) =
0.7. We're decoding bins one at a time. The arithmetic coder state is
(low=0, high=0xFFFF), and we're reading from the bitstream.

```text
Read first bin. Interval narrows.
  If bin = 0: interval becomes [0, 0x7000)  (70% of width)
  If bin = 1: interval becomes [0x7000, 0xFFFF) (30% of width)
Bitstream tells us bin = 0.
  Narrow interval to [0, 0x7000).
  Update context state: p(bin=0) nudged up to 0.72.

Read second bin.
  Sub-divide [0, 0x7000) by 72/28.
  If bin = 0: interval [0, 0x4F70)
  If bin = 1: interval [0x4F70, 0x7000)
Bitstream tells us bin = 1.
  Narrow to [0x4F70, 0x7000).
  Update context: p(bin=0) nudged down to 0.69.

(Renormalization happens periodically when the high bit of low and
high settle.)
```

Real CABAC's state machine is more elaborate (handling the
renormalization, the bypass bins that skip context update for speed,
the "termination" bin at slice end), but this captures the rhythm:
read bin → use context to interpret → update context.

### Bypass bins

Some bins are coded in **bypass mode** — straightforward 50/50
arithmetic coding with no context model. These are typically the
sign bits and the exp-Golomb suffixes of large magnitudes, where
context modeling wouldn't help and bypass is faster. The decoder
recognizes "this bin is bypass" and uses a simplified interval-split
(50/50) with no model update.

Production CABAC decoders are dominated by the bypass-bin fast path
because bypass bins are common and parallel-decodable.

### Why CABAC is so productive

CABAC typically gains 5–15% BD-rate over CAVLC for H.264. This is
mostly because:

1. **Real probabilities** (not just integer-bit Huffman codes) — the
   information theory bound is hit, not approximated.
2. **Adaptation** — the model converges to local statistics, so
   different parts of a sequence get different effective codes.
3. **Per-context** — different bins have different statistics, and
   contexts capture that. Modern codecs have 100s of contexts.

The cost: CABAC is serial (each bin depends on the previous bin's
state), so it's hard to parallelize. Production decoders use specific
SIMD tricks for the inner loop, but per-block throughput is the
bottleneck for high-resolution decoding.

## 10.5 The range coder (VP9, AV1)

AV1 (and VP9) use a **range coder** that's mathematically equivalent
to arithmetic coding but uses a slightly different state machine:

```text
Arithmetic coder: tracks low / high
Range coder:      tracks low / range
```

The difference is bookkeeping; the encoded bitstreams are similar in
length. The range coder is easier to implement in fast SIMD-friendly
code, which is why AV1 chose it.

AV1's **symbol coder** layers contexts on top of the range coder, with
context modeling that's similar in spirit to CABAC but with a different
specific structure:

- Probabilities are stored as 15-bit integers (more precision than
  CABAC's 6-bit).
- Probability updates use a different formula (a linear adaptation
  rate).
- The symbol coder handles multi-symbol distributions directly (not
  just binary), so binarization is sometimes skipped.

For implementation: study dav1d's `src/decode.c` and AV1's symbol coder
documentation. The shape mirrors CABAC: read symbol → use context →
narrow range → update context.

## 10.6 Putting it all together — what the decoder does

For each block at stage 5 of the pipeline:

1. The decoder knows the codec (and thus the entropy coder type).
2. For each field needed:
   a. Determine the context (from spec rules based on
   neighbours / prior bins).
   b. Read bins (CABAC) / symbols (range coder) using the entropy
   coder's state machine.
   c. Update context state.
3. Assemble bins/symbols into the field's value.
4. Move on.

The implementer's job:

- Implement the arithmetic / range coder state machine (~200 lines).
- Implement each context selection rule (lots of small, table-driven
  logic).
- Implement the binarization for each multi-valued field (~50 fields
  in H.264).

Production CABAC implementations are **several thousand lines** of
code. Not because the math is hard, but because there are so many
context-selection rules and binarization patterns.

## 10.7 What makes entropy coding hard

**State dependence.** Every bin's interpretation depends on prior
bins. Get one bin wrong and the rest is garbage. Debugging is brutal —
you need a working reference decoder to compare against, and you
trace bin-by-bin until you find the divergence.

**Renormalization edge cases.** The arithmetic coder's renormalization
has specific edge cases (carry propagation, underflow handling) that
must match the spec exactly. Most bugs in production CABAC
implementations are in renormalization corner cases.

**Performance constraints.** Entropy decode is the serial bottleneck
of decode time. SIMD doesn't help much because each bin depends on
the previous. The trick is overlapping context lookups with bin reads
(software pipelining), which makes the code unreadable but fast.

**Conformance tests.** Bit-exact decoders are tested by feeding them
millions of bitstreams; an incorrect entropy decoder shows up as
*every test failing* in unpredictable ways. Build conformance testing
in early (Chapter 19).

## 10.8 Where this lives in the workspace

ProRes uses a **CAVLC variant** that's simpler than H.264's. See
[`crates/oximedia-codec/src/prores/entropy.rs`](../../crates/oximedia-codec/src/prores/).
The file is ~300 lines, dominated by table-based VLC decoding.

For H.264, when added, expect:

```text
crates/oximedia-codec/src/h264/cavlc.rs   ← ~400 lines
crates/oximedia-codec/src/h264/cabac.rs   ← ~2500 lines (the big one)
```

For AV1:

```text
crates/oximedia-codec/src/av1/range_coder.rs   ← ~300 lines
crates/oximedia-codec/src/av1/symbol_coder.rs  ← ~600 lines
crates/oximedia-codec/src/av1/contexts.rs      ← ~2000 lines of tables
```

The biggest reference codebases for studying production entropy
coding:

- **x264**'s `common/cabac.c` and `encoder/cabac.c` — H.264 CABAC.
- **dav1d**'s `src/decode.c` — AV1 symbol decoding.
- **JM** (H.264 reference) — slow but bit-exact, easy to read.

## 10.9 Further reading

- **[Marpe2003]** — the canonical CABAC paper, by the people who
  designed it. The clearest description of CABAC outside the standard.
- **[Wiegand2003]** §III.G — H.264 entropy coding overview.
- **[Sayood2017]** Chapters 4–6 — arithmetic coding from first
  principles, with worked examples.
- **[Sullivan2012]** §VII — HEVC's CABAC (similar to H.264's, with
  some extensions).
- **[Chen2020]** §6 — AV1's range coder and symbol coder.

The **Said and Pearlman** arithmetic coding tutorial papers are
classics. Search "Said Pearlman arithmetic coding tutorial."

For an in-the-trenches encoder's view, the [DarkShikari] blog has
posts on CABAC optimization that are illuminating about why CABAC is
hard to make fast.

## 10.10 Exercises

1. **VLC efficiency.** A symbol distribution has p(0) = 0.5, p(1) =
   0.25, p(2) = 0.125, p(3) = 0.125. What's the entropy? Construct a
   Huffman code; what's its average bit length? How close to the
   entropy is it?

2. **Arithmetic coding intuition.** A symbol sequence is `A B B A`
   with `p(A) = 0.7`, `p(B) = 0.3`. What's the final interval width
   after encoding the sequence? What's `-log₂(width)` (the entropy of
   this specific sequence)?

3. **CABAC binarization.** For a coefficient with magnitude 5, what
   bins are coded? (Hint: §10.4's bin sequence, with the exp-Golomb
   tail for magnitudes > 3.)

4. **Context selection.** For `mb_skip_flag`, the context depends on
   left and above neighbours' skip flags. If both neighbours were
   skipped, what's the context's likely probability estimate after
   seeing 100 such cases? (Hint: think about what "neighbours skipped"
   correlates with.)

5. *(Reading.)* Open
   [`crates/oximedia-codec/src/prores/entropy.rs`](../../crates/oximedia-codec/src/prores/).
   Find the VLC decoding for non-zero coefficient counts. Compare its
   shape to H.264 CAVLC TotalCoeff (which you'd need to study in the
   spec). What's similar? What's specific to ProRes?

6. **Production CABAC.** In FFmpeg's `libavcodec/h264_cabac.c`, find
   `decode_significant_coefficients`. Don't try to understand every
   line — just observe the pattern: read bin → look up context →
   update model. The number of contexts in H.264 (~460) is roughly
   the line count's complexity multiplier.

7. **Why range coding for AV1?** AV1 deliberately chose range coding
   over CABAC, even though they're mathematically similar. Why might
   range coding be easier to make fast? (Hint: think about which
   operations are easier in SIMD.)

---

Next: [Chapter 11 — Loop Filtering](ch11-loop-filter.md).
