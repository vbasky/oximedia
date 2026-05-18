# Chapter 9 — Quantization

> **Engineering takeaway:** Quantization is the *only* lossy step in
> the pipeline; everything else is exactly invertible. The encoder
> divides each transform coefficient by a step size (smaller = more
> precision, more bits, higher quality; larger = the opposite). The
> decoder *de*quantizes by multiplying back, but the small coefficients
> the encoder rounded to zero stay zero — and that's where compression
> comes from. The "step size" is usually a per-frequency *matrix* (so
> high frequencies get throw-away treatment, low frequencies are
> preserved) scaled by a *quantization parameter* (QP) that the
> encoder picks per slice or per block.

In Chapter 8 you saw the encoder produce sparse transform coefficients
— most near zero. Quantization is what makes them *exactly* zero, and
that's where the entropy coder finally gets a payoff: a sparse vector
of small integers, mostly zero, compresses much better than a dense
vector of floats.

This chapter covers what quantization does, how QP and quantization
matrices interact, and what the decoder must do to dequantize
correctly.

## 9.1 What quantization is

A quantizer divides each coefficient by a step size, rounds to the
nearest integer, and stores the integer:

```rust
quantized = round(coefficient / step);
```

The decoder's dequantizer multiplies back:

```rust
dequantized = quantized * step;
```

After this round-trip, `dequantized` ≈ `coefficient`, but not exactly:
the rounding lost a value in the range `(-step/2, step/2]`. For
coefficients that were already smaller than `step/2`, the round goes
to zero, and the original information is *gone*.

That gone information is exactly what compression buys you. The
encoder chose `step` to be large enough that small coefficients get
zeroed (saving bits in entropy coding) but small enough that important
coefficients survive (preserving quality).

## 9.2 The quantization parameter (QP)

Codecs expose a single integer — **QP** — as the user-facing
quality dial. Higher QP = larger step size = more loss = fewer bits.
Lower QP = smaller step size = less loss = more bits.

The exact mapping from QP to step size is codec-specific:

### H.264 / HEVC: QP doubles step size every 6 increments

```text
step_size(QP) = base_step × 2^((QP - 4) / 6)
```

So:
- QP 4 → step 1 (essentially lossless)
- QP 10 → step 2
- QP 16 → step 4
- QP 22 → step 8
- QP 28 → step 16
- QP 34 → step 32

The "QP doubles every 6" mapping means each 6-QP increase
*approximately* doubles the rate and halves the PSNR step. This
matches the "6 dB per bit" rate-distortion rule of thumb you'd see in
Chapter 23.

### AV1: similar shape, different specific table

AV1 uses a different table mapping QP to step size, but the
exponential shape is the same.

### The implementer's view

You don't derive these mappings. You transcribe them from the spec.

For H.264:
```rust
fn step_size_h264(qp: u8) -> u16 {
    static QSTEP: [u16; 6] = [10, 11, 13, 14, 16, 18];
    let base = QSTEP[(qp % 6) as usize];
    let shift = qp / 6;
    base << shift
}
```

That table — `[10, 11, 13, 14, 16, 18]` — is six base step values for
the first six QPs. From there, every 6 increments shifts the value
left by 1 (doubling). The table comes from the spec; you copy it
verbatim.

## 9.3 Quantization matrices

The above describes *scalar* quantization (one step size for all
coefficients in a block). Real codecs use **matrix quantization**:
different coefficients get different step sizes. The matrix shape is
designed for **perceptual quantization** — preserve precision where
the eye cares (low frequencies), throw it away where the eye doesn't
(high frequencies).

Example H.264 default intra 8×8 luma matrix:

```text
   6 10 13 16 18 23 25 27
  10 11 16 18 23 25 27 29
  13 16 18 23 25 27 29 31
  16 18 23 25 27 29 31 33
  18 23 25 27 29 31 33 36
  23 25 27 29 31 33 36 38
  25 27 29 31 33 36 38 40
  27 29 31 33 36 38 40 42
```

Read top-to-bottom and left-to-right, value increases:
- Top-left (DC): 6 — small step, preserve precision.
- Bottom-right (high frequency): 42 — large step, throw it away.

The decoder dequantizes by:

```rust
dequantized[i][j] = quantized[i][j] * matrix[i][j] * scale(QP);
```

Two scalings: the matrix shapes the noise, QP scales overall.

### Matrix signaling

Codecs let the encoder either:
- **Use the default matrices** (small bit cost; signal just an index).
- **Send custom matrices** in the PPS / sequence header — bigger
  upfront cost, can be tuned to specific content.

Most streaming uses default matrices. Cinema and broadcast pipelines
sometimes use custom matrices tuned for their content type and
display environment.

### HEVC and AV1 matrices

HEVC has separate matrices for each transform size (4×4 through
32×32) and each combination of (intra/inter, luma/chroma). AV1 takes
a different approach: a small set of QP-based scaling tables with
fewer per-frequency variations.

For the decoder: you implement the lookup; the spec specifies which
matrix applies to which (transform size, prediction type, luma/
chroma).

## 9.4 Dead-zone quantization

Pure scalar quantization (`round(x / step)`) has a dead-zone of `±step/2`
around zero — any coefficient in that range becomes zero. **Dead-zone
quantization** widens this:

```rust
quantized = if coefficient > 0 {
    floor((coefficient - dead_zone) / step + 0.5)
} else if coefficient < 0 {
    ceil((coefficient + dead_zone) / step - 0.5)
} else {
    0
};
```

The dead-zone is typically `step/3` instead of `step/2`. More small
coefficients round to zero, which means fewer entropy-coded coefficients,
which means fewer bits — at a small quality cost for the coefficients
that were borderline.

H.264 has implicit dead-zone behavior via specific encoder-side
adjustments (the spec is decoder-only, so the dead-zone is an encoder
optimization — the decoder just dequantizes with the standard formula).

## 9.5 Quantization in the pipeline (decoder side)

For each block at stage 6 of the pipeline:

1. Receive quantized coefficients from stage 5 (entropy decoded).
2. Look up the appropriate quantization matrix (based on transform
   size, intra/inter, luma/chroma).
3. Get the slice's QP (from the header), plus any per-block QP delta.
4. Compute `step_size` from QP using the codec's mapping.
5. For each coefficient: `dequantized = quantized × matrix × step / scale`.
   (The exact integer math is codec-specific; see the spec.)

In code:

```rust
fn dequantize_h264(coeffs: &mut [[i32; 8]; 8], qp: u8,
                   matrix: &[[u8; 8]; 8]) {
    let step = step_size_h264(qp);
    let shift = appropriate_shift_h264(qp);
    for i in 0..8 {
        for j in 0..8 {
            let scaled = coeffs[i][j] * (matrix[i][j] as i32) * (step as i32);
            coeffs[i][j] = (scaled + (1 << (shift - 1))) >> shift;
        }
    }
}
```

The shift is part of bit-exact precision management. Real codec specs
specify the exact integer math sequence.

## 9.6 Per-block QP delta

The slice header specifies a *base* QP, but the encoder can adjust QP
on a per-block basis using a `qp_delta` field in the block's side
info. Why?

- **Rate control granularity.** In a busy scene (lots of bits needed
  for some areas), the encoder can lower QP on detail-heavy blocks
  and raise it on flat blocks, balancing the total rate.
- **Adaptive quantization.** Encoders that implement
  psychovisually-aware AQ (x264's `--aq-mode`) lower QP in dark areas
  (where banding is visible) and raise it in busy areas (where the
  detail masks quantization noise).

The decoder just reads the delta, computes effective QP for the block,
and dequantizes accordingly. From the decoder's view, AQ is invisible
machinery.

## 9.7 HDR and quantization

In Chapter 4 you saw that HDR uses a perceptually-tuned transfer
function (PQ or HLG). This interacts with quantization in a subtle
way:

- For HDR, the quantization step in **code-value space** is not
  uniform in linear light. PQ has small linear steps in shadow
  (where the eye notices error) and large linear steps in highlights
  (where it doesn't).
- This is *exactly the right shape* for quantization to work well at
  HDR's wider luminance range.
- So HDR content uses similar bitrates as SDR at similar perceptual
  quality, even though it carries 100× the luminance range.

There's no special "HDR quantization" — the codec just runs its normal
quantizer over PQ-encoded values. The TF is doing the heavy lifting.

That said, encoders may use HDR-aware *matrices* that compensate for
the TF's varying perceptual sensitivity. Whether to do so is an
encoder tuning choice, not part of the bitstream.

## 9.8 What makes real quantization complex

Real implementations have ~3× the line count of the basic version
because of:

**Per-block QP delta decoding.** The qp_delta is exp-Golomb coded with
specific clipping rules.

**Chroma QP offsets.** A separate QP offset for chroma vs luma,
signaled in the PPS.

**Different matrices per (block size, prediction type, plane).** HEVC
has 16+ separate matrices.

**Transform skip handling.** Blocks signaled "transform skip" get a
slightly different quantization treatment.

**Per-transform-type matrices in AV1.** Different transform types (DCT,
ADST, identity) have different step-size scaling.

The basic dequantize function is 10 lines; production code handles all
the special cases and ends up at 200–500 lines.

## 9.9 Decoder vs encoder workload

**Decoder:** Cheap. A single multiply and shift per coefficient. No
SIMD pressure beyond what naturally falls out of vectorizing the
inner loop.

**Encoder:** The forward quantization is similarly cheap. The expensive
part is **trying different QPs**. The rate-distortion-optimal
quantizer for a given QP is straightforward; the rate-distortion-
optimal *QP* requires evaluating multiple candidates, plus considering
how the choice interacts with prediction and motion. See Chapter 13
for rate control mechanics.

## 9.10 Where this lives in the workspace

ProRes has its own [quantization
module](../../crates/oximedia-codec/src/prores/quant.rs). ProRes uses
matrix-based dequantization with a slice-QP scale factor. Read it
alongside SMPTE RDD 36 §C.5.

For other codecs (when added):

```text
crates/oximedia-codec/src/h264/quant.rs
  step_size(qp)                       -> u16
  dequantize_4x4(coeffs, qp, matrix)  -> [[i16; 4]; 4]
  ...
```

The dequantization functions are usually 20–100 lines each.

## 9.11 Further reading

- **[Wiegand2003]** §III.F — H.264 quantization, including the QP-to-
  step mapping derivation.
- **[Sullivan2012]** §VI — HEVC quantization, including matrix
  signaling.
- **[Sayood2017]** Chapter 9 — quantization theory, including
  dead-zone and uniform/non-uniform tradeoffs.
- **[Wiegand2003]** Appendix — the H.264 specific dequantization
  formula (it's a worked-out tutorial).

For an encoder's view of quantization (where the interesting choices
happen), x264's `encoder/rdo.c` and the discussion of "trellis
quantization" in [DarkShikari]'s blog are the canonical resources.

## 9.12 Exercises

1. **By hand.** A coefficient is 50. The step size is 16. What's the
   quantized value? When the decoder dequantizes, what's the
   reconstructed value? What was the loss?

2. **Sweep.** For the H.264 step-size table `[10, 11, 13, 14, 16,
   18]`, compute step sizes for QP = 0 through QP = 51. Plot
   (log-linear) and confirm it doubles every 6 increments.

3. **Matrix interpretation.** Pick a quantization matrix from §9.3.
   What's the ratio of step sizes for the highest-frequency to lowest-
   frequency coefficient? In other words, how much *more* loss does
   the encoder permit at high frequencies?

4. **Bit savings.** Suppose a block has 64 coefficients. Without
   quantization (lossless), they average 8 bits each → 512 bits per
   block. After quantization with the matrix above, 50 of the 64
   coefficients round to zero. Estimate the new bit cost (a single
   EOB token for the trailing 50 plus ~6 bits each for the surviving
   14 = ~85 bits). What's the compression ratio for this block alone?

5. *(Reading.)* In
   [`crates/oximedia-codec/src/prores/quant.rs`](../../crates/oximedia-codec/src/prores/),
   identify the dequantization formula. Compare to §9.5 — what's
   ProRes-specific vs. the generic shape?

6. **Conceptual:** Why is the quantization matrix smallest in the
   top-left and largest in the bottom-right? (Hint: think about
   what frequencies the eye is most sensitive to, and how those
   map to coefficient positions.)

---

Next: [Chapter 10 — Entropy Coding](ch10-entropy.md).
