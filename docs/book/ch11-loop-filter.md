# Chapter 11 — Loop Filtering

> **Engineering takeaway:** After block-by-block reconstruction, the
> frame has visible "block boundary" artifacts because adjacent blocks
> were quantized independently. Loop filters smooth those boundaries
> *before* the frame goes into the DPB. "In-loop" means: subsequent
> frames see the filtered output as their reference, so the filter is
> part of the decoder's normative behavior — not optional post-
> processing. Every modern codec has a deblocking filter; HEVC adds
> SAO; AV1 adds CDEF and LR. As a decoder implementer, the filter
> code is straightforward but the per-edge / per-block parameter
> lookups are tedious.

After every block in a slice is reconstructed (stages 3–8 of the
pipeline), the slice has a problem: each block was quantized with its
own residual, and the boundaries between blocks don't quite line up.
The result is a **blockiness artifact** — visible 8×8 or 16×16 squares
in the reconstructed image, especially on smooth gradients.

The fix is an **in-loop filter** that smooths block boundaries before
the frame is finalized.

## 11.1 Why "in-loop" matters

There are two places you can put a smoothing filter:

- **Post-processing.** Apply after decode, before display. Doesn't
  affect the bitstream or future references.
- **In-loop.** Apply during decode, before the frame goes into the
  DPB. Affects what subsequent frames see as their reference.

Codecs use **in-loop** filtering because:

1. **Reference quality.** Filtered frames are cleaner references for
   inter prediction. P-frames predicting from filtered I-frames get
   better matches → smaller residuals → better compression.
2. **Drift prevention.** If the encoder and decoder don't agree on
   exactly what's in the DPB, drift compounds across frames. Making
   the filter normative (bit-exactly specified) eliminates this
   class of bug.

The downside: every decoder must implement the filter exactly as
specified. Post-processing filters can be optional and varied; loop
filters cannot.

## 11.2 The H.264 deblocking filter

H.264's deblocking filter is applied to each 4×4 (or 8×8) block
boundary inside an I/P/B picture. For each edge:

### Step 1 — boundary strength (bS)

Compute a per-edge value `bS ∈ {0, 1, 2, 3, 4}` based on the edge's
context:

```text
bS = 4  if either block is intra-coded and the edge is a macroblock edge
bS = 3  if either block is intra-coded (non-MB edge)
bS = 2  if either block has non-zero residual coefficients
bS = 1  if reference frames or MVs differ across the edge
bS = 0  if everything matches (smooth motion, no residual)
```

Higher `bS` = stronger filter applied. `bS = 0` skips this edge.

### Step 2 — gradient test

For each pixel column (or row, depending on edge orientation) crossing
the edge:

```text
p3 p2 p1 p0 | q0 q1 q2 q3        (block boundary in middle)
```

Apply a gradient-based test:
- If `|p0 - q0| < α` (threshold) and `|p1 - p0| < β` and `|q1 - q0| < β`:
  the edge is *smooth* — filter aggressively.
- Otherwise: filter weakly or not at all.

`α` and `β` are thresholds derived from the slice's QP (higher QP →
larger thresholds, since more aggressive smoothing is appropriate).

### Step 3 — apply the filter

For each crossing line, modify `p0, p1, q0, q1` (and sometimes `p2,
q2`) using small short-tap filters. Example for strong filtering:

```text
p0' = (p2 + 2·p1 + 2·p0 + 2·q0 + q1 + 4) >> 3
q0' = (p1 + 2·p0 + 2·q0 + 2·q1 + q2 + 4) >> 3
```

The exact filter formulas are in the H.264 spec [List2003]. Different
filter strengths use different formulas — typically a 4-tap or 5-tap
filter for normal edges, longer for strong edges.

### Implementation: tedious tables, simple math

A real H.264 deblocking filter is ~500–800 lines of Rust because:

- Different edge orientations (vertical / horizontal) have separate
  inner loops.
- Different filter strengths (bS = 1..4) have separate formulas.
- The α / β thresholds come from per-QP lookup tables.
- Chroma deblocking is a separate (similar but smaller) operation.
- Boundary cases at picture edges and slice edges need special
  handling.

Each piece is small; together they add up.

## 11.3 HEVC SAO — Sample Adaptive Offset

HEVC has the H.264 deblocking filter (slightly tuned) *plus* a second
in-loop filter: **SAO** (Sample Adaptive Offset).

SAO adjusts pixel values to better match the original, by classifying
each pixel into a category and applying a category-specific offset:

### Edge offset (EO)

For each pixel, compare it to its neighbours in a chosen direction
(horizontal, vertical, diagonal). Classify based on whether it's a
local maximum, minimum, or part of an edge:

```text
Category 0:  pixel < both neighbours along direction
Category 1:  pixel < one neighbour, == other
Category 2:  pixel == both neighbours
Category 3:  pixel > one neighbour, == other
Category 4:  pixel > both neighbours
```

For each category, the slice header signals an offset (small integer)
to add. The offsets compensate for systematic bias the codec introduced.

### Band offset (BO)

The pixel range [0, 255] is divided into 32 bands. The slice header
signals offsets for 4 consecutive bands (which 4 depend on encoder
choice). Pixels falling in those bands get adjusted.

### Decoder cost

SAO is per-CTU-signaled, with the SAO type, classification direction,
and offsets all in the bitstream. The decoder reads them, classifies
each pixel, applies the offset. Modest computational cost (an
addition per pixel after a small classification).

## 11.4 AV1 — three loop filters

AV1 increases the complexity:

### 1. Deblocking filter

Similar to HEVC's, applied to each block boundary. Different filter
strengths and thresholds, but the same structure (gradient test,
short-tap filter).

### 2. CDEF — Constrained Directional Enhancement Filter

CDEF targets **ringing artifacts** around real image edges. It's a
non-linear filter that:

1. Identifies the dominant edge direction in a small (8×8) block.
2. Applies smoothing perpendicular to that direction.
3. Limits the smoothing amount to avoid over-blurring.

For each block, the spec signals the primary direction and a strength
parameter. The decoder applies CDEF with those parameters.

CDEF is one of AV1's distinctive features. It gains ~3% BD-rate over
deblocking-only filtering, by reducing ringing artifacts that
deblocking can't address.

### 3. LR — Loop Restoration

A separate post-filter applied after CDEF, optionally per region:

- **Wiener filter**: a 7-tap separable filter with bit-exact integer
  coefficients signaled in the bitstream.
- **Self-guided restoration**: a guided filter that uses local
  statistics to recover detail lost in earlier stages.

LR gains another ~1% BD-rate. The decoder reads the filter
coefficients (or guided-filter parameters) per region and applies them.

### Overall AV1 filter pipeline

```text
reconstruct frame
     │
     ▼
deblocking filter
     │
     ▼
CDEF
     │
     ▼
LR (if enabled per region)
     │
     ▼
store in DPB
```

Each stage is independent in the bitstream signaling (enable/disable
flags per slice or per CTU/superblock). The decoder reads the flags
and dispatches accordingly.

## 11.5 What the decoder does — end to end

For each frame's loop filter stage:

1. Read filter parameters from the slice / frame header (filter
   strengths, enable flags, thresholds).
2. For deblocking: iterate over all block boundaries; for each:
   - Compute boundary strength (bS).
   - Apply the gradient test on each crossing line.
   - Apply the filter on lines that pass the test.
3. For SAO (HEVC): iterate over CTUs; for each:
   - Read SAO type from the bitstream (off / EO / BO).
   - Classify pixels and apply offsets.
4. For CDEF (AV1): iterate over 8×8 blocks; for each:
   - Read CDEF parameters.
   - Apply directional filter.
5. For LR (AV1): iterate over LR regions; for each:
   - Read filter type and parameters.
   - Apply Wiener or self-guided filter.

The frame, now fully filtered, is finalized and inserted into the
DPB.

## 11.6 SIMD considerations

Loop filtering is per-pixel and embarrassingly parallel within a row.
Production decoders heavily SIMD-optimize the deblocking inner loops
— they're 15–25% of total decode time on large resolutions.

dav1d's loop filter kernels (`src/x86/loopfilter_*.asm`) are the
state-of-the-art reference. They use AVX2 to process 16 or 32 pixels
in parallel per instruction.

CDEF is harder to SIMD-optimize because the direction selection has
data-dependent branches; dav1d works around this with explicit
vectorized comparison patterns.

## 11.7 Implementation pitfalls

**Edge ordering matters.** Deblocking is applied in a specific order
(typically left-to-right, top-to-bottom). Different orders produce
different results because earlier filtering changes the inputs to
later filtering. Match the spec's order exactly.

**Slice / tile boundaries.** Filtering across slice boundaries is
*optional* (signaled per slice). If disabled, the filter must stop at
the boundary. This complicates the inner loop because the iteration
ranges depend on slice geometry.

**Bit depth handling.** For 10-bit content, the filter coefficients
are the same but the intermediate computations need 16-bit (or wider)
arithmetic. Bit-exact correctness requires the right intermediate
widths.

**Pre-filtered vs. post-filtered samples.** When filtering an edge,
the samples on one side may have already been touched by a previous
edge's filter pass. The standard specifies which samples to use —
usually the pre-filter values from the original reconstruction. Get
this wrong and the filter cascade drifts.

## 11.8 Where this lives in the workspace

ProRes has **no loop filter** — it's intra-only with no inter
prediction, so block boundary artifacts don't cause drift, and any
smoothing is left to post-processing.

For codecs with loop filters (H.264, HEVC, AV1 when added):

```text
crates/oximedia-codec/src/h264/deblock.rs    ← ~600 lines
crates/oximedia-codec/src/hevc/deblock.rs    ← ~700 lines
crates/oximedia-codec/src/hevc/sao.rs        ← ~400 lines
crates/oximedia-codec/src/av1/deblock.rs     ← ~500 lines
crates/oximedia-codec/src/av1/cdef.rs        ← ~400 lines
crates/oximedia-codec/src/av1/loop_restore.rs ← ~300 lines
```

dav1d's loop filter files are the reference for production-quality
implementations.

## 11.9 Further reading

- **[List2003]** — the H.264 deblocking filter paper. Short, with
  full filter formulas.
- **[Sullivan2012]** §VIII — HEVC's deblocking and SAO.
- **[Chen2020]** §7 — AV1's deblock + CDEF + LR pipeline.
- **dav1d source** (`src/x86/loopfilter_*.asm`) — the production
  SIMD reference for AV1 loop filtering.

## 11.10 Exercises

1. **Block boundary intuition.** Sketch a smooth grayscale gradient
   covering a 16×16 area, then mark where 8×8 block boundaries are.
   After independent quantization with QP=32 for each block,
   describe what visible artifact you'd see along the boundaries.

2. **Boundary strength.** Two adjacent 4×4 blocks: the left one is
   intra-predicted, the right is inter-predicted with non-zero
   residual. What's the H.264 `bS` value for the edge between them?

3. **Filter strength.** For a slice with QP = 30, the α and β
   thresholds for H.264 deblocking are approximately 30 and 7
   respectively. A pixel pair across an edge has `|p0 - q0| = 35`.
   Does the filter activate? Why or why not?

4. **CDEF direction.** Sketch an 8×8 block with a strong diagonal
   edge (from top-left to bottom-right). What direction would CDEF
   detect as dominant? What does the directional filter then do?

5. *(Reading.)* In dav1d's `src/decode.c`, find where deblocking is
   invoked. Note the order (luma first, chroma later; vertical
   edges before horizontal). Why this order?

6. **Slice boundaries.** When deblocking is disabled across a slice
   boundary, what visual artifact would you expect? When would an
   encoder choose to disable it?

---

Next: [Chapter 12 — Reference Picture Management (DPB)](ch12-dpb.md).
