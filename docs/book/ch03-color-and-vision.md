# Chapter 3 — Color, Pixels, and Human Vision

> If you ship a codec, you will spend more time on color bugs than on
> any single algorithmic stage. The reason is that color is signaled
> through *metadata* — a handful of small integer fields buried in
> sequence headers and SEI messages — and getting any one of those
> fields wrong produces output that looks broken but isn't. This
> chapter develops the conceptual model you need to read those fields
> correctly, write them correctly, and diagnose mismatches.
>
> **Engineering takeaway:** Color is signaled by four small integer
> fields — `colour_primaries`, `transfer_characteristics`,
> `matrix_coefficients`, and `video_full_range_flag`. Get them right
> and color "just works." Get any one wrong and the picture looks
> broken in ways that *aren't* the decoder's fault. Reading those four
> fields out of any file should become reflex.

## 3.1 Why color leads the bug count

A junior engineer sees a washed-out skin tone and assumes the decoder
is broken. The senior engineer asks four questions:

1. What are the **primaries** of the source? (`colour_primaries`)
2. What is the **transfer function**? (`transfer_characteristics`)
3. What is the **matrix** used for YCbCr derivation?
   (`matrix_coefficients`)
4. What is the **range** — limited or full? (`video_full_range_flag`)

90% of "decoder bugs" are mismatches between what those four fields
*claim* and what the pixels actually are. The decoder is doing exactly
what the bitstream told it to. The producer mis-tagged. Or a
transcoder dropped the tags. Or a renderer didn't read them.

This chapter is about establishing a coherent mental model so you can
ask those four questions reflexively, and answer them by reading the
right bits.

## 3.2 Light, color, and the CIE 1931 diagram

A short detour into color science. Visible light is electromagnetic
radiation in roughly the 380–780 nm wavelength range. The human eye
has three cone receptor types — short (S, peaking around 420 nm),
medium (M, ~530 nm), and long (L, ~560 nm) — and one rod type for
low-light luminance.

Any spectral power distribution (SPD) of light gets reduced by these
three cone types to a 3-dimensional response. This is why color is
3-dimensional: not because light is 3-dimensional (SPDs live in
infinite-dimensional space), but because *human vision* projects it to
3D.

CIE 1931 standardized this projection. The CIE matching functions
x̄(λ), ȳ(λ), z̄(λ) map an SPD to three "tristimulus" coordinates X, Y,
Z that fully describe its visual response:

```text
X = ∫ S(λ) · x̄(λ) dλ
Y = ∫ S(λ) · ȳ(λ) dλ
Z = ∫ S(λ) · z̄(λ) dλ
```

Y is constructed so it corresponds to **luminance** — the brightness
sensation. X and Z together carry the chromatic information.

We usually work in the **chromaticity** plane:

```text
x = X / (X + Y + Z)
y = Y / (X + Y + Z)
```

The 2D (x, y) plane, plus a luminance Y, fully describes any color.
The famous horseshoe-shaped CIE 1931 chromaticity diagram is the set
of (x, y) reachable by physical light (the "spectral locus" is its
boundary).

This matters for codecs because **color spaces are defined by their
position in CIE 1931**.

## 3.3 Color spaces, primaries, and white point

A color space is, at a minimum, three things:

1. **Primaries.** The (x, y) chromaticity coordinates of red, green,
   blue. These are the corners of the gamut triangle inside CIE 1931.
2. **White point.** The (x, y) point that the system calls "pure
   white." For broadcast/streaming this is almost always D65 (x =
   0.3127, y = 0.3290), corresponding to roughly 6504 K. For print
   it's D50.
3. **Transfer function.** How code values map to linear light. This is
   so important it gets its own chapter ([Chapter 3](ch03-transfer-and-hdr.md)).

### Standard color spaces

The ones a working codec engineer must recognize:

| Space        | Primaries                        | White        | Gamut size | Used for                          |
|--------------|----------------------------------|--------------|------------|-----------------------------------|
| BT.601       | NTSC / PAL primaries             | D65          | Smallest   | SD broadcast (legacy)             |
| BT.709       | Same primaries as sRGB           | D65          | Small      | HD broadcast, streaming SDR       |
| BT.2020      | Wide-gamut "UHD" primaries       | D65          | Large      | UHD, 4K, HDR (HDR10 / HLG / DV)   |
| DCI-P3       | Digital cinema                   | DCI (~6300K) | Medium     | Digital cinema                    |
| Display P3   | DCI-P3 primaries                 | D65          | Medium     | Apple displays, modern phones     |
| ACES AP0     | Beyond spectral locus            | D60          | Huge       | ACES master working space         |
| ACES AP1     | Slightly larger than BT.2020     | D60          | Wide       | ACES working space (practical)    |

The chromaticities you'd need to actually compute the YCbCr matrix
yourself come from these standards documents — H.273 has all of them
in one table; that's why H.273 is the spec to keep on your desk
[H273Spec].

### Why the gamut size matters

Each color space's primaries define a triangle in CIE 1931. Colors
inside the triangle can be reproduced; colors outside can't. The
BT.2020 triangle is much larger than BT.709, which means BT.2020 can
represent more saturated reds and greens. (Blues are roughly similar
because the spectral locus is nearly straight there.)

If you encode BT.2020 content and a downstream tool *interprets* it as
BT.709 — because the `colour_primaries` field is missing or wrong — the
gamut gets shrunk and oversaturated colors clip to whatever's nearest
inside the BT.709 triangle. Skies bandify, foliage goes neon, skin
tones shift orange. **The fix is one integer in a header field, not a
re-encode.**

### The signaling fields (H.273)

The same set of fields appears across H.264, HEVC, AV1, and H.266:

| Field                       | What it signals                                        | Typical value                  |
|-----------------------------|--------------------------------------------------------|--------------------------------|
| `colour_primaries`          | (x,y) of R, G, B and the white point                   | 1=BT.709, 9=BT.2020, 12=DCI-P3 |
| `transfer_characteristics`  | The TF — see Chapter 3                                 | 1=BT.709, 16=PQ, 18=HLG        |
| `matrix_coefficients`       | The YCbCr derivation matrix                            | 1=BT.709, 9=BT.2020 NCL        |
| `video_full_range_flag`     | Limited (0) vs full (1) range                          | 0 for broadcast/streaming      |
| `chroma_sample_loc_type`    | Where the chroma sample sits inside the 2×2 luma block | 0=MPEG-2 (top-left), 2=center  |

The integer values are H.273-defined, and every modern codec spec
defers to H.273 for their semantics. Once you've read H.273 once, you
can read any codec's color signaling.

## 3.4 RGB to YCbCr: decorrelation for compression

RGB is bad for compression because R, G, and B are correlated. A
green leaf is high in G but also high in R and B — knowing the value
of one channel predicts the others. Compressing them independently
wastes bits.

YCbCr trades three correlated channels for:
- **Y** — luma, the brightness channel (highly correlated to human
  brightness perception).
- **Cb** — blue minus luma (chrominance "blue-yellow" axis).
- **Cr** — red minus luma (chrominance "red-cyan" axis).

After conversion, Y carries most of the energy and most of the visual
importance. Cb and Cr are nearly zero on grayscale content and carry
the small color difference. Most importantly, they are largely
decorrelated from Y — different statistics, different optimal
quantization, different sensitivity to error.

### The matrices

The conversion is *not* a fixed matrix. It depends on the primaries.
For each color space, the standard luma coefficients (Kr, Kg, Kb) are:

```text
BT.601:   Kr = 0.299,   Kg = 0.587,   Kb = 0.114
BT.709:   Kr = 0.2126,  Kg = 0.7152,  Kb = 0.0722
BT.2020:  Kr = 0.2627,  Kg = 0.6780,  Kb = 0.0593
```

These coefficients are derived from the primaries themselves: they
fall out of the requirement that Y match CIE Y (luminance) for white
light at the system's white point.

Then:

```text
Y  = Kr · R + Kg · G + Kb · B
Cb = (B − Y) / (2 · (1 − Kb))   = (B − Y) / 1.8556   for BT.709
Cr = (R − Y) / (2 · (1 − Kr))   = (R − Y) / 1.5748   for BT.709
```

Cb and Cr land in [-0.5, 0.5] for input R/G/B in [0, 1]. To store them
as unsigned integers, we add an offset (½ in floating point, 128 for
8-bit, 512 for 10-bit, 2048 for 12-bit):

```text
Y'  = round(Y · (Yhi − Ylo) + Ylo)       — limited range
Cb' = round(Cb · (Chi − Clo) + (Chi + Clo)/2)
Cr' = round(Cr · (Chi − Clo) + (Chi + Clo)/2)
```

For 8-bit limited range, Y ∈ [16, 235], Cb,Cr ∈ [16, 240]. For 10-bit
limited range, Y ∈ [64, 940], Cb,Cr ∈ [64, 960].

### Why the matrix matters for diagnostics

The same R/G/B values produce *different* (Y, Cb, Cr) under each
matrix. If the encoder used BT.601 coefficients but tagged the
bitstream as BT.709, every color gets shifted slightly — most
noticeably, skin tones go pinkish. The bug is one integer in the VUI.

Test for this by encoding a known patch (Macbeth chart, color bars)
through your pipeline and comparing measured (Y, Cb, Cr) to predicted.

### Constant luminance vs. non-constant luminance

There are actually *two* BT.2020 matrices. The "non-constant
luminance" (NCL) one is the straightforward extension of the BT.709
formula above. The "constant luminance" (CL) one converts to YCbCr
*after* the transfer function, in a way that makes Y a better
luminance approximation under wide-gamut conditions. Almost no one
uses CL in practice; everything is NCL. But know that the
distinction exists, because the H.273 `matrix_coefficients` field has
separate values for the two.

## 3.5 Chroma subsampling

The human eye has roughly 5× more luminance receptors than color
receptors. So codecs throw away color resolution:

| Notation | Y resolution | C resolution     | Samples per pixel |
|----------|--------------|------------------|-------------------|
| 4:4:4    | full         | full             | 3.0               |
| 4:2:2    | full         | ½ horizontal     | 2.0               |
| 4:2:0    | full         | ½ both axes      | 1.5               |
| 4:1:1    | full         | ¼ horizontal     | 1.5               |

**4:2:0** is universal for streaming and broadcast. ½ the data of
4:4:4 at imperceptible quality loss for typical content. **4:2:2** is
the broadcast/post-production standard (ProRes 422, DNxHR HQ, AVC-Intra
422) — preserves enough chroma detail for chroma keying and grading.
**4:4:4** is for graphics, screen content, color grading. **4:1:1**
appears in DV / DVCPRO 25 and (mostly) no one else.

### Where exactly is the chroma sample?

In 4:2:0 a single (Cb, Cr) sample stands in for a 2×2 block of luma
samples. *Where, inside that 2×2 block, does the chroma sample
conceptually sit?*

There are three answers in use:

- **MPEG-2 / H.264 / HEVC / AV1 / VVC** — chroma sample at the
  *top-left* of the 2×2 luma block (horizontally aligned with luma,
  vertically halfway between luma rows). H.273 calls this
  `chroma_sample_loc_type = 0`.
- **MPEG-1 / JPEG / DV** — chroma sample at the *center* of the 2×2
  luma block. H.273 value `2`.
- **DV (PAL 4:2:0)** — top-left, but with a quirky vertical alignment.

Mismatches cause 1-pixel color shifts on gradients — usually visible
as a half-pixel color smear on sharp edges. This is the canonical
"this looks slightly off but I can't tell why" bug.

### Chroma upsampling for display

When you decode 4:2:0 and want to display 4:4:4 (every monitor is
4:4:4 internally), the decoder has to **upsample** chroma. Bilinear is
the default in most software; nicer filters (bicubic, Lanczos) trade
sharpness against ringing. For HDR/wide-gamut content the upsampling
choice matters more — banding becomes visible.

For codec engineering purposes, **bit-exact upsampling is part of
neither the standard nor the decoder's correctness**; it's a renderer
concern. But it's the source of most "the decoder looks soft" reports,
and the answer is to inspect the renderer's chroma upsampling filter,
not the decoder.

## 3.6 Bit depth

Each component (Y, Cb, Cr) is stored as integers. Common depths:

| Bit depth | Codecs / formats                                     | Used for                       |
|-----------|------------------------------------------------------|--------------------------------|
| 8-bit     | H.264 Main, HEVC Main 8, AV1 (most profiles), JPEG   | Streaming SDR, capture         |
| 10-bit    | H.264 High 10, HEVC Main 10, AV1, ProRes, DNxHR     | HDR, broadcast, post           |
| 12-bit    | HEVC Main 12, AV1 12, ProRes 4444 XQ                 | Cinema, grading masters        |
| 14/16-bit | RAW, OpenEXR                                         | Camera negatives, VFX          |

The naive view of bit depth is "more colors". The engineering view is
**intermediate precision**.

Consider an 8-bit luma gradient from 50 to 60 over 100 pixels. The
difference per pixel is 0.1, which rounds to 0. The gradient is
encoded as flat 50-50-50-…-60-60-60 with a sharp step somewhere — the
"banding" artifact. At 10-bit, the same gradient gets quarter-bit
gradations, and the bands disappear.

Banding is worst on smooth gradients — skies, gradients on skin, flat
walls — and worst at low light, where the eye is most sensitive to
quantization. **HDR essentially requires 10-bit minimum**, because PQ's
perceptual quantization (Chapter 3) concentrates code values where
banding would otherwise be visible.

### High-bit-depth math through 8-bit pipelines

Most 8-bit content was *processed* internally at higher precision —
in the camera ISP, in the grading suite, in the encoder's internal
fixed-point. The 8-bit truncation happens at the last possible
moment. If you process 8-bit content through an 8-bit pipeline, every
intermediate operation rounds and accumulates error; gradients band.

This is why high-quality encoders (x264 with `--input-depth 10
--output-depth 10` even when targeting 8-bit content, x265 likewise)
upsample to 10-bit internally regardless of the source. The deeper
intermediate precision avoids quantization at every stage. The output
bit depth is whatever the bitstream profile demands.

## 3.7 Range: limited vs. full

Independently of bit depth, the *range of valid code values* can be:

| Range        | 8-bit Y values | 8-bit Cb/Cr values | 10-bit Y     | 10-bit Cb/Cr |
|--------------|----------------|--------------------|--------------|--------------|
| Limited / Video / TV | 16 – 235 | 16 – 240   | 64 – 940     | 64 – 960     |
| Full / PC / Studio   | 0 – 255  | 0 – 255    | 0 – 1023     | 0 – 1023     |

**Limited range** dates from analog video, where sync pulses needed
headroom below black and overshoots needed headroom above white.
Broadcast and streaming SDR are universally limited range.

**Full range** uses the entire code value space. Common in PC video
(YUV "JPEG" mode), some HDR content, some camera RAW intermediates.

Mixing them is the most common color bug after primaries mismatch:

- Limited content interpreted as full → "washed out" — blacks lift
  toward gray, whites compress.
- Full content interpreted as limited → "crushed" — values below 16
  clip to black, values above 235 clip to white.

The relevant bitstream field is `video_full_range_flag`:
0 = limited, 1 = full. **Always signal it.** Some tools (FFmpeg
specifically) will *guess* the range from heuristics if the field is
absent, and the heuristics are often wrong.

## 3.8 Putting it all together: a worked diagnostic

A pipeline produces output that "looks washed out." You have access to
the bitstream and a reference monitor. What do you check, in order?

1. **`video_full_range_flag`.** Most common failure. If the source is
   full-range and the bitstream signals limited (or vice versa), this
   is the bug. Diagnostic: render a known patch — pure black should be
   Y=16 (limited 8-bit) or Y=0 (full); pure white should be Y=235 or
   Y=255. Read out the actual stored Y on the patch and compare.

2. **`colour_primaries`.** If the source is BT.709 but signaled as
   BT.2020 (or unsignaled, and the renderer defaults to BT.2020), the
   gamut gets stretched, colors desaturate. Diagnostic: encode a
   100% red bar (R=255, G=0, B=0 in source RGB). Decoded and
   color-managed back to source RGB, the red should be the same code
   value. Compare to predicted.

3. **`matrix_coefficients`.** If the YCbCr matrix used in encoding
   doesn't match the one signaled, skin tones shift. Diagnostic:
   compute the predicted (Y, Cb, Cr) for a known RGB patch under each
   candidate matrix; the bitstream value closest to actual identifies
   the encoder's choice. Then check if it matches the signaled value.

4. **`transfer_characteristics`.** SDR signaled as HDR, or HDR signaled
   as SDR, produces dramatic brightness errors. Less subtle, usually
   caught quickly. (Chapter 3.)

5. **`chroma_sample_loc_type`.** If everything else is right but
   gradients have a 1-pixel color smear, this is the culprit.

Most of these are *one-integer fixes*. A serious engineer can read all
five out of an H.264 stream in two minutes by `ffprobe -show_streams`
and looking at the `color_*` fields. Build that into reflex.

## 3.9 Further reading

- **[Poynton2012]** — Charles Poynton's *Digital Video and HD*. The
  canonical color reference. Read Part I (Introduction) and the
  YCbCr / chroma subsampling chapters. The math of every matrix in
  this chapter is derived in Poynton from scratch.
- **[H273Spec]** — ITU-T H.273. The signaling-field bible. Short
  document. Keep it on your desk.
- **[BT709]**, **[BT2020]** — the ITU recommendations themselves.
  Short, terse, but authoritative.
- **[Wiegand2003]** §IV — the H.264 overview paper covers VUI
  signaling in a compact form.

For a working engineer's introduction, the FFmpeg wiki page on color
metadata and the Poynton website (search "Poynton color FAQ") cover
the practical end of this chapter without the math.

If you make color bugs and can't reproduce them, [Doom9] forum
archives and the [DarkShikari] blog have many real-world diagnostic
walkthroughs.

## 3.10 Exercises

1. Compute the (Y, Cb, Cr) values, 8-bit limited range, for the
   following RGB inputs under BT.709 and BT.601 matrices: pure red
   (255, 0, 0), pure green (0, 255, 0), pure blue (0, 0, 255), 50%
   gray (128, 128, 128). For which input does the matrix choice make
   the most difference? Hypothesize why.

2. A test pattern shows a smooth horizontal gradient from black to
   white. After encoding and decoding, you see banding. Three
   plausible causes (besides "the codec quantized too aggressively")
   are: source was 8-bit, range mismatch, bit-depth truncation in the
   render path. How would you distinguish them?

3. *In H.273.* Look up `matrix_coefficients = 9` and
   `matrix_coefficients = 10`. What is the difference? Why are there
   two BT.2020 entries?

4. *In the codebase.* `wire_formats.md` describes NV12. Read its
   description and answer: is NV12 4:2:0 with MPEG-2 chroma siting or
   centered? What happens if you feed it to a renderer that assumes
   the other convention?

5. A file is tagged BT.709 but the colors look oversaturated on a
   BT.2020 monitor. Without re-encoding, what one field can you
   change to fix it (in some cases)? When would that fix be wrong?

6. Bonus / open-ended. Find the YCbCr matrix derivation in Poynton.
   Re-derive the BT.709 Kr / Kg / Kb coefficients from the BT.709
   primaries and a D65 white point. (This is one page of linear
   algebra and will demystify the table forever.)

---

Next: [Chapter 4 — Transfer Functions and HDR](ch04-transfer-and-hdr.md).
