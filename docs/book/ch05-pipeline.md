# Chapter 5 — The Decoder Pipeline in Detail

> **Engineering takeaway:** The decoder pipeline is eight stages: parse
> header → for each block { decode side info → form prediction →
> entropy-decode residual → dequantize → inverse-transform → add
> residual to prediction → store reconstructed } → loop filter → emit
> frame. Every chapter in Part II is the *expanded* version of one of
> these stages. Memorize the eight-step list; the rest of Part II is
> commentary.

In Chapter 1 you wrote a toy decoder for a stripped-down intra-only
codec. That decoder had a particular shape, and we claimed that the
shape transfers to every modern codec. This chapter cashes that claim
out in detail. We'll walk the pipeline from start to finish, naming
every stage, naming the data structures that flow between them, and
naming the chapters in Part II that go deep on each.

By the end of this chapter you should be able to **draw the pipeline
from memory** and explain what each stage takes as input and produces
as output. That mental skeleton is what you hang every subsequent
chapter on.

## 5.1 The pipeline as a sequence of typed functions

Treat each stage as a pure function with typed inputs and outputs.
That framing matches how the real decoder is structured in code and
makes each stage independently understandable.

```text
Input:    encoded bitstream (bytes)
              │
              ▼
   ┌─────────────────────────────────┐
   │ 1. Parse access unit boundaries │  → list of NAL units / OBUs
   └─────────────────────────────────┘
              │
              ▼
   ┌─────────────────────────────────┐
   │ 2. Parse headers                │  → SPS, PPS, slice_header,
   │    (SPS / PPS / slice_header)   │     QP, ref indices, etc.
   └─────────────────────────────────┘
              │
   ┌──────────┴──────────────────────────────────────┐
   │  For each coded block in the slice:             │
   │                                                 │
   │     ┌────────────────────────────────┐         │
   │     │ 3. Decode side info            │         │
   │     │    (mb_type, mode, MV deltas)  │         │
   │     └────────────────────────────────┘         │
   │              │                                 │
   │              ▼                                 │
   │     ┌────────────────────────────────┐         │
   │     │ 4. Form prediction             │         │
   │     │    intra: from neighbours      │         │
   │     │    inter: from ref frame + MV  │         │
   │     └────────────────────────────────┘         │
   │              │                                 │
   │              ▼                                 │
   │     ┌────────────────────────────────┐         │
   │     │ 5. Entropy-decode residual     │         │
   │     │    coefficients                │         │
   │     └────────────────────────────────┘         │
   │              │                                 │
   │              ▼                                 │
   │     ┌────────────────────────────────┐         │
   │     │ 6. Dequantize                  │         │
   │     │    coeff × Q-matrix × QP-scale │         │
   │     └────────────────────────────────┘         │
   │              │                                 │
   │              ▼                                 │
   │     ┌────────────────────────────────┐         │
   │     │ 7. Inverse transform           │         │
   │     │    coefficients → pixel resid. │         │
   │     └────────────────────────────────┘         │
   │              │                                 │
   │              ▼                                 │
   │     ┌────────────────────────────────┐         │
   │     │ 8. reconstructed = pred + resid│         │
   │     └────────────────────────────────┘         │
   │              │                                 │
   │              ▼                                 │
   │        store in frame buffer                   │
   │                                                 │
   └─────────────────────────────────────────────────┘
              │
              ▼
   ┌─────────────────────────────────┐
   │ 9. In-loop filtering            │
   │    deblock + (CDEF, SAO, LR)    │
   └─────────────────────────────────┘
              │
              ▼
   ┌─────────────────────────────────┐
   │ 10. Store in DPB, emit by PTS   │
   └─────────────────────────────────┘
              │
              ▼
        decoded YUV frame
```

Memorize this. Every chapter in Part II zooms in on one of these
boxes.

## 5.2 Stage 1 — Access unit boundaries

For Annex B framing (the typical live/streaming format), NAL units
are separated by start codes `00 00 00 01`. The decoder scans for
these. Each access unit (one frame worth of NAL units) is delimited
either by an AUD NAL or by the position of an IDR/SPS/PPS in the
stream.

For ISOBMFF framing (every fMP4/mp4 segment), the container hands you
sample-aligned byte ranges directly via the `mdat` and `stbl`/`trun`
metadata. No start-code scanning needed.

For AV1, the analogous unit is the OBU. The first byte's `obu_type`
field tells you what's in it (sequence header, frame header, tile
group, metadata, etc.). The OBU contains its own size field for self-
delimitation.

**Implementation note.** Most workspaces have a small "framer" or
"parser frontend" module dedicated to this — it doesn't decode
anything, just slices bytes into NAL units / OBUs. In this workspace
look at `crates/oximedia-codec/src/h264/framer.rs` or analogous
modules. The framer is a great first thing to implement when adding a
new codec; it's small, testable in isolation, and the rest of the
decoder depends on it.

## 5.3 Stage 2 — Header parsing

You learned this skill in Chapter 2. Open the spec, read fields in
order with a bit reader, into a typed struct.

The three header structures every H.264-family decoder needs:

- **SPS** (Sequence Parameter Set) — picture size, profile, level,
  bit depth, chroma format, VUI signaling. Per-sequence.
- **PPS** (Picture Parameter Set) — entropy coder mode, deblock
  filter parameters, default QP offset. Can change per-picture.
- **slice_header** — slice type (I/P/B), QP for this slice, reference
  picture lists, deblock overrides. Per-slice.

By the time you hit Stage 3 (per-block decoding), you have:

- A `SequenceContext` with picture dimensions and other static info.
- A `PictureContext` with the active PPS.
- A `SliceContext` with the slice's QP, ref lists, and partition info.

Each block decode reads from these contexts plus the bit reader's
current position.

## 5.4 Stages 3–8 — Per-block decoding

Here's where everything interesting happens. Walking it stage by
stage:

### Stage 3 — Side info

Before you can decode coefficients, you need to know what *kind* of
block you're looking at. Side info includes:

- `mb_type` / `pred_mode` — intra or inter? Which partition scheme
  (4×4 intra, 16×16 inter, etc.)?
- Intra direction (one of 9 for H.264 4×4, etc.)
- Motion vector deltas (and reference index for P/B)
- `coded_block_pattern` — which sub-blocks have non-zero coefficients
- `qp_delta` — per-block QP adjustment

All of these are entropy-coded. For H.264 CAVLC, they're variable-
length codes; for CABAC, context-modeled arithmetic-coded bins. The
decoder calls into the entropy module for each (more in Chapter 10).

### Stage 4 — Form prediction

Same block, two paths:

**Intra prediction (Chapter 6).** Use already-decoded neighbouring
pixels. The selected mode determines the formula:

- Mode 0 (vertical): copy the row above downward.
- Mode 1 (horizontal): copy the column to the left rightward.
- Mode 2 (DC): use the average of neighbours.
- Modes 3–8: directional extrapolation along specific angles.

The result is an 8×8 (or 16×16, etc.) prediction block — a complete
guess at what this block looks like.

**Inter prediction (Chapter 7).** Use a reference frame already in
the DPB. Side info gave you `(ref_idx, mv_x, mv_y)`. The decoder:

1. Looks up the reference frame at `ref_idx`.
2. Fetches a block at `(mv_x, mv_y)` from that frame.
3. If the MV has sub-pixel precision, applies an interpolation filter.

The result, as in intra, is a prediction block.

For bi-predictive (B) blocks, you do this twice — once per reference
— and average (or weighted-average) the two predictions.

### Stage 5 — Entropy-decode residual

The encoder transformed the residual (original − prediction), quantized
it, and entropy-coded the result. The decoder reverses the last step
first: from the bitstream, recover the (possibly sparse, mostly zero)
quantized transform coefficients for this block.

For H.264 CAVLC: parses TotalCoeff, TrailingOnes, the signed levels,
total_zeros, and run_before fields, then assembles them into the
8×8 (or 4×4) coefficient block.

For H.264 CABAC and the AV1 range coder: parses context-modeled
arithmetic bins. State per context is updated as each bin is decoded.

The output of this stage is an integer array, length matching the
block area (16 for 4×4, 64 for 8×8, etc.), still in scan order
(zigzag for H.264, or transform-dependent in AV1).

### Stage 6 — Dequantize

Multiply each coefficient by its quantization step. For codecs using
a quantization matrix, this means:

```text
coeff_dequantized[i][j] = coeff[i][j] × Q_matrix[i][j] × scale(QP)
```

The matrix shapes the noise (more precision in low-frequency, less in
high-frequency); `scale(QP)` is a global multiplier set by the slice
QP. Some codecs (H.264, HEVC) have a non-trivial integer math here to
avoid bit-mismatch between encoders. See Chapter 9 for the gritty
details.

### Stage 7 — Inverse transform

Apply the codec's specified inverse transform — IDCT, integer DCT
approximation, ADST (asymmetric discrete sine transform), identity,
etc. — to convert frequency coefficients back to spatial pixel
residuals.

Modern codecs do this with **bit-exact integer arithmetic** — every
decoder produces identical pixel values, with no rounding ambiguity.
The algorithms are specified down to the exact intermediate shift and
clip operations. See Chapter 8.

Output: a block of pixel residuals — signed integers, typically in a
range like [−2048, 2047] before clamping.

### Stage 8 — Add prediction to residual

The reconstruction:

```text
reconstructed[i][j] = clamp(prediction[i][j] + residual[i][j],
                             0, (1 << bit_depth) - 1)
```

The `clamp` handles overflow at the bit-depth boundary. For 8-bit:
[0, 255]. For 10-bit: [0, 1023]. The clamp matters — without it you
get wrap-around artifacts, which look like sudden bright pixels in
dark areas.

The reconstructed block is written into the current frame buffer at
its (x, y) location. Crucially, *this is the same reconstructed
pixel data the encoder saw* (because the encoder computed its own
prediction the same way and stored the same reconstructed value into
its DPB). Any drift between encoder and decoder reconstructions is
disaster. This is why every decoder operation must be bit-exact: see
Chapter 19.

## 5.5 Stage 9 — In-loop filtering

After every block of an access unit is reconstructed, the decoder
runs **in-loop filters** that smooth artifacts at block boundaries.

**Deblocking filter (every codec).** Quantization causes
discontinuities at block edges. The deblocking filter is a short
adaptive filter applied across block boundaries, smoothing the
transitions where appropriate. "Where appropriate" is decided by
analyzing the gradient: smooth regions get aggressive smoothing,
edges (real image edges, not artifacts) get little. H.264 and HEVC
both have this; AV1's is more elaborate but conceptually the same.
See Chapter 11.

**SAO — Sample Adaptive Offset (HEVC only).** A second pass that
adjusts pixel values to better match the original — either edge
offset (correcting band artifacts at edges) or band offset (shifting
specific brightness bands).

**CDEF — Constrained Directional Enhancement Filter (AV1).** A non-
linear filter targeting ringing artifacts around edges.

**LR — Loop Restoration (AV1).** A separate post-filter (Wiener filter
or self-guided restoration) that recovers detail lost in earlier
stages.

All of these are *in-loop* — meaning they happen *before* the frame
goes into the DPB, so subsequent frames see the filtered output as
their reference. This is why they're constraints on the bitstream
(every decoder must apply them identically) and not optional
post-processing.

## 5.6 Stage 10 — DPB management and emission

The **decoded picture buffer (DPB)** holds reconstructed frames that
might be used as references for future frames, plus frames that have
been decoded but not yet displayed (because B-frames cause decode
order to diverge from display order).

The decoder:

1. Inserts the just-reconstructed frame into the DPB.
2. Marks it as either short-term reference, long-term reference, or
   non-reference, based on the slice header's reference picture
   marking commands.
3. Removes old frames the encoder has signaled are no longer needed.
4. Emits frames in display order (by PTS) as soon as their display
   prerequisites are met.

This is the most subtle part of the decoder, and the place where
PTS/DTS divergence first matters. Chapter 12 explores DPB management
in depth, with the H.264 / HEVC reference picture set logic worked
out.

For your simplest intra-only codecs (ProRes, DNxHR, JPEG, the Ch 1
toy), there is no DPB: every frame is emitted immediately because
nothing references anything else. Inter-coded codecs need it.

## 5.7 Encoder side — mirror image, plus rate control

This book is decoder-first, but two encoder concerns leak into every
decoder discussion:

**Reconstruction in the encoder.** The encoder must *also* reconstruct
each block exactly as the decoder will — because the encoder's
prediction for the *next* block uses these reconstructed pixels as
neighbours. The encoder runs steps 5–8 (entropy *encode*, quantize,
forward transform, etc., and their inverses) to produce the same
reconstructed pixels the decoder will. Mismatch between encoder's
"shadow decoder" and the real decoder → drift.

**Rate control.** Decoders are not optimized for; they just decode.
Encoders are optimized — for a target bit budget under quality
constraints. The QP that flows into each slice is decided by the
encoder's *rate controller*, which trades quality (D) against bits
(R) using the Lagrangian J = D + λR. Chapter 13 covers rate control
in production (with citations into the workspace's existing
[`rate_control.md`](../rate_control.md)). Chapter 23 (optional) covers
the underlying theory.

For now: as a decoder implementer, you don't need to understand rate
control. You just need to know the encoder picked QP and signaled it;
you read it and use it.

## 5.8 Mapping the pipeline to the workspace

The [`oximedia-codec`](../../crates/oximedia-codec/) crate organizes
each codec around the same stages. Look at ProRes:

```text
crates/oximedia-codec/src/prores/
  framer.rs        ← stage 1: byte-range → frame buffer
  parser.rs        ← stage 2: frame header + slice header
  entropy.rs       ← stage 5: entropy decode (CAVLC variant)
  quant.rs         ← stage 6: dequant with per-slice matrix
  idct.rs          ← stage 7: integer IDCT
  assemble.rs      ← stage 8 (no prediction for ProRes): direct write
  mod.rs           ← top-level orchestration of the above
```

ProRes is intra-only, so stages 3 and 4 (side info / prediction) are
trivial (or absent). The structure still mirrors the pipeline.

A more complete inter-coded codec — when we add H.264 to the workspace
— will have additional modules for `intra.rs` (Ch 6), `motion.rs`
(Ch 7), `deblock.rs` (Ch 11), and `dpb.rs` (Ch 12). The skeleton stays
the same; new modules light up the missing stages.

When you read someone else's decoder source (FFmpeg's `libavcodec`,
dav1d, x264), look for these same names. Every codec source tree has
some recognizable analog of `parse_slice_header`, `decode_residual`,
`inverse_transform`, `apply_deblock`. The names vary; the shapes
don't.

## 5.9 Data structures that flow

What flows between stages, as concrete types:

| Between stages | Data structure                                  |
|----------------|-------------------------------------------------|
| 1 → 2          | NAL unit byte slices + framing metadata         |
| 2 → 3          | SPS / PPS / slice_header structs + bit reader   |
| 3 → 4          | `BlockInfo { mode, intra_dir or (ref, mv), … }` |
| 4 → 8          | `prediction: [[i32; W]; H]` (pixel-domain)      |
| 3 → 5          | `coded_block_pattern`, `qp_delta`              |
| 5 → 6          | `coeffs: [i32; N]` (still in scan order)        |
| 6 → 7          | `coeffs: [[i32; W]; H]` (dequantized, 2D)       |
| 7 → 8          | `residual: [[i32; W]; H]` (pixel-domain)        |
| 8 → 9          | reconstructed frame buffer (still mutable)      |
| 9 → 10         | filtered, finalized frame (read-only)           |

Note that prediction and residual are both in pixel-domain (signed
integers), and stage 8 just adds them and clips. The transformation
back to pixel domain happens at stage 7 (IDCT). Coefficient-domain
data (between stages 5 and 7) is never directly observable as an
image — it's a frequency representation.

## 5.10 Where the codecs differ

This pipeline shape is universal. What changes per codec:

| Stage | What varies                                                    |
|-------|----------------------------------------------------------------|
| 1     | Framing (Annex B vs. length-prefix vs. OBU)                    |
| 2     | Header structure and bit-level layout                          |
| 3     | Side info representation (CAVLC / CABAC / range coder)         |
| 4 intra | Mode set (9 / 35 / 56+); block sizes; chroma-from-luma       |
| 4 inter | Reference list management; MV prediction; sub-pel filters    |
| 5     | Entropy coder (CAVLC / CABAC / range coder)                    |
| 6     | Quantization matrix structure; QP scale                        |
| 7     | Transform sizes (4×4–64×64) and types (DCT / ADST / identity)  |
| 8     | (Pure addition; no codec variation)                            |
| 9     | Deblock-only / +SAO / +CDEF+LR                                 |
| 10    | DPB capacity and reference picture management rules            |

So *every chapter in Part II* is a column-by-column tour: same row,
multiple codecs.

## 5.11 What's next

Chapter 6 dives into Stage 4 (intra prediction), starting with what
the H.264 9 modes actually compute and ending with how AV1's 56-mode
zoo is structured. After that:

- Chapter 7 — Stage 4 (inter prediction): motion vectors, sub-pel
  filters, the DPB-as-prediction-source.
- Chapter 8 — Stage 7 (transforms): the DCT, integer transforms,
  AV1's family of separable transforms.
- Chapter 9 — Stage 6 (quantization): scalar vs matrix, perceptual
  shaping.
- Chapter 10 — Stage 5 (entropy coding): CAVLC, CABAC, the AV1 range
  coder. The deepest chapter in this book.
- Chapter 11 — Stage 9 (loop filtering): deblocking, SAO, CDEF, LR.
- Chapter 12 — Stage 10 (DPB): reference picture sets, picture order
  count, B-frame reordering.
- Chapter 13 — Encoder-side rate control. Optional for decoder
  implementers; read it once you've shipped.

## 5.12 Exercises

1. **Draw the pipeline from memory.** Without looking at §5.1, sketch
   the ten-stage diagram. Compare to the figure. What did you miss?

2. **For the toy decoder in Chapter 1**, identify which stages 1–10
   are present and which are trivial / absent. (Hint: 3, 4, 9, 10
   are mostly absent.)

3. *(Reading.)* Open
   [`crates/oximedia-codec/src/prores/mod.rs`](../../crates/oximedia-codec/src/prores/).
   Identify which function corresponds to each of the 10 stages
   above. Some will be inline, some in separate modules.

4. **Pick a codec and predict.** For HEVC, what would each of the 10
   stages look like at a high level? Don't read the spec — just
   guess from analogy to H.264, then check yourself in Chapter 14.

5. *(Optional.)* In FFmpeg's source, find
   `libavcodec/h264dec.c::ff_h264_decode_slice`. Read the function's
   top-level loop. Mark each major call with which of the 10 stages
   it covers. (Don't try to understand each helper — just count.)
