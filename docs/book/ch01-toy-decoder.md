# Chapter 1 — Build a Toy Decoder

> **Engineering takeaway:** Every modern video decoder has the same
> skeleton — parse a header, then per-block: entropy-decode, dequantize,
> inverse-transform, predict, add, write. This chapter implements that
> skeleton end-to-end in ~200 lines of Rust for a minimal codec we
> invent on the spot. Once you can see the whole pipeline running, the
> rest of the book is just specializing each stage for real codecs.

The fastest way to demystify codecs is to write one. Not a real one —
those are tens of thousands of lines, and the complexity is in the
*details* of each stage, not the overall shape. We'll invent the
simplest possible codec that still has every stage of the pipeline,
implement a decoder for it in this chapter, and then spend the rest
of the book replacing each stage with the real-codec version.

The toy codec is intra-only (no motion, no reference frames),
grayscale only (one plane, no chroma), 8-bit. It uses:

- **8×8 blocks** in raster order (same as JPEG).
- **DCT** transform (same as JPEG / H.264 / HEVC / ProRes; AV1 uses
  more variants).
- **Uniform scalar quantization** with a single QP for the whole frame.
- **Zigzag scan** to convert 2D coefficients to a 1D sequence.
- **Run-length-plus-magnitude** entropy coding (a stripped-down JPEG
  CAVLC).

Despite its simplicity, this toy codec exercises every stage of the
block-based hybrid decoder pipeline. The mental model you build here
will transfer directly to ProRes, H.264, HEVC, and AV1 — those codecs
just have more options at each stage.

## 1.1 The toy codec format ("TOY1")

We need to invent a bitstream format. Here it is, in 12 lines:

```text
Header (fixed, byte-aligned, 9 bytes):

  Offset  Length  Field         Value
  ──────  ──────  ─────         ─────
  0       4       magic         ASCII "TOY1"
  4       2       width         u16 big-endian, multiple of 8
  6       2       height        u16 big-endian, multiple of 8
  8       1       qp            u8, 1..=127

Block data (bit-stream, starts immediately after header):

  For each 8×8 block, in raster scan (left-to-right, top-to-bottom):
    Read coefficients until 64 are accumulated:
      Read 1 byte. High 4 bits = zero_run, low 4 bits = size.
      If zero_run == 0xF and size == 0:      append 16 zeros and continue
      If zero_run == 0x0 and size == 0:      pad remaining with zeros (EOB)
      Else:
        Append `zero_run` zeros.
        Read `size` more bits, interpret as signed integer, append.
    The 64 coefficients are in zigzag order (see §1.4).
```

That's the entire spec. A real codec spec is 800 pages of this kind of
prose — same vocabulary, more knobs.

To make it concrete, here's a hand-encoded 8×8 block of all zeros
except DC = 42:

```text
0x01    →  zero_run=0, size=1.  Read 1 bit: assume 0. Append 0… wait,
           we need size>0 to give a magnitude. Actually for DC=42,
           we'd need:
0x06    →  zero_run=0, size=6.  Read 6 bits: 101010 = 42. Append 42.
0x00    →  zero_run=0, size=0.  EOB. Pad remaining 63 coeffs with 0.
```

Two bytes (`0x06 0x06` and `0x00`) for one block. Good compression!

(A real H.264 decoder would parse this same conceptual structure
through CABAC, with each `(zero_run, size, magnitude)` triple decoded
from arithmetic-coded bits using context models. The shape is the
same; the mechanism is more complex. See Chapter 10.)

## 1.2 The decoder skeleton

Here is the entire decoder, in Rust pseudocode. We'll explain each
piece in §1.3–§1.6.

```rust
pub fn decode(input: &[u8]) -> Result<Frame, DecodeError> {
    // 1. Parse the 9-byte header.
    let header = parse_header(input)?;
    let block_data = &input[9..];

    // 2. Allocate output frame.
    let mut frame = Frame::new(header.width, header.height);

    // 3. Set up the bitstream reader.
    let mut bits = BitReader::new(block_data);

    // 4. For each 8×8 block in raster order:
    let blocks_x = header.width  as usize / 8;
    let blocks_y = header.height as usize / 8;

    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            // 4a. Entropy decode 64 coefficients (zigzag order).
            let zz = entropy_decode_block(&mut bits)?;

            // 4b. Un-zigzag to 2D order.
            let mut coeffs = [[0i16; 8]; 8];
            for k in 0..64 {
                let (i, j) = ZIGZAG[k];
                coeffs[i][j] = zz[k];
            }

            // 4c. Dequantize.
            for i in 0..8 {
                for j in 0..8 {
                    coeffs[i][j] *= header.qp as i16;
                }
            }

            // 4d. Inverse DCT — frequency to space.
            let pixels = idct_8x8(&coeffs);

            // 4e. Clamp to u8 and write into the frame.
            for i in 0..8 {
                for j in 0..8 {
                    let v = pixels[i][j].clamp(0, 255) as u8;
                    frame.set(bx * 8 + j, by * 8 + i, v);
                }
            }
        }
    }

    Ok(frame)
}
```

**Read it like prose.** Every modern decoder — H.264, HEVC, AV1,
ProRes — is structured exactly like this at the top level. The
differences are:

- They have a prediction step before the IDCT (intra or inter), so the
  block is `prediction + residual` rather than just `residual`.
- They run a deblocking filter after each frame.
- Their bitstream is divided into NAL units / OBUs / slices.
- Their entropy decoder is CABAC / range-coder, not raw bit reads.
- Their transforms come in multiple sizes and types.
- Their quantization is non-uniform (a matrix, not a scalar).

But the **pipeline shape** — `parse header → per-block { entropy →
dequant → transform → add → store }` — is unchanged. **This is the
single most important thing in the book.**

## 1.3 The header parse

The simplest stage. Read 9 bytes, validate, return a struct:

```rust
pub struct Header {
    pub width:  u16,
    pub height: u16,
    pub qp:     u8,
}

fn parse_header(input: &[u8]) -> Result<Header, DecodeError> {
    if input.len() < 9 {
        return Err(DecodeError::TooShort);
    }
    if &input[0..4] != b"TOY1" {
        return Err(DecodeError::BadMagic);
    }
    let width  = u16::from_be_bytes([input[4], input[5]]);
    let height = u16::from_be_bytes([input[6], input[7]]);
    let qp     = input[8];

    if width  % 8 != 0 { return Err(DecodeError::BadDimension); }
    if height % 8 != 0 { return Err(DecodeError::BadDimension); }
    if qp == 0 || qp > 127 { return Err(DecodeError::BadQp); }

    Ok(Header { width, height, qp })
}
```

The real decoder equivalent of this is **parsing an SPS** for H.264.
That's also a header — just with ~60 fields instead of 3, encoded
variable-length. Chapter 2 covers the bit-reading and exp-Golomb
mechanics you need for SPS parsing. The *structure* of "parse the
header into a struct, validate, then proceed" is the same.

## 1.4 The zigzag table

The DCT produces coefficients arranged in a 2D 8×8 block where:
- The **top-left** entry is the **DC** coefficient (the block's
  average).
- Coefficients toward the top-left are **low frequency** (smooth).
- Coefficients toward the bottom-right are **high frequency** (sharp
  edges, noise).

After quantization, most high-frequency coefficients are zero. To
exploit this for entropy coding, we want to *scan the coefficients in
roughly frequency order* so the long run of zeros at the end can be
RLE'd into a single EOB token. The scan order is called the **zigzag**:

```text
   0  1  5  6 14 15 27 28
   2  4  7 13 16 26 29 42
   3  8 12 17 25 30 41 43
   9 11 18 24 31 40 44 53
  10 19 23 32 39 45 52 54
  20 22 33 38 46 51 55 60
  21 34 37 47 50 56 59 61
  35 36 48 49 57 58 62 63
```

So coefficient at (i=0,j=0) is scanned first, then (0,1), then (1,0),
(2,0), (1,1), (0,2), and so on, sweeping diagonally toward the
bottom-right.

In code, we precompute the inverse mapping — given scan position `k`,
what's the (row, col) — as a constant table:

```rust
const ZIGZAG: [(usize, usize); 64] = [
    (0,0), (0,1), (1,0), (2,0), (1,1), (0,2), (0,3), (1,2),
    (2,1), (3,0), (4,0), (3,1), (2,2), (1,3), (0,4), (0,5),
    // ... 48 more entries
];
```

The decoder calls this table to "un-zigzag" — given the linear
sequence of 64 coefficients the entropy coder produced, write each
one to its 2D location.

Different codecs use different scans. H.264 uses zigzag for
progressive content, field-scan for interlaced. HEVC has multiple
scans (diagonal, horizontal, vertical). AV1 has even more, picked
based on transform type. But every codec has *a* scan that converts
2D blocks to 1D sequences for entropy coding. See Chapter 8 for the
zoo.

## 1.5 Entropy decoding

This is where the actual bitstream is consumed. Given a `BitReader`
(covered in detail in Ch 2), we decode one block:

```rust
fn entropy_decode_block(bits: &mut BitReader) -> Result<[i16; 64], DecodeError> {
    let mut coeffs = [0i16; 64];
    let mut pos = 0;

    while pos < 64 {
        let byte = bits.read_bits(8)? as u8;
        let zero_run = (byte >> 4) as usize;
        let size     = (byte & 0x0F) as usize;

        if zero_run == 0xF && size == 0 {
            // ZRL: 16 zeros, then continue.
            pos = (pos + 16).min(64);
            continue;
        }
        if zero_run == 0 && size == 0 {
            // EOB: rest are zero. (Already initialized to 0.)
            break;
        }

        // Append `zero_run` zeros.
        pos = (pos + zero_run).min(64);
        if pos >= 64 { break; }

        // Append signed magnitude of width `size` bits.
        let magnitude_bits = bits.read_bits(size)? as i16;
        let value = decode_signed_magnitude(magnitude_bits, size);
        coeffs[pos] = value;
        pos += 1;
    }

    Ok(coeffs)
}

fn decode_signed_magnitude(bits: i16, size: usize) -> i16 {
    // JPEG-style signed-magnitude: the high bit indicates sign.
    let threshold = 1i16 << (size - 1);
    if bits >= threshold { bits } else { bits - (1i16 << size) + 1 }
}
```

The reader implementation (`BitReader::read_bits(n)`) is the subject
of Chapter 2. For now, treat it as "give me the next n bits as an
unsigned integer."

In a real codec, this stage is more elaborate. H.264 CAVLC parses
five separate fields per block (TotalCoeff, TrailingOnes, levels,
total_zeros, run_before). H.264 CABAC parses everything as
context-modeled arithmetic-coded bins. But the *output* is the same
shape: a list of (position, value) for the non-zero coefficients in
zigzag order, plus an implicit EOB.

## 1.6 Inverse quantization

Quantization in the encoder threw away precision by dividing each
coefficient by a step size (the QP). Inverse quantization in the
decoder undoes the division:

```rust
fn dequantize(coeffs: &mut [[i16; 8]; 8], qp: u8) {
    for i in 0..8 {
        for j in 0..8 {
            coeffs[i][j] *= qp as i16;
        }
    }
}
```

In a real codec, the quantization is **non-uniform** — different
coefficients have different step sizes, encoded in a **quantization
matrix**. The matrix is designed to throw away more high-frequency
precision (where the eye doesn't notice) and less low-frequency
precision (where it does):

```text
H.264 default intra 8×8 luma matrix (lower = preserve precision):

   6 10 13 16 18 23 25 27
  10 11 16 18 23 25 27 29
  13 16 18 23 25 27 29 31
  16 18 23 25 27 29 31 33
  18 23 25 27 29 31 33 36
  23 25 27 29 31 33 36 38
  25 27 29 31 33 36 38 40
  27 29 31 33 36 38 40 42
```

Low frequencies (top-left) use smaller steps (6-18); high frequencies
(bottom-right) use larger steps (33-42). The QP scales this whole
matrix. Chapter 9 explains how the matrix interacts with QP.

For our toy codec, we just multiply by QP scalar. Same idea, no
matrix.

## 1.7 The inverse DCT

The DCT (Discrete Cosine Transform) is a basis change. The 8×8 block
of pixels becomes an 8×8 block of frequency coefficients; the IDCT
goes back. **For decoder purposes you can treat the IDCT as a black
box that turns frequency back into pixels** — the math is in Chapter
8.

The 2D IDCT is *separable*: a 2D IDCT is two 1D IDCTs (one along
each axis):

```rust
fn idct_8x8(coeffs: &[[i16; 8]; 8]) -> [[i16; 8]; 8] {
    // Step 1: 1D IDCT on each row.
    let mut tmp = [[0f32; 8]; 8];
    for i in 0..8 {
        let row_in = coeffs[i].map(|x| x as f32);
        let row_out = idct_1d(&row_in);
        tmp[i] = row_out;
    }

    // Step 2: 1D IDCT on each column.
    let mut out = [[0i16; 8]; 8];
    for j in 0..8 {
        let col_in = [
            tmp[0][j], tmp[1][j], tmp[2][j], tmp[3][j],
            tmp[4][j], tmp[5][j], tmp[6][j], tmp[7][j],
        ];
        let col_out = idct_1d(&col_in);
        for i in 0..8 {
            out[i][j] = col_out[i].round() as i16;
        }
    }

    out
}

fn idct_1d(input: &[f32; 8]) -> [f32; 8] {
    // Standard 8-point IDCT.  Don't worry about why these coefficients —
    // Chapter 8 derives them.  Accept that this matrix sends
    // frequency back to pixels.
    let c = [
        0.35355339, 0.35355339, 0.35355339, 0.35355339,
        0.35355339, 0.35355339, 0.35355339, 0.35355339,
    ]; // (placeholder — full matrix in Ch 8)
    // ... 8-point IDCT computation
    *input // placeholder
}
```

For correctness in a real implementation, the 1D IDCT is either:
- A direct matrix multiply (slow but trivially correct), or
- A "fast IDCT" that exploits the cosine symmetry — the **AAN
  algorithm** for JPEG, **Loeffler-Lichtenberg-Moschytz** for many
  others. Modern codecs (H.264, HEVC, AV1) use **integer transforms**
  that approximate the DCT without floating-point, designed for
  bit-exact reproducibility.

This workspace's [ProRes IDCT](../../crates/oximedia-codec/src/prores/idct.rs)
is a worked example of an integer 8×8 IDCT. For ProRes specifically,
the integer transform is specified in [SMPTE RDD 36] exactly enough
that every decoder produces bit-identical output.

For the toy codec, any IDCT implementation works (as long as you also
use its matching DCT in your encoder).

## 1.8 Putting it all together — what you'd see running

If you write the encoder side too (encode an 8×8 grayscale image of
the letter A), the round trip looks like:

```text
Original 8x8 luma block (grayscale):

  10  10  10 200 200  10  10  10        (the spine of an "A")
  10  10 200 200 200 200  10  10
  10 200 200  10  10 200 200  10
  10 200 200 200 200 200 200  10
  10 200 200  10  10 200 200  10
  10 200 200  10  10 200 200  10
  10 200 200  10  10 200 200  10
  10 200 200  10  10 200 200  10

Forward DCT → quantize with QP=8 → zigzag → entropy encode →
bytes (~12 bytes encoded for this block).

Decoder:
bytes → entropy decode → un-zigzag → dequantize → IDCT →

Reconstructed 8x8 block:

  11  9  12 197 198  12  10  11
  10 11  198 199 200 199  9  10
  11 199 198  11   9 198 199  11
  ...

Pixel-by-pixel error: ~1-3 per pixel.
This is loss. The eye can't see it.
```

Run the decode → display → encode → display roundtrip and you've
witnessed compression in action.

## 1.9 Mapping the toy decoder to real codecs

Here's where the conceptual payoff lands. Same skeleton, different
codec — same eight stages, with these substitutions:

| Stage          | Toy codec                | ProRes                   | H.264                       | AV1                         |
|----------------|--------------------------|--------------------------|------------------------------|------------------------------|
| Header parse   | 9 fixed bytes            | Frame header + slice hdr  | NAL → SPS → PPS → slice hdr  | OBU sequence + frame hdr     |
| Block iteration| 8×8 raster               | 8×8 within slices         | 16×16 macroblocks            | up to 128×128 superblocks    |
| Prediction     | None                     | None (intra-only, no spatial pred) | Intra (9 dirs) / inter (MV) | Intra (56) / inter / IBC     |
| Entropy decode | Run-length + magnitude   | CAVLC variant             | CAVLC or CABAC               | Range coder + symbol coder   |
| Un-scan        | Zigzag                   | Slice-specific scan       | Zigzag or field-scan         | Multiple (per tx type)       |
| Dequantize     | QP × coeff               | Slice QP × matrix         | QP × matrix                  | QP × matrix                  |
| Inverse xform  | 8×8 float IDCT           | 8×8 integer IDCT          | 4×4 / 8×8 integer            | 4×4 ... 64×64, many types    |
| Add prediction | (no prediction step)     | (no prediction step)      | residual + prediction        | residual + prediction        |
| Post-filter    | None                     | None                      | Deblocking                   | Deblock + CDEF + restoration |
| Emit frame     | Direct write             | Direct write              | DPB reorder by PTS           | DPB reorder by PTS           |

You now have a column to put every chapter of this book in. Chapter
6 fills in the intra prediction column. Chapter 7 fills in motion.
Chapter 8 specializes the transform. Chapter 10 specializes entropy
coding. Etc.

**Every chapter from here on says: here's what column N looks like
when you replace the toy codec's choice with codec X's.** That's the
entire book in one sentence.

## 1.10 Where this lives in the workspace

If you want to see a *real* implementation of the same skeleton, walk
through [`crates/oximedia-codec/src/prores/`](../../crates/oximedia-codec/src/prores/):

```text
prores/
  mod.rs       — top-level decode function (the skeleton)
  parser.rs    — header parsing
  entropy.rs   — slice entropy decode
  quant.rs     — dequantization (matrix-based, not scalar)
  idct.rs      — integer 8×8 IDCT
  assemble.rs  — block-to-plane assembly
```

This mirrors the toy decoder section-by-section. Recommended reading
after Chapter 1: open `prores/mod.rs` next to this chapter, read both
top-to-bottom, and notice that they have the same *shape* even though
ProRes is a real broadcast-grade codec.

The companion doc [`prores_decoder.md`](../prores_decoder.md) walks
through that real decoder at the same level of detail this chapter
walked through the toy.

## 1.11 What you should be able to do now

After reading and (importantly) re-reading this chapter, you should
be able to:

1. **State the eight stages of a decoder pipeline without notes.**
2. **Recognize each stage in real codec source code** — when you open
   x264 or libavcodec, you should be able to point at "this is the
   entropy decoder, this is the dequant, this is the IDCT" within a
   few minutes of reading.
3. **Explain why each stage exists** — without invoking information
   theory (yet). The toy codec's structure is its own argument: every
   stage trades something for something else.
4. **Translate a small spec into Rust** — the toy codec spec was 12
   lines; you turned it into 200 lines of decoder. The H.264 spec is
   800 pages; the decoder is ~50,000 lines. The ratio is similar.

## 1.12 Exercises

1. **Implement the toy decoder for real.** All the code in this
   chapter is in this workspace's
   [`crates/oximedia-codec/src/toy/`](../../crates/oximedia-codec/src/toy/)
   directory *(to be added — see the issue tracker)*. Or: write it
   yourself from scratch using this chapter as the spec. Get one 8×8
   block round-tripping (encode-then-decode) with <5% pixel error.

2. **Encode a "QP sweep."** Take any 64×64 grayscale image. Encode it
   with your toy encoder at QP = 1, 4, 16, 64. Compute the MSE between
   reconstructed and original. Plot bits-per-pixel vs MSE on a
   log-linear axis. You've just drawn your first R-D curve.

3. **Read a real decoder.** Open
   [`crates/oximedia-codec/src/prores/mod.rs`](../../crates/oximedia-codec/src/prores/).
   Trace one frame's decode end-to-end. Mark in a notebook where each
   of the eight stages happens. Compare to the table in §1.9.

4. *(Reading.)* Open `libavcodec/h264dec.c` in any FFmpeg
   checkout. Find where the slice data is decoded — it's a function
   roughly called `ff_h264_decode_slice` or similar. Don't try to
   understand it; just count the stages. There will be more than
   eight (CAVLC/CABAC table setup, deblocking, MBAFF/PAFF
   complications), but the core pipeline is there.

5. **Conceptual:** What would change in the toy decoder skeleton to
   add **inter prediction** (P-frames)? Sketch the modified
   pseudo-code. (Hint: you need a reference frame buffer, a per-block
   motion vector, a `fetch_block_from_reference` function, and the
   formula becomes `pixels = idct(coeffs) + reference_block`.)

## 1.13 What's next

You can read a high-level pipeline. The two next things you need:

- **[Chapter 2 — Bitstream I/O](ch02-bitstream-io.md).** The actual
  mechanics of reading bits, exp-Golomb (used by H.264/HEVC), and how
  to turn an ITU spec table into Rust. This is the skill that gates
  every codec implementation.
- **[Chapter 3 — Color](ch03-color-and-vision.md).** What yuv420p
  really means, and the four header fields that cause 90% of
  production color bugs.

Either chapter is fine to read next. Ch 2 is mandatory before you
implement anything; Ch 3 is mandatory before you debug anything.
