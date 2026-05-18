# Chapter 7 — Motion Estimation and Compensation

> **Engineering takeaway:** When you see a P-frame or B-frame block,
> the bitstream tells you (a) which previously-decoded frame to look
> at, and (b) a motion vector saying where in that frame to look. The
> decoder fetches the indicated block — possibly with sub-pixel
> interpolation — and uses it as the prediction. That's motion
> *compensation* (the decoder's job). Motion *estimation* (finding the
> best vector) is an encoder problem, expensive, and not your concern
> as a decoder implementer.

Inter prediction is where 90% of a typical video codec's compression
gains live. Once you have a previous frame already decoded, predicting
the current frame from it is much more powerful than predicting from
the same-frame neighbours that intra prediction uses.

This chapter covers the decoder side of inter prediction:
**compensation**. We'll briefly visit the encoder side
(**estimation**) at the end, because some encoder concepts (MV
prediction, merge mode) leak into how the decoder reads its inputs.

## 7.1 The motion compensation primitive

Given:
- A reference frame `ref` (sitting in the DPB).
- A block position `(x, y)` in the current frame.
- A motion vector `(mv_x, mv_y)` saying "this block in the current
  frame matches the block at `(x + mv_x, y + mv_y)` in `ref`".

The prediction is just:

```rust
fn motion_compensate(ref_frame: &Frame, x: i32, y: i32,
                     mv_x: i32, mv_y: i32) -> Block {
    let src_x = x + mv_x;
    let src_y = y + mv_y;
    copy_block(ref_frame, src_x, src_y)
}
```

That's the entire concept. The "copy a block from another frame" is
ALU-trivial. What makes motion compensation interesting (and slow) in
real codecs is everything around it.

## 7.2 Sub-pixel motion

Real-world motion isn't integer pixels. A pan that moves the camera
3.2 pixels per frame can't be captured by integer MVs. So codecs
support **fractional motion vectors** — typically ¼-pixel for H.264 /
HEVC, ⅛-pixel for AV1.

To fetch at a fractional offset, the decoder **interpolates** integer-
grid samples. H.264 uses a **6-tap filter** for half-pixel positions:

```text
Half-pel sample = (1·a − 5·b + 20·c + 20·d − 5·e + 1·f) / 32

where a, b, c, d, e, f are the six adjacent integer samples
along the direction of interpolation.
```

For ¼-pixel positions, half-pel and integer samples are averaged.

HEVC uses **7-tap and 8-tap filters** with different coefficients,
designed for better frequency response.

AV1 has **multiple filter sets** (regular, smooth, sharp) and signals
which to use per-block. The encoder picks based on content
characteristics; the decoder reads the signal and applies the
corresponding filter.

### Why specific filter coefficients matter

The filter coefficients are **specified bit-exactly** in the standard.
Every decoder must use the same coefficients, applied with the same
intermediate precision, with the same rounding rules. **Any deviation
produces drift**: the decoder's reconstructed frame won't match the
encoder's, future predictions will compound the error, and the picture
will progressively degrade until the next IDR.

This is why interpolation filters are a frequent target for SIMD
optimization (these are the inner loops a decoder spends time in) and
also a frequent source of bugs (off-by-one in the rounding round-trip
breaks conformance).

Workspace example: when a decoder for H.264 is added,
`crates/oximedia-codec/src/h264/mc.rs` will contain bit-exact 6-tap
and ¼-pel filters. See the dav1d AV1 decoder for production-quality
SIMD versions of the same filters.

## 7.3 The reference picture list

A P-frame block predicts from one reference; a B-frame block predicts
from up to two. Where does the decoder find these references?

The slice header carries a **reference picture list** (in H.264, two
lists: `L0` for the past references, `L1` for the future ones). The
block's side info includes a small integer `ref_idx_l0` (and
`ref_idx_l1` for bi-prediction) that indexes into this list.

The decoder, in turn, knows what's in each list because the SPS / PPS
/ slice header has explicitly enumerated the reference pictures (or
the encoder used a default list construction algorithm whose result
the decoder mirrors). Either way: by the time the decoder hits the
block, `L0[ref_idx_l0]` is a concrete pointer into the DPB.

## 7.4 Motion vector prediction (MVP)

A motion vector can be up to ~22 bits in H.264 (large displacement
support). Sending one per block costs real bits — for a small block
in a busy scene, the MV bits dominate the bits-for-residual.

The solution: **predict the MV from neighbour MVs**, then transmit
only the *delta* between predicted and actual.

### H.264 median predictor

For H.264, the MV predictor is the **median** (per axis) of the MVs
from three neighbours: left, above, above-right:

```rust
let mvp_x = median3(left.mv_x, above.mv_x, above_right.mv_x);
let mvp_y = median3(left.mv_y, above.mv_y, above_right.mv_y);
let mvd_x = signaled_in_bitstream;
let actual_mv_x = mvp_x + mvd_x;
```

Where motion is locally smooth (panning camera, slow-moving subject),
`mvd` is small or zero, and the encoder spends very few bits.

### HEVC AMVP

HEVC's **Advanced MVP** builds a small list of MV candidates from
spatial neighbours and a temporal collocated block, then signals an
index plus a delta. The list construction is deterministic — both
encoder and decoder build the same list from the same available data.

### Merge mode (HEVC / AV1)

The big innovation in HEVC: **merge mode** lets the encoder say "use
exactly the MV from neighbour N, no delta." A single small index
(usually 1–3 bits) encodes the whole motion choice — no MV bits at
all, no reference index bits, just a list index pointing to the same
information already on a neighbour block.

For a panning shot where every block has the same MV, merge mode
captures motion essentially for free. This is one of the biggest BD-
rate gains HEVC has over H.264 (typically 2–5% just from merge mode).

AV1 generalizes this with **NEAREST_MV**, **NEAR_MV**, **GLOBAL_MV**
modes — variations on "use a neighbour's MV with small or no delta."

### Decoder-side MV refinement (DMVR)

H.266/VVC adds a tool where the *decoder* refines the MV slightly
using local optical-flow-style analysis around the indicated position.
The encoder doesn't transmit the refinement; the decoder computes it
from already-available pixel data. This is decoder-side processing
(unusual for video codecs), and it gains another 1–2% BD-rate.

## 7.5 B-frames and bi-prediction

A **B-frame** can predict from two references — typically one past and
one future. The two predictions are then averaged (or weighted-
averaged):

```rust
prediction[x][y] = (pred_from_L0 + pred_from_L1 + 1) / 2;
```

This averaging gives B-frames their compression advantage:

1. **Smooth motion is captured more accurately** — two predictions
   bracketing the true motion give a better central estimate.
2. **Occlusions are handled gracefully** — when one reference predicts
   poorly (object newly visible or newly hidden), the other typically
   picks up the slack.

### Weighted bi-prediction

Instead of 50/50 averaging, codecs support weighted blending:

```rust
prediction[x][y] = (w0 * pred_from_L0 + w1 * pred_from_L1 + offset) >> shift;
```

Useful when one reference is more reliable (e.g., a recent reference
vs. a far-away one).

HEVC and AV1 also signal a small set of pre-defined weight pairs
(¼–¾, ⅜–⅝, etc.) per block. **Generalized Bi-prediction (GBi)** in
HEVC typically gains 0.5–1% BD-rate.

## 7.6 The decode order vs. display order divergence

B-frames must be decoded *after* both their references exist. So the
bitstream interleaves frames non-monotonically:

```text
Display order:  I  B  B  P  B  B  P  …
Frame number:   0  1  2  3  4  5  6
Decode order:   I  P  B  B  P  B  B
                ↓
Bitstream:      0  3  1  2  6  4  5
```

The container carries two timestamps per frame:
- **PTS** (presentation timestamp): when to display.
- **DTS** (decode timestamp): when to decode.

For a stream with no B-frames, PTS = DTS. For B-frames, they diverge,
and the decoder must reorder by PTS at the output stage. (More in
Chapter 12 — DPB management.)

### Hierarchical B

Modern encoders use **hierarchical B-frames** — a structured layering
where higher temporal layers reference lower ones. Example with depth
3 (8-frame mini-GOP):

```text
Decode order:    I0  P8  B4  B2  B6  B1  B3  B5  B7
Layer:           0   0   1   2   2   3   3   3   3
                                                    ↑
                                                    "throwaway" frames

Display order:   I0  B1  B2  B3  B4  B5  B6  B7  P8
```

This gives **temporal scalability**: a low-bandwidth client can drop
the highest layer (frames 1, 3, 5, 7) without breaking the decode of
lower layers. Used heavily in WebRTC SVC streams.

## 7.7 Encoder side — motion *estimation*

Estimation (encoder) is the hard part; compensation (decoder) is
trivial. For completeness, here's the encoder's job:

For each block of the current frame:
1. **Search** for the best-matching block in the reference frame.
2. **Sub-pel refinement** — refine the integer-grid match with
   ¼-pixel resolution.
3. **MV prediction** — compute the MVP from neighbours.
4. **Cost evaluation** — compute J = D + λR for each candidate MV,
   pick the minimum.

The search is the expensive part — exhaustively trying every position
within a search window. Heuristics like **diamond search**, **EPZS**,
and **hierarchical (pyramidal) search** speed this up by orders of
magnitude. See x264's `encoder/me.c` for production-quality motion
search.

**The decoder never does any of this.** It just reads the MV and
fetches.

## 7.8 What makes inter prediction complex in real code

Real inter prediction implementations are 2–5× larger than the
"conceptual" version because of:

**Block partitioning.** A 16×16 macroblock might be partitioned into
two 16×8 sub-blocks, with one MV per sub-block. Or four 8×8s. Or
eight 8×4s. The decoder reads the partition tree, then per-partition
MVs.

**Weighted prediction tables.** Implicit weighted prediction (HEVC,
AV1) requires the decoder to compute weights based on the temporal
distance to each reference. The formula is in the spec; you transcribe
it.

**Long-term references.** Some frames stay in the DPB for many seconds
(used for slowly-changing backgrounds in surveillance, for example).
Reference picture marking commands tell the decoder which frames are
short-term, long-term, or unused.

**Affine motion (HEVC/AV1).** Instead of a single (mv_x, mv_y) per
block, signal three or four control-point MVs and the decoder
interpolates a 2D affine transform across the block. Better captures
zoom, rotation, perspective. Each control-point MV is its own little
prediction problem.

**Global motion (AV1).** Per-frame parameters describing camera-wide
motion. Useful for sequences with consistent pans.

**Overlapped block motion compensation (OBMC, HEVC/AV1).** Predict
each block independently, but blend the boundaries with neighbour
predictions to soften artifacts at partition edges.

Each of these gets a few lines per chapter in the spec — but the
**core stage 4 in the pipeline** is still "given (ref, MV), fetch."

## 7.9 Decoder vs encoder workload (motion-specific)

For the **decoder**:
- Fetch a block from the reference at integer or sub-pel offset.
- Apply a 6/7/8-tap interpolation filter.
- Average with another fetch if bi-prediction.

The work is dominated by the interpolation filter. For modern
resolutions (4K, 8K), this is a substantial fraction of total decode
time — easily 20–30%. It's also embarrassingly parallel (per-block,
per-pixel), which is why every production decoder has heavily SIMD'd
motion compensation kernels.

For the **encoder**:
- Search dozens (fast) to hundreds (slow) of candidate MV positions
  per block, each requiring SAD or SATD computation.
- Rate-distortion-evaluate the top candidates.

The encoder spends 60–80% of its time in motion estimation. This is
where the "preset slow vs preset fast" speed difference comes from.

## 7.10 Where this lives in the workspace

When H.264 / HEVC / AV1 decoders are added, they'll have:

```text
crates/oximedia-codec/src/<codec>/motion.rs
  mc_luma(ref, x, y, mv, filter_type) -> [u8]
  mc_chroma(ref, x, y, mv, filter_type) -> [u8]
  apply_interpolation_filter_6tap(...) -> u8
  apply_interpolation_filter_8tap(...) -> u8
  mvp_h264_median(left, above, above_right) -> MV
  merge_candidate_list(...) -> Vec<MergeCandidate>
```

For the **production-quality reference**, study **dav1d**'s
`src/x86/mc*.c` or **x264**'s `common/mc.c`. They show the SIMD'd
filters, which are the actual inner loop of decode time.

ProRes has **no inter prediction** — purely intra — so this workspace
doesn't yet exercise this stage.

## 7.11 Further reading

- **[Wiegand2003]** §III.D — H.264 inter prediction overview.
- **[Sullivan2012]** §IV — HEVC inter prediction overview.
- **[Chen2020]** §4 — AV1 inter prediction (including OBMC, affine,
  global motion).
- **[Richardson2010]** Chapter 7 — clearest book-length treatment of
  H.264 inter, including the sub-pel filters with worked examples.
- **x264 `encoder/me.c`** — the canonical motion *estimation*
  implementation if you ever need to write one. Heavily commented.

## 7.12 Exercises

1. **By hand.** Given integer-grid samples `... 100 120 130 140 ...`
   along a row, compute the half-pel sample between 120 and 130 using
   the H.264 6-tap filter from §7.2. Use neighbours `100 120 130 140
   150 160`. Expected result: somewhere around 125 (precise value: do
   the arithmetic).

2. **MV prediction.** A block has left-neighbour MV `(2, 1)`,
   above-neighbour MV `(2, 2)`, above-right MV `(3, 1)`. What's the
   H.264 median predictor? If the actual MV is `(2, 1)`, what's the
   transmitted delta?

3. **B-frame ordering.** A stream has display sequence `I B B P B B
   P`. List the decode order. List the PTS and DTS values if the
   frame rate is 30 fps (1 unit = 1/30s) and the first frame is
   PTS=0.

4. **Bi-prediction error.** A block's true value is 100. The L0
   prediction is 95; the L1 prediction is 110. Unweighted bi-
   prediction gives what? Compare the single-reference error vs the
   bi-pred error.

5. *(Reading.)* In x264's `common/mc.c`, find `mc_luma_w16`. Don't
   try to read the SIMD; just observe how a 16×N block fetch is
   structured (typically: for each row, apply the filter
   horizontally, then vertically).

6. **Decoder-side workload.** For a 4K 30fps H.264 stream with 90%
   inter macroblocks, estimate how many 6-tap filter operations the
   decoder performs per second. (Hint: 3840×2160 / 16² blocks per
   frame; some fraction use sub-pel; each sub-pel needs both
   horizontal and vertical filter passes.)

---

Next: [Chapter 8 — Transforms](ch08-transforms.md).
