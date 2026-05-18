# Chapter 6 — Intra Prediction

> **Engineering takeaway:** Intra prediction "guesses" what a block
> looks like using already-decoded *neighbouring* pixels (top row, left
> column, sometimes top-left and top-right). The encoder picks the
> guess that's closest to the original, sends the *index* of the mode,
> and only the difference (residual) needs to be coded. A decoder reads
> the mode index and reproduces the same guess — that's the whole
> stage. Mode counts grow over time (H.264: 9, HEVC: 35, AV1: 56+),
> but each individual mode is a simple per-block pixel-copy formula.

In the encoder's view: "I just decoded the blocks above and to the
left of this one. Is there a simple combination of their pixels that
predicts this block well enough that I only need to transmit the small
correction?" That's the whole motivation for intra prediction. The
decoder side is even simpler — given the mode index, **apply the
formula**, and you have a prediction block.

This chapter covers what the formulas look like, why so many of them
exist, and how to read mode tables in real specs.

## 6.1 The neighbourhood

Every intra mode predicts an N×N block from some subset of these
pixels:

```text
   ┌─────────────────┬─────────────────┬─────────────────┐
   │  top-left       │   above row     │  above-right    │
   │  pixel "P"      │ (N pixels)      │  (often N more) │
   ├─────────────────┼─────────────────┼─────────────────┤
   │  left column    │                 │                 │
   │  (N pixels)     │   current block │                 │
   │                 │   (predict this)│                 │
   │                 │                 │                 │
   └─────────────────┴─────────────────┴─────────────────┘
```

The decoder has all of those already — they were decoded in earlier
blocks of this slice. (For the topmost row or leftmost column of a
slice, some neighbours don't exist; codecs handle that with default
fill values or specific edge modes.)

For an 8×8 block, "left column" is 8 pixels, "above row" is 8 pixels,
and the modes combine these in different patterns.

## 6.2 H.264 — 9 modes for 4×4 luma

The simplest case. H.264 specifies 9 intra-4×4 modes for luma. Drawn
on a 4×4 grid (where `?` is the pixel being predicted, `A`–`D` is the
top row, `I`–`L` is the left column):

```text
         A B C D
       Q ? ? ? ?
       I ? ? ? ?
       J ? ? ? ?
       K ? ? ? ?
       L ? ? ? ?
```

(Q is the top-left corner pixel, treated as a "context" for some
modes.)

The nine modes:

| Mode | Name           | Formula (per pixel)                                  |
|------|----------------|------------------------------------------------------|
| 0    | Vertical       | predict each column by repeating the top sample      |
| 1    | Horizontal     | predict each row by repeating the left sample        |
| 2    | DC             | use the average of available neighbours              |
| 3    | Diagonal Down-Left | extrapolate top-right diagonally down-left      |
| 4    | Diagonal Down-Right | extrapolate top-left diagonally down-right     |
| 5    | Vertical-Right | mostly vertical, slight rightward tilt               |
| 6    | Horizontal-Down| mostly horizontal, slight downward tilt              |
| 7    | Vertical-Left  | mostly vertical, leftward tilt                       |
| 8    | Horizontal-Up  | mostly horizontal, upward tilt                       |

### Worked example: Vertical (mode 0)

```text
   A B C D     ← these are the four top neighbours
   ? ? ? ?
   ? ? ? ?
   ? ? ? ?
   ? ? ? ?

After mode 0 prediction:

   A B C D
   A B C D
   A B C D
   A B C D
   A B C D
```

Every row is filled with the four top samples. If the actual block is
a vertical stripe pattern, this prediction is near-perfect and the
residual is mostly zero.

### Worked example: DC (mode 2)

The DC prediction uses the average of the eight available neighbours
(A–D plus I–L), with each pixel of the 4×4 block set to that scalar:

```text
   A B C D
 I  3  3  3  3   (where 3 = (A+B+C+D+I+J+K+L+4)/8)
 J  3  3  3  3
 K  3  3  3  3
 L  3  3  3  3
```

This is the predictor of choice for smooth blocks where there's no
strong directional structure.

### Mode coding

Mode indices are 4 bits each, except for the special case "most
probable mode": the decoder checks if the predicted mode equals the
mode of the block above or to the left; if so, a 1-bit flag suffices.
This exploits the spatial correlation of mode decisions — adjacent
blocks tend to choose similar prediction directions.

For the H.264 4×4 case:

- 1 bit signals "use most probable mode"
- If 0, 3 more bits give the explicit mode index (0–8 with one
  exclusion)

This compact mode coding is why H.264 can afford 9 modes per block
without spending too many bits per macroblock.

### H.264 16×16 intra

For larger blocks (16×16 macroblocks), H.264 has only **4 intra
modes**: vertical, horizontal, DC, and Plane (a planar prediction that
fits a 2D plane through the corner samples). Larger blocks need fewer
modes because the residual still gets the fine-grained transform
treatment.

### H.264 intra-8×8 (High Profile only)

The High Profile adds 8×8 intra prediction with the same 9-mode set
as 4×4. Used when the block has clear directional structure at a
larger scale than 4×4.

## 6.3 HEVC — 35 modes for variable block size

HEVC generalizes H.264's intra prediction substantially:

- **Block sizes 4×4 through 32×32** (with intra prediction applied
  at each size via quad-tree partitioning of the CTU).
- **33 directional modes + DC + Planar = 35 total** per block.
- Same neighbourhood structure (top row, left column, top-left, top-
  right).

The 33 directional modes correspond to **specific angles** sweeping
from vertical through diagonal to horizontal:

```text
       Mode  Angle
       18    vertical (top → bottom)
       19    very slight tilt right of vertical
       20    slight tilt right
       ...
       26    diagonal (top-right corner extrapolation)
       ...
       34    horizontal (left → right)
```

A pixel in the block is predicted as a weighted blend of two reference
samples along the chosen direction. Each angle has specific fractional
weights specified in the standard. For implementation, the directional
predictor is essentially:

```rust
for each pixel (x, y) in the block:
    // Find the reference position along the angle.
    let ref_pos = compute_intersection(x, y, mode_angle);
    // Linear-interpolate between two adjacent reference samples.
    pred[y][x] = (1 - frac) * ref[floor(ref_pos)]
               +     frac   * ref[floor(ref_pos) + 1];
```

The angle-to-fractional-weight tables are in the HEVC spec
(`intraPredModeY` and the `invAngle` table). Don't derive them; copy
them.

### HEVC planar mode

Planar fits a 2D plane through the block's neighbours so the
prediction smoothly interpolates between the top row and the left
column. Used for smooth gradients.

### Most-probable-mode (MPM) extended

HEVC uses 3 MPM candidates from neighbours instead of H.264's 2.
3 mode comparisons → only need to transmit the MPM index (a few bits)
in the common case. When the actual mode isn't in the MPM set, a
5-bit explicit mode is sent.

## 6.4 AV1 — 56+ modes including special cases

AV1 keeps growing the mode list. The basics:

- **8 directional modes**: V, H, D45, D135, D117, D153, D207, D63
  (an irregular subset of angles, chosen for content-relevant
  directions).
- **Pure non-directional**: DC, Paeth, Smooth, Smooth_V, Smooth_H.
- **Recursive intra**: predict a sub-block from already-predicted
  parts of the same parent.
- **Chroma from luma (CfL)**: predict chroma as a linear function of
  the just-decoded luma in this block. Surprisingly effective for
  natural content where chroma correlates with luma intensity.
- **Palette mode**: for screen content with few unique colors,
  transmit a per-block palette and per-pixel indices.
- **Intra block copy (IBC)**: like motion compensation, but the
  reference is *within the current frame*. Great for screen content
  with repeating elements (text, icons, UI patches).

AV1's directional intra also has **angle deltas** — fine-tuning the
prediction angle ±3 by 3° increments around each of the 8 base
directions, giving 56 directional modes total. The encoder picks one;
the decoder reads it.

### Paeth predictor (worth knowing)

Paeth is a non-directional mode that chooses, for each pixel, the one
of `(top, left, top-left)` neighbours that minimizes a local gradient
estimate:

```rust
fn paeth_predictor(top: i32, left: i32, top_left: i32) -> i32 {
    let p = top + left - top_left;
    let pa = (p - top).abs();
    let pb = (p - left).abs();
    let pc = (p - top_left).abs();
    if pa <= pb && pa <= pc { top }
    else if pb <= pc       { left }
    else                   { top_left }
}
```

It's borrowed from PNG, where it's the smoothest predictor for
non-directional content. AV1 found it useful.

### Smooth modes

`Smooth_V`, `Smooth_H`, and `Smooth` linearly interpolate between
top-row and left-column samples. They handle gradient-shaded blocks
that don't fit a strict directional model. (HEVC's planar mode is the
ancestor of these.)

## 6.5 Chroma intra prediction

For 4:2:0 content, chroma blocks are smaller than luma (½ in each
dimension) and have their own intra modes. H.264 / HEVC have a small
set of chroma intra modes (DC, V, H, Plane) reused from the luma
infrastructure. AV1 has **Chroma from Luma (CfL)** as a key mode: the
chroma block is predicted as α × luma + β, where α (and sometimes β)
are signaled per-block. The encoder figures out the right linear fit;
the decoder applies it.

CfL gains 1–3% BD-rate on natural content because chroma in YUV is
correlated with luma at the same position.

## 6.6 What makes intra prediction harder than it looks

A few practical implementation notes:

**Boundary handling.** At slice / tile / picture boundaries, some
neighbours are unavailable. The standard specifies *constrained intra
prediction* modes that allow the encoder to disallow specific cross-
boundary references, useful for tiling and parallelism. The decoder
must obey the constraints in the bitstream.

**Reference sample smoothing.** Some HEVC modes apply a low-pass
filter to the neighbour samples before using them. The filter
parameters are specified per-mode and per-block-size. This is one of
the small spec tables you transcribe directly.

**Mode-dependent intra smoothing (MDIS).** Whether to apply the
reference smoothing depends on the mode and block size. The spec
table tells you exactly when.

**Boundary filtering after intra prediction (AV1).** AV1 applies a
filter to the *predicted* pixels at the block boundary (different
from the loop deblocking filter). This is part of the intra
prediction step, not deblocking.

These details make intra prediction code in real codecs 2× larger
than a naïve implementation. The mode formulas themselves are small;
the per-mode, per-size, per-context smoothing/filtering choices are
where the line count goes.

## 6.7 Decoder vs encoder workload

For the **decoder**: receive a mode index, apply the formula, that's
it. The per-block work is microseconds at most. Intra prediction is
not a decoder hotspot.

For the **encoder**: try many modes per block (with rate-distortion
optimization for each candidate), pick the best, signal the index.
Intra mode selection is one of the most expensive encoder stages, and
the choice of which modes to *consider* is one of the biggest knobs
in encoder speed presets. Fast presets check only a handful of modes;
slow presets exhaustively check all 56.

## 6.8 Where this lives in the workspace

This workspace's ProRes decoder *doesn't have intra prediction* —
ProRes uses pure transform coding without spatial prediction.

The H.264 decoder, when added, will have an `intra.rs` module with
roughly:

```text
crates/oximedia-codec/src/h264/intra.rs
  intra_4x4_pred(mode, neighbours)  -> [[u8; 4]; 4]
  intra_8x8_pred(mode, neighbours)  -> [[u8; 8]; 8]
  intra_16x16_pred(mode, neighbours) -> [[u8; 16]; 16]
  chroma_intra_pred(mode, neighbours) -> [[u8; 8]; 8]
```

Each function dispatches on `mode` to one of 9 (or 35, etc.) per-mode
formulas. The shape is the same across codecs; only the per-mode
formulas vary.

A great place to study a real intra prediction implementation is
**dav1d's** `src/x86/ipred*.c` or the Rust-translated rav1e's
`src/predict.rs`. The base case is straightforward; the SIMD versions
are templates over block sizes and modes.

## 6.9 Further reading

- **[Wiegand2003]** §III.B — H.264 intra prediction overview.
- **[Sullivan2012]** §III — HEVC intra prediction overview.
- **[Chen2020]** §3.1–3.3 — AV1 intra prediction.
- **[Richardson2010]** Chapter 6 — the most readable book-length
  treatment of H.264 intra.

For dav1d / rav1e's intra implementation, the source files are
self-documenting once you've internalized this chapter's mode list.

## 6.10 Exercises

1. **By hand.** Apply H.264 mode 0 (vertical) and mode 2 (DC) to a
   synthetic 4×4 block where neighbours `A`–`D` = `100 150 200 250`
   and `I`–`L` = `100 100 100 100`. Compute both predicted blocks
   manually. Which is more useful for a vertical-stripe original?

2. **HEVC angles.** Mode 26 in HEVC is the "diagonal" direction.
   Sketch which neighbour sample (top, left, top-right, etc.) drives
   the bottom-left pixel of an 8×8 block.

3. **AV1 Paeth.** Implement `paeth_predictor` from §6.4 in Rust.
   Apply it to a 2×2 block with `top = 100, left = 200, top_left =
   150`. What does it predict?

4. *(Reading.)* Open dav1d's `src/x86/ipred.h` and find the
   declaration for any directional intra predictor. Don't try to read
   the SIMD; just observe how the function signature reflects the
   neighbourhood (you'll see parameters for top, left, and possibly
   top-right and top-left).

5. **Mode coding.** The H.264 MPM scheme saves bits when the actual
   mode equals one of the predicted modes. If 70% of blocks hit the
   MPM, how many bits per block does the encoder save vs. always
   sending the 4-bit explicit mode? (Easy arithmetic, but worth
   doing — this is where coding gains hide.)

6. **Conceptual.** Why doesn't intra prediction also use the *below*
   and *right* neighbours? (Hint: at decode time, which neighbours
   are already decoded?)

---

Next: [Chapter 7 — Motion Estimation and Compensation](ch07-motion.md).
