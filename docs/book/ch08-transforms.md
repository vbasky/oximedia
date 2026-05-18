# Chapter 8 — Transforms

> **Engineering takeaway:** A transform is a basis change that takes
> spatial pixels and produces frequency coefficients. The point is
> *energy compaction*: in natural images, transform coefficients are
> mostly near zero — making them cheap to entropy-code after
> quantization throws the small ones away. The DCT is the workhorse;
> modern codecs use integer approximations of it (for bit-exactness)
> across multiple block sizes (4×4 to 64×64) and sometimes types (DCT,
> ADST, identity). As a decoder implementer, you call the **inverse**
> transform — a specified integer matrix multiply — and don't need to
> derive the math.

The whole point of the transform stage is to make subsequent
quantization and entropy coding *work better*. Pixels are correlated
in the spatial domain (neighbouring pixels are similar). Frequency
coefficients of natural content are *decorrelated* and *sparse*
(most are near zero). Quantize the sparse representation, RLE the
zeros, and you compress.

This chapter develops intuition for what the DCT does, walks through
how real codecs use integer approximations of it, and shows what the
decoder must actually compute.

## 8.1 What a transform does, intuitively

The 2D DCT of an 8×8 block produces 64 coefficients arranged so that:

- **The top-left coefficient** (DC) is the block's average brightness.
- **Coefficients toward the top-left** capture low-frequency content
  (smooth gradients).
- **Coefficients toward the bottom-right** capture high-frequency
  content (sharp edges, fine texture, noise).

For natural images, most blocks are mostly smooth. So most blocks have
their energy concentrated in the top-left corner. The bottom-right is
mostly zero or near-zero. This is **energy compaction**: a small
number of coefficients carry most of the visual information.

Quantization throws away the small ones. Entropy coding represents the
result compactly (with a single EOB token signaling "everything else
is zero"). That's the win.

The basis vectors of the 8×8 DCT, viewed as 8×8 images, look like a
2D set of cosine waves at increasing frequencies. The top-left is
"flat" (DC); moving right or down increases the frequency along that
axis. Any 8×8 image is a weighted sum of these basis images — that's
what the coefficients are: the weights.

## 8.2 Separability

The 2D DCT is **separable**: a 2D DCT of an 8×8 block equals an 8-
point 1D DCT applied to each row, followed by an 8-point 1D DCT
applied to each column of the result. (Or column-first then row-first
— same answer.)

```rust
fn dct_2d(block: &[[i32; 8]; 8]) -> [[i32; 8]; 8] {
    let mut tmp = [[0i32; 8]; 8];
    for i in 0..8 {
        tmp[i] = dct_1d(&block[i]);   // each row
    }
    let mut out = [[0i32; 8]; 8];
    for j in 0..8 {
        let col = [tmp[0][j], tmp[1][j], tmp[2][j], tmp[3][j],
                   tmp[4][j], tmp[5][j], tmp[6][j], tmp[7][j]];
        let col_dct = dct_1d(&col);
        for i in 0..8 {
            out[i][j] = col_dct[i];
        }
    }
    out
}
```

This reduces the 2D operation to 16 1D operations. Every production
implementation uses this structure. The inverse (IDCT) decomposes the
same way.

## 8.3 The DCT math (★ skip on first read)

The 8-point DCT is a linear map. Treating an 8-vector `x[0..8]` as a
column, the DCT is `X = C · x` where `C` is the 8×8 DCT matrix:

```text
C[k][n] = √(α[k]) · cos((2n + 1) · k · π / 16)

with α[0] = 1/8, α[k>0] = 2/8.
```

The IDCT is `x = C^T · X` (the transpose, which happens to also be
the inverse — the DCT matrix is orthonormal).

You will essentially never write this code from scratch in a real
codec. Real codecs specify *integer approximations* of these formulas
that:

- Are bit-exact across implementations.
- Use small-integer math (no floating-point).
- Have a well-defined sequence of shifts and adds matching a
  specific factorization.

So the conceptual math here is "yes, a basis change with these
coefficients." The implementation reality is "transcribe the spec's
integer code."

## 8.4 Why integer transforms

Floating-point math is not bit-exact across platforms. Two compliant
H.264 decoders compiled with different compilers (or even different
flags) could produce slightly different float results for the same
inverse DCT — and that small drift compounds across frames into
visible degradation.

So every modern codec specifies an **integer transform** that's
algorithmically close to the DCT but uses only integer addition,
subtraction, multiplication by small constants, and shifts. The
specification pins down *every* intermediate value and shift, so
every conformant decoder produces bit-identical output.

The cost is a small loss in compression efficiency (the integer
transform isn't quite as good a basis as the true DCT). The gain is
correctness.

### H.264's 4×4 integer transform

H.264 specifies a 4×4 transform matrix:

```text
H = ⎡ 1   1   1   1 ⎤
    ⎢ 2   1  -1  -2 ⎥
    ⎢ 1  -1  -1   1 ⎥
    ⎣ 1  -2   2  -1 ⎦
```

Decode: `pixels = H^T · coeffs · H >> 6` (with specific rounding).
This is an approximation of the 4×4 DCT chosen so:

- The matrix entries are small integers (1, 2, easy multiplies).
- The forward and inverse can be implemented with only shifts and
  adds.
- It's bit-exact when computed in any conformant implementation.

Real H.264 decoders implement the 4×4 IDCT in ~30 lines of code per
direction (row pass + column pass), with the exact shift sequences in
the spec.

### HEVC and AV1 transforms

HEVC has 4×4, 8×8, 16×16, and 32×32 integer transforms — all
approximations of the DCT at each size.

AV1 has the most elaborate set:

- **DCT** at 4×4, 8×8, 16×16, 32×32, 64×64.
- **ADST** (asymmetric DST) for blocks with strong asymmetry — better
  for boundary content where the residual energy concentrates near
  one edge.
- **Identity transform** — no transform at all. Used when the
  residual is best left unspatialized (often happens at very high QP
  or for screen content).
- **Hybrid combinations** — DCT in one direction, ADST in the other,
  identity in the third configuration. Each block can pick.

The transform type per block is signaled in the bitstream. For
implementation: per direction (rows, columns), look up the transform
type, dispatch to the corresponding kernel.

```rust
fn inverse_transform_av1(coeffs: &[i32], block: &mut [i32],
                         tx_type: TxType, w: usize, h: usize) {
    let (row_tx, col_tx) = decompose_tx_type(tx_type);
    // Row pass:
    for i in 0..h {
        apply_1d_transform(row_tx, &coeffs[i*w..(i+1)*w]);
    }
    // Column pass:
    for j in 0..w {
        apply_1d_transform(col_tx, &/* column slice */);
    }
}
```

The decoder simply dispatches to the right 1D transform for each
direction. For AV1 specifically there are 16 valid `(tx_type,
block_size)` combinations per direction, each with its own bit-exact
1D kernel.

## 8.5 The decoder's job, end to end

For each block at stage 7 of the pipeline:

1. Read transform type (if codec supports multiple) from side info.
2. Read transform size (often inherited from partition size).
3. Receive dequantized coefficients from stage 6.
4. Apply the codec's specified inverse transform:
   - Row pass: 1D inverse transform on each row of coefficients.
   - Column pass: 1D inverse transform on each column of the
     intermediate.
5. Apply rounding shifts as specified.
6. Output: residual pixel values.

Conceptually: a function `inverse_transform(coeffs, type, size) →
residual` that's a few hundred lines of bit-exact integer math.

## 8.6 Common implementation gotchas

**Overflow.** Intermediate values can exceed 16-bit range. H.264's
inverse 4×4 transform produces intermediate sums fitting in 16 bits
exactly by careful balancing; HEVC's larger transforms need 32-bit
intermediates. The spec says exactly what bit-width to use.

**Rounding direction.** Specifications use a particular rounding rule
(typically "round to nearest, half away from zero" or "round to zero").
Round the wrong way and you accumulate bias across frames.

**Skip flag.** If the bitstream signals "this block has no transform
coefficients" (all-zero residual), the decoder must skip the entire
transform stage and use `residual = 0`. Don't run the inverse transform
on an all-zero buffer just for "correctness" — it wastes cycles and
sometimes triggers different intermediate values that fail
bit-exact tests.

**Transform skip.** Several modern codecs have a per-block "skip
transform" flag — use the quantized residual directly as the
spatial-domain residual, without any inverse transform. Used for
screen content and high-QP blocks where the transform's energy
compaction doesn't help.

## 8.7 Decoder vs encoder workload

**Decoder side:** The inverse transform is a fixed cost per block.
For 4K content, the decoder applies the inverse transform thousands of
times per frame. SIMD vectorization is critical; production decoders
write hand-tuned AVX2 / NEON kernels for the most common transform
sizes.

dav1d's transform kernels are state-of-the-art — see
`src/x86/itx_avx2.asm` for production AV1 inverse transforms in
assembly.

**Encoder side:** Forward transform is similarly cheap. The expensive
part is *trying different transforms* — for AV1, the encoder might
forward-transform a residual with each of 16 transform types, evaluate
the cost of each, and pick the best. That's 16× the transform cost
per block, plus the entropy coding cost evaluation. For high-quality
encodes this is a significant fraction of total time.

## 8.8 Where this lives in the workspace

This workspace's [ProRes integer
IDCT](../../crates/oximedia-codec/src/prores/idct.rs) is the worked
example. Read the file alongside SMPTE RDD 36 §C.4 (the ProRes IDCT
spec). You'll see:

- The 8-point 1D inverse kernel.
- The 2D structure (rows then columns).
- The specified rounding shifts.

For other codecs (when added):

```text
crates/oximedia-codec/src/h264/idct.rs
  inverse_4x4(coeffs) -> [[i16; 4]; 4]
  inverse_8x8(coeffs) -> [[i16; 8]; 8]

crates/oximedia-codec/src/av1/itx.rs
  inverse_dct_8(coeffs) -> [i16; 8]
  inverse_adst_8(coeffs) -> [i16; 8]
  inverse_identity_8(coeffs) -> [i16; 8]
  ...
```

For each codec, the inverse transform files are 200–1000 lines
total, dominated by the specific 1D kernels at each size.

## 8.9 Further reading

- **[Wiegand2003]** §III.E — H.264 transforms, including the 4×4
  integer matrix derivation.
- **[Sullivan2012]** §V — HEVC transforms (more sizes, more detail).
- **[Chen2020]** §5 — AV1 transforms (DCT, ADST, identity, hybrid
  combinations).
- **[Sayood2017]** Chapter 12 — DCT and transform coding from first
  principles, with worked examples on small blocks.
- **[Richardson2010]** Chapter 8 — H.264-specific transform with
  numeric walkthroughs.

For the math of DCT factorizations (AAN, Loeffler-Lichtenberg-
Moschytz), Pennebaker & Mitchell's *JPEG: Still Image Data Compression
Standard* covers it definitively. You won't need to derive
factorizations yourself, but knowing they exist helps you read fast
implementations.

## 8.10 Exercises

1. **DCT intuition.** Sketch an 8×8 block with one strong vertical
   edge in the middle (say, left half = 50, right half = 200). Where
   in the 8×8 coefficient matrix would you expect the energy to
   concentrate? (Hint: a vertical edge has horizontal-frequency
   content. Which axis is "horizontal" in your coefficient layout?)

2. **Energy compaction.** Take any 8×8 patch from a real image (use
   `ffmpeg` to extract a frame, then a python snippet to crop). Apply
   a 2D DCT (numpy: `scipy.fftpack.dct(dct(x, axis=0), axis=1)`).
   Report what fraction of total coefficient energy lives in the top-
   left 4×4 quadrant.

3. **H.264 integer transform.** Verify by hand that the H.264 4×4
   transform matrix `H` from §8.4 has orthogonal rows (the dot product
   of any two distinct rows is zero). This is why it's invertible.

4. *(Reading.)* Open
   [`crates/oximedia-codec/src/prores/idct.rs`](../../crates/oximedia-codec/src/prores/).
   Find the 8-point 1D inverse kernel. Count the number of additions
   and multiplications. Compare to a naïve "matrix × vector" version
   (8 multiplies × 8 sums = 64 multiplies). The fast IDCT saves
   constant-factor work by exploiting symmetry.

5. **Transform size selection.** Why would a codec want to support
   4×4 and 32×32 transforms? Sketch a scenario where each is optimal.
   (Hint: think about block size vs. residual structure.)

6. **AV1 ADST.** Why is asymmetric DST better than DCT for residuals
   with energy concentrated near one edge? (Hint: think about which
   basis functions of each transform have non-zero values where the
   energy is.)

---

Next: [Chapter 9 — Quantization](ch09-quantization.md).
