# Appendix B — A Complete 4×4 IDCT Walkthrough

> Chapter 8 described the inverse transform conceptually. This
> appendix walks through H.264's 4×4 integer inverse transform on a
> concrete numerical example. Same shape as the real spec — actual
> integer arithmetic, actual shifts, no floating point. By the end
> you should be able to verify the math against the H.264 spec by
> hand.

H.264's 4×4 inverse transform turns 16 dequantized coefficients into
16 pixel-domain residuals. It's specified in §8.5.10 of H.264 with
specific integer math — bit-exact, no floats.

This appendix uses one example coefficient block and traces it
through the row and column passes.

## B.1 The transform matrix

H.264's 4×4 inverse transform uses the matrix:

```text
       ⎡ 1   1   1   1  ⎤
   T = ⎢ 1  1/2 -1/2 -1 ⎥
       ⎢ 1  -1  -1   1  ⎥
       ⎣ 1/2 -1   1  -1/2 ⎦
```

In integer math, the 1/2 values are stored as 1 but with appropriate
shifts. The inverse is implemented with adds, subtracts, and shifts
only — no multiplications by anything other than powers of two.

The 2D inverse transform decomposes:

```text
residual = T^T · (T · coeffs · T^T) · T

         = (T^T · X) · T          where X = T · coeffs · T^T
```

In code, we do two 1D transforms:

1. **Row pass**: for each row of the input, apply the 1D inverse
   transform.
2. **Column pass**: for each column of the intermediate, apply the
   1D inverse transform.

Each 1D inverse is a fixed pattern of adds and shifts on 4 input
values. We'll walk through it.

## B.2 The example coefficient block

Suppose after dequantization the coefficient block is:

```text
   ⎡ 200  60  -40  10 ⎤
   ⎢  20  10  -20   0 ⎥
   ⎢ -10   0   10   5 ⎥
   ⎣   0  -5    0   0 ⎦
```

These are signed integers, in the range typical after dequant with a
moderate QP. Most coefficients are zero or small; the DC (200) and
first few AC coefficients carry most of the energy.

We expect, after the inverse transform, a pixel residual block that
when added to the prediction reconstructs the original. The residual
values should be in a range like [-128, 127] for 8-bit content.

## B.3 The 1D inverse transform

For a 4-element row `[d_0, d_1, d_2, d_3]`, the 1D inverse:

```text
Step 1 - Even terms:
  e_0 = d_0 + d_2          (sum of evens)
  e_1 = d_0 - d_2          (difference of evens)

Step 2 - Odd terms (with shifts to handle the fractional rows):
  e_2 = (d_1 >> 1) - d_3   (half d_1 minus d_3)
  e_3 = d_1 + (d_3 >> 1)   (d_1 plus half d_3)

Step 3 - Combine:
  out_0 = e_0 + e_3
  out_1 = e_1 + e_2
  out_2 = e_1 - e_2
  out_3 = e_0 - e_3
```

This is one of the equivalent integer factorizations of the IDCT. The
shifts implement the fractional rows of T with integer arithmetic.

## B.4 Row pass

Apply the 1D inverse to each row of our example. Let's do row 0:

```text
Row 0: [200, 60, -40, 10]

Step 1 - Evens:
  e_0 = d_0 + d_2 = 200 + (-40) = 160
  e_1 = d_0 - d_2 = 200 - (-40) = 240

Step 2 - Odds:
  e_2 = (d_1 >> 1) - d_3 = (60 >> 1) - 10 = 30 - 10 = 20
  e_3 = d_1 + (d_3 >> 1) = 60 + (10 >> 1) = 60 + 5 = 65

Step 3 - Combine:
  out_0 = e_0 + e_3 = 160 + 65 = 225
  out_1 = e_1 + e_2 = 240 + 20 = 260
  out_2 = e_1 - e_2 = 240 - 20 = 220
  out_3 = e_0 - e_3 = 160 - 65 = 95

Row 0 after IDCT: [225, 260, 220, 95]
```

Now row 1:

```text
Row 1: [20, 10, -20, 0]

Step 1:
  e_0 = 20 + (-20) = 0
  e_1 = 20 - (-20) = 40

Step 2:
  e_2 = (10 >> 1) - 0 = 5
  e_3 = 10 + (0 >> 1) = 10

Step 3:
  out_0 = 0 + 10 = 10
  out_1 = 40 + 5 = 45
  out_2 = 40 - 5 = 35
  out_3 = 0 - 10 = -10

Row 1 after IDCT: [10, 45, 35, -10]
```

Row 2:

```text
Row 2: [-10, 0, 10, 5]

Step 1:
  e_0 = -10 + 10 = 0
  e_1 = -10 - 10 = -20

Step 2:
  e_2 = (0 >> 1) - 5 = -5
  e_3 = 0 + (5 >> 1) = 2     (integer shift: 5 >> 1 = 2)

Step 3:
  out_0 = 0 + 2 = 2
  out_1 = -20 + (-5) = -25
  out_2 = -20 - (-5) = -15
  out_3 = 0 - 2 = -2

Row 2 after IDCT: [2, -25, -15, -2]
```

Row 3:

```text
Row 3: [0, -5, 0, 0]

Step 1:
  e_0 = 0 + 0 = 0
  e_1 = 0 - 0 = 0

Step 2:
  e_2 = (-5 >> 1) - 0 = -2    (5 >> 1 = 2; -5 >> 1 in two's
                                complement is -3 with floor, but
                                arithmetic shift gives -3; here we
                                use truncated -2)
  e_3 = -5 + (0 >> 1) = -5

Step 3:
  out_0 = 0 + (-5) = -5
  out_1 = 0 + (-2) = -2
  out_2 = 0 - (-2) = 2
  out_3 = 0 - (-5) = 5

Row 3 after IDCT: [-5, -2, 2, 5]
```

After row pass:

```text
   ⎡ 225  260  220   95 ⎤
   ⎢  10   45   35  -10 ⎥
   ⎢   2  -25  -15   -2 ⎥
   ⎣  -5   -2    2    5 ⎦
```

## B.5 Column pass

Now apply the 1D inverse to each *column* of the intermediate. Treat
each column as a 4-vector.

Column 0: `[225, 10, 2, -5]`

```text
Step 1:
  e_0 = 225 + 2 = 227
  e_1 = 225 - 2 = 223

Step 2:
  e_2 = (10 >> 1) - (-5) = 5 + 5 = 10
  e_3 = 10 + (-5 >> 1) = 10 + (-3) = 7

Step 3:
  out_0 = 227 + 7 = 234
  out_1 = 223 + 10 = 233
  out_2 = 223 - 10 = 213
  out_3 = 227 - 7 = 220

Column 0 after IDCT: [234, 233, 213, 220]
```

Column 1: `[260, 45, -25, -2]`

```text
Step 1:
  e_0 = 260 + (-25) = 235
  e_1 = 260 - (-25) = 285

Step 2:
  e_2 = (45 >> 1) - (-2) = 22 + 2 = 24
  e_3 = 45 + (-2 >> 1) = 45 + (-1) = 44

Step 3:
  out_0 = 235 + 44 = 279
  out_1 = 285 + 24 = 309
  out_2 = 285 - 24 = 261
  out_3 = 235 - 44 = 191

Column 1 after IDCT: [279, 309, 261, 191]
```

Column 2: `[220, 35, -15, 2]`

```text
Step 1:
  e_0 = 220 + (-15) = 205
  e_1 = 220 - (-15) = 235

Step 2:
  e_2 = (35 >> 1) - 2 = 17 - 2 = 15
  e_3 = 35 + (2 >> 1) = 35 + 1 = 36

Step 3:
  out_0 = 205 + 36 = 241
  out_1 = 235 + 15 = 250
  out_2 = 235 - 15 = 220
  out_3 = 205 - 36 = 169

Column 2 after IDCT: [241, 250, 220, 169]
```

Column 3: `[95, -10, -2, 5]`

```text
Step 1:
  e_0 = 95 + (-2) = 93
  e_1 = 95 - (-2) = 97

Step 2:
  e_2 = (-10 >> 1) - 5 = -5 - 5 = -10
  e_3 = -10 + (5 >> 1) = -10 + 2 = -8

Step 3:
  out_0 = 93 + (-8) = 85
  out_1 = 97 + (-10) = 87
  out_2 = 97 - (-10) = 107
  out_3 = 93 - (-8) = 101

Column 3 after IDCT: [85, 87, 107, 101]
```

## B.6 Combined intermediate

After row pass + column pass, the intermediate result is:

```text
   ⎡ 234  279  241   85 ⎤
   ⎢ 233  309  250   87 ⎥
   ⎢ 213  261  220  107 ⎥
   ⎣ 220  191  169  101 ⎦
```

## B.7 Final scaling and rounding

The H.264 spec specifies a final rounding shift to convert the
intermediate (scaled) values back to pixel-domain residuals:

```text
residual[i][j] = (intermediate[i][j] + 32) >> 6
```

The `+ 32` is round-to-nearest; the `>> 6` is the inverse of the
scale factor built into the transform.

Applying:

```text
   ⎡ (234+32)>>6 = 266>>6 = 4   ⎤
   ⎢ (279+32)>>6 = 311>>6 = 4   ⎥
   ⎢ (241+32)>>6 = 273>>6 = 4   ⎥
   ⎣ ( 85+32)>>6 = 117>>6 = 1   ⎦
   ⎡ (233+32)>>6 = 265>>6 = 4   ⎤
   ⎢ (309+32)>>6 = 341>>6 = 5   ⎥
   ...
```

Working through the whole matrix:

```text
   ⎡  4   4   4   1 ⎤
   ⎢  4   5   4   1 ⎥
   ⎢  3   4   3   2 ⎥
   ⎣  3   3   3   2 ⎦
```

These are the **pixel-domain residuals**. They'll be added to the
prediction block to produce the final reconstructed pixels.

## B.8 What this teaches you

After tracing this:

1. **The 1D inverse is small.** It's 4 lines of code per pass — adds
   and shifts. No multiplications.

2. **Order matters.** Row pass first, then column pass on the
   intermediate. Doing it the other way gives identical results
   (transform separability) but you must commit to one order for
   bit-exact reproducibility.

3. **Intermediate values can be large.** Our example intermediate had
   values up to 309. The final shift compresses them back to pixel
   range. Make sure your intermediate type is wide enough (16-bit at
   minimum for H.264 4×4; 32-bit for HEVC larger transforms).

4. **Rounding matters.** The `+ 32` before the `>> 6` is round-to-
   nearest. Forget it and you get a small systematic bias that
   compounds across frames.

## B.9 SIMD considerations

A SIMD'd version processes multiple pixels per instruction:

```text
// Row pass with SSE2 (16-bit lanes, 8 elements per register):
__m128i row = _mm_load_si128(coeffs);
__m128i e_0_e_1 = ...    // even sum + diff
__m128i e_2_e_3 = ...    // odd terms with shift
__m128i result = ...     // combine
```

The 4×4 IDCT for 4 rows in parallel fits exactly in one SSE2 register
(16-bit × 8 = 128 bits = 4 rows × 4 cols). dav1d's AV1 IDCT
implementations are the gold-standard reference for this kind of
vectorization.

## B.10 Where this lives in the workspace

The ProRes IDCT is in
[`crates/oximedia-codec/src/prores/idct.rs`](../../crates/oximedia-codec/src/prores/).
It's a 8×8 integer IDCT specified by SMPTE RDD 36 — different from
H.264's 4×4 (different matrix, different scaling), but same structural
approach.

H.264's 4×4 IDCT, when added, will live in:

```text
crates/oximedia-codec/src/h264/idct.rs
  inverse_4x4(coeffs: &[[i16; 4]; 4]) -> [[i16; 4]; 4]
  inverse_8x8(coeffs: &[[i16; 8]; 8]) -> [[i16; 8]; 8]
```

Each function is ~50 lines of scalar code, with optional SIMD variants
dispatched via the workspace's runtime feature detection.

## B.11 Further reading

- **[Wiegand2003]** §III.E — H.264 transform, with the matrix
  derivation and integer arithmetic details.
- **H.264 spec §8.5.10** — the exact procedure to follow.
- **[Sayood2017]** Chapter 12 — DCT and integer transforms from first
  principles.
- **dav1d source** — `src/x86/itx*.asm` for production-quality SIMD
  transforms.

This appendix is the worked counterpart to Chapter 8. Together they
should be enough to implement bit-exact integer transforms.
