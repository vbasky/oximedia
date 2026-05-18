# Chapter 4 — Transfer Functions and HDR

> A transfer function (TF) is the curve that maps stored code values
> to real-world luminance. Get it wrong and the picture is dramatically
> too dark, too bright, or has the wrong contrast. This chapter walks
> through the SDR TF (sRGB / Rec.709), then PQ and HLG for HDR, then
> the metadata and tone-mapping plumbing around them. By the end you
> should be able to read a code value, name its TF, and convert it to
> cd/m².
>
> **Engineering takeaway:** A TF maps stored code values to actual
> light. SDR uses BT.709 / sRGB power-law. HDR uses PQ (absolute,
> 0–10,000 nits) or HLG (relative, display-adaptive). The codec does
> not *apply* the TF — it carries TF-encoded values through unchanged.
> But knowing which TF is in use matters for quantization choices
> upstream and for the renderer / display pipeline downstream.

## 4.1 Why transfer functions exist

If you store luminance values linearly — code 100 means twice as much
light as code 50 — you waste code values. The eye is *not* linear in
how it perceives brightness: doubling cd/m² from 100 to 200 is
visually the same step as doubling from 1000 to 2000, but linear
encoding gives the first step 100 code values and the second 1000.

A transfer function bends the code value axis so equal *visual* steps
get equal numbers of code values. Mathematically, this concentrates
quantization error where you don't see it.

There's a second motivation, historical but still relevant: CRT
phosphors had a roughly power-law brightness response (γ ≈ 2.2). The
camera's encoding curve was the inverse of that, so the system was
linear end-to-end without any explicit correction step. The "gamma"
that codecs care about is the *encoding* gamma, which is roughly
1/2.2.

Modern displays are LCD/OLED, not CRT, but the encoding curve stayed
because the perceptual benefit outweighed the technical reason. We
just embed the now-redundant CRT gamma in the standard and call it
done.

## 4.2 SDR: sRGB and Rec.709

For SDR content (BT.709 / sRGB / Display P3 etc.), the encoding TF is
approximately a power law γ ≈ 2.4, with a small linear segment at
the low end to avoid a singularity in the derivative at zero (which
would otherwise blow up noise).

### The sRGB curve

```text
sRGB encode (linear light L ∈ [0,1] → encoded V ∈ [0,1]):

  if L ≤ 0.0031308:
      V = 12.92 · L
  else:
      V = 1.055 · L^(1/2.4) − 0.055

sRGB decode (V → L):

  if V ≤ 0.04045:
      L = V / 12.92
  else:
      L = ((V + 0.055) / 1.055)^2.4
```

The exponent 1/2.4 (encode) corresponds to a decoding gamma of 2.4.
The "effective" gamma over the whole curve, including the linear
segment, is roughly 2.2 — which is why you often hear "sRGB gamma 2.2"
as a shorthand.

### The BT.709 curve

Rec.709 (broadcast HD SDR) uses a *slightly* different curve:

```text
BT.709 encode (camera OETF):

  if L < 0.018:
      V = 4.5 · L
  else:
      V = 1.099 · L^0.45 − 0.099
```

And the display side (EOTF) is the *industry-standard* BT.1886 power
law:

```text
BT.1886 decode (display EOTF):
  L = V^2.4
```

The slight mismatch between camera OETF (0.45 ≈ 1/2.22) and display
EOTF (2.4) is intentional and corresponds to a slight S-curve in the
end-to-end system, which mastering engineers prefer for the way it
darkens midtones and adds perceived contrast.

For codec purposes, all of this is *invertible math the decoder doesn't
have to do*. Stored code values are TF-encoded; the decoder produces
the same TF-encoded values; the display reverses it. The codec just
moves bits.

But: bit depth requirements depend on the TF. SDR power-law curves
distribute code values reasonably well over the SDR luminance range,
so 8-bit Y stores SDR content without obvious banding. The HDR curves
below need more bits.

## 4.3 HDR: the PQ curve (BT.2100, ST 2084)

PQ — **Perceptual Quantizer** — was designed by Dolby (Scott Miller)
specifically to put code values where banding would otherwise be
visible across a 0–10,000 cd/m² luminance range. It's defined in
[ST2084] and adopted in [BT2100].

### The PQ EOTF (display)

```text
PQ EOTF (V ∈ [0,1] → L in cd/m²):

  L = 10000 · ((max(V^(1/m2) − c1, 0)) / (c2 − c3 · V^(1/m2)))^(1/m1)

  m1 = 2610 / 16384            = 0.1593017578125
  m2 = 2523 / 4096 · 128       = 78.84375
  c1 = 3424 / 4096             = 0.8359375
  c2 = 2413 / 4096 · 32        = 18.8515625
  c3 = 2392 / 4096 · 32        = 18.6875
```

The inverse is what the encoder applies to map cd/m² → V.

### Key properties of PQ

- **Absolute mapping.** Code 0 means 0 cd/m². Code 1023 means 10,000
  cd/m². Code 512 always means roughly 92 cd/m², regardless of who
  encoded it. This is the opposite of SDR's *relative* mapping.

- **Perceptually uniform.** The curve is designed so equal code value
  steps correspond to equal visibility-threshold steps in human vision
  (Barten's contrast sensitivity function). At any luminance level,
  the next code value is just *barely* visibly brighter under typical
  viewing.

- **Wide range.** 10-bit PQ covers 0–10,000 cd/m². For reference, a
  typical office is around 300 cd/m² ambient; a cloudless sunlit
  cloud is ~30,000 cd/m².

- **Requires 10-bit minimum.** At 8-bit, even PQ's perceptual
  distribution shows banding in dark regions. Almost all HDR content
  is 10-bit; cinema masters are 12-bit.

### Worked example: PQ code value → luminance

What luminance does V = 0.5 represent under PQ?

```text
V^(1/m2) = 0.5^(1/78.84375) ≈ 0.5^0.01268 ≈ 0.9913
numerator = max(0.9913 − 0.8359, 0) = 0.1554
denominator = 18.8515 − 18.6875 · 0.9913 = 18.8515 − 18.5252 = 0.3263
ratio = 0.1554 / 0.3263 = 0.4763
L = 10000 · 0.4763^(1/0.1593) = 10000 · 0.4763^6.277 ≈ 10000 · 0.00922
  ≈ 92.2 cd/m²
```

So V = 0.5 (mid-range in PQ) corresponds to ~92 cd/m², which is roughly
peak SDR white. The upper half of the PQ range is *all* HDR
highlights.

The implication for codec design: bits spent on PQ-encoded highlights
buy a lot of perceptual range, but the quantization matrix interacts
with the TF in non-obvious ways. HDR-aware quantization matrices
([Chapter 9](ch09-quantization.md)) shape error to follow the TF, not
just chroma subsampling.

## 4.4 HDR: the HLG curve (BT.2100, ARIB B67)

HLG — **Hybrid Log-Gamma** — was designed by BBC and NHK to be
*backwards-compatible* with SDR displays. An HLG signal displayed on
a Rec.709 SDR TV looks approximately correct (a little crushed but
recognizable), unlike PQ which looks completely wrong on SDR.

### The HLG OETF (camera)

```text
HLG OETF (scene linear L ∈ [0,1] → V ∈ [0,1]):

  if L ≤ 1/12:
      V = √(3 · L)
  else:
      V = a · ln(12 · L − b) + c

  a = 0.17883277
  b = 0.28466892
  c = 0.55991073
```

The lower half is a square-root curve (close to BT.709 gamma at the
low end); the upper half is logarithmic. The transition at L = 1/12 is
designed for continuity in value and slope.

### The HLG OOTF (system)

HLG's other key property is that it's *scene-referred* rather than
*display-referred*. The camera-side OETF produces V values that
roughly preserve scene relative light. The display-side then applies
an **OOTF** that scales the upper portion of the signal to the
display's actual peak brightness:

```text
HLG OOTF (display):
  L_display = α · L_scene^γ

  γ = 1.2 + 0.42 · log10(L_peak / 1000)
  α normalizes so the output spans the display's range
```

The point: the *same* HLG signal looks correct on a 600-nit display
and a 4000-nit display, with the OOTF adapting. PQ requires either
display-side adaptation (tone-mapping) or per-display masters.

For broadcast (live HDR), HLG's adaptability is a huge advantage. For
streaming and cinema, PQ's absolute reference is preferred because
the mastering engineer's intent is more precisely preserved.

### When to use which?

| Use case                     | Common choice          |
|------------------------------|------------------------|
| Streaming HDR (Netflix, etc.)| PQ + HDR10/+/DV        |
| Live broadcast HDR (sports)  | HLG                    |
| Cinema (Dolby Cinema, IMAX)  | PQ + Dolby Vision      |
| BBC iPlayer / Japanese broadcast | HLG               |
| Camera production            | Either, transcoded later |

## 4.5 HDR formats: HDR10, HDR10+, Dolby Vision, HLG10

The TF is only part of the HDR story. The full delivery format
combines:

1. A color space (almost always BT.2020).
2. A TF (PQ or HLG).
3. **Metadata** describing the mastering environment and content
   characteristics, so downstream tools can tone-map appropriately.

| Format        | Carrier             | Metadata                                       | Used by                          |
|---------------|---------------------|------------------------------------------------|----------------------------------|
| HDR10         | BT.2020 + PQ        | Static, per-stream: MaxCLL, MaxFALL, MDCV      | Universal baseline               |
| HDR10+        | BT.2020 + PQ        | Dynamic, per-scene: tone-mapping curves        | Samsung, Amazon                  |
| Dolby Vision  | BT.2020 + PQ        | Dynamic, per-frame RPU; multiple profiles      | Dolby ecosystem                  |
| HLG10         | BT.2020 + HLG       | None required (HLG is display-adaptive)        | Broadcast                        |

### The metadata in HDR10

Three small numbers carried in SEI messages or container-level fields:

- **MaxCLL** (Maximum Content Light Level): the brightest single pixel
  in the whole stream, in cd/m². Tells the renderer's tone-mapper "no
  point in preparing for >X nits."
- **MaxFALL** (Maximum Frame-Average Light Level): the brightest *frame
  average* in the stream. Useful for adaptive backlighting.
- **MDCV** (Mastering Display Color Volume): the (x,y) primaries and
  min/max luminance of the *display the content was mastered on*.
  Tells the renderer the creative intent's reference environment.

A static HDR10 stream with no MaxCLL/MaxFALL still decodes correctly,
but renderers have to guess at tone-mapping. This is why some HDR
streams look great on one TV and oversaturated on another.

### HDR10+ and Dolby Vision

Static metadata is one set of numbers for the entire stream. Dynamic
metadata is *per-scene* (HDR10+) or *per-frame* (Dolby Vision),
allowing the mastering engineer to set scene-specific tone-mapping
intent.

Mechanically:

- **HDR10+** carries SMPTE ST 2094-40 metadata in SEI messages. The
  Bezier-shape tone-mapping curve gets specified per scene.
- **Dolby Vision** is more invasive — there are 9 profiles spanning
  base-layer-only and dual-layer arrangements, with the RPU (Reference
  Picture Unit) metadata carried as a NAL unit. Profiles 4 (single-
  layer HDR10 + dynamic metadata), 5 (no base layer, IPT-PQ), 8.1
  (HDR10 + RPU) are the streaming-common ones.

The pixels are the same as HDR10 for these. The metadata is what
differs. **Dropping the metadata still gives you a valid HDR10
stream**, just without per-scene tone-mapping. That's a useful
property when transcoding: if your downstream pipeline doesn't
understand HDR10+, strip the SEIs and you have HDR10.

## 4.6 Tone mapping

When you have HDR content but only an SDR display — by far the most
common case — you must *tone-map*: compress the 10,000 cd/m² range to
the ~100–600 cd/m² SDR display can show.

The challenge is that naive linear scaling crushes highlights to black
and creates a flat image. Tone-mapping operators are nonlinear curves
designed to preserve highlight detail and mid-tone contrast.

### Standard tone-mapping curves

- **Reinhard.** `L' = L / (1 + L/Lmax)`. The simplest. Asymptotically
  saturates at L'_max = 1 as L → ∞. No shoulder, no toe — gentle but
  flat-looking.

- **Hable / Uncharted 2.** Filmic curve with explicit toe (low
  end, slowly rising from black), linear midsection, and shoulder
  (high end, slowly rolling off to white). Several free parameters for
  the curve shape. Used in the game industry.

- **ACES (RRT + ODT).** Industry-standard cinema curve, parameterized
  by the target display. Mathematically a piecewise function with a
  proprietary-looking shape but reverse-engineered free
  implementations exist.

- **BT.2390.** ITU's recommendation for HDR-to-SDR mapping. Specifies
  a parametric knee curve with display-dependent parameters. The
  current "right" answer for broadcast.

### Display capability matching

Even within HDR, you tone-map. A stream mastered on a 4000-nit
reference display, played on a 1000-nit consumer panel, must be
mapped. MDCV metadata says "I was mastered on N nits"; MaxCLL says
"the brightest pixel is M nits"; the renderer combines these with
its panel's peak to compute a knee curve.

In practice, hardware tone-mappers (every HDR TV has one) use the
MDCV + MaxCLL + their own panel characteristic, and the result varies
significantly across TV manufacturers. This is why the HDR10+ and DV
mastering engineers ask for per-scene curves: it constrains the TV's
tone-mapper to something closer to the creative intent.

## 4.7 The ACES pipeline (for context)

ACES — Academy Color Encoding System — is the industry pipeline for
preserving color *intent* end-to-end through production, grading, and
delivery. Codecs don't directly speak ACES, but the codec engineer
encounters it on the production side. The pipeline:

```text
camera RAW
   │
   ▼  IDT (Input Device Transform) — per-camera matrix to ACES2065-1
   │
ACES2065-1 (AP0 working space)
   │
   ▼  optional LMT (Look Modification Transform) — creative grade
   │
ACES AP1 working space (sometimes called ACEScg or ACEScc)
   │
   ▼  RRT (Reference Rendering Transform) — picture rendering
   │
OCES (Output Color Encoding Specification)
   │
   ▼  ODT (Output Device Transform) — per-display
   │
delivery in Rec.709 SDR / BT.2100 HDR / DCI-P3 cinema
```

The ACES master is in ACES2065-1 (AP0 primaries, ACES_2065-1 TF — a
small linear segment plus a 16-bit "ACEScc" log curve in working
space). Grading happens in AP1. The final deliverable is rendered out
through the appropriate ODT for the target.

The codec sees only the final output — Rec.709 SDR or BT.2100 HDR
encoded as YCbCr in a container. But understanding that the master
came from ACES tells you the upstream pipeline's color intent, and
explains why certain tone-mapping curves look "right" — they're
implementations of the ACES RRT.

## 4.8 Putting TF + primaries + matrix together

Every video stream is specified by three (technically four — also
the YCbCr matrix) independent choices:

```text
colour_primaries    + transfer_characteristics + matrix_coefficients
       ↓                       ↓                          ↓
   "what gamut?"            "what curve?"            "how to YCbCr?"
```

The standard combinations:

| Stream                 | Primaries  | TF      | Matrix     |
|------------------------|-----------|---------|------------|
| SDR HD streaming/broadcast | BT.709    | BT.709  | BT.709     |
| SDR UHD                | BT.2020   | BT.709  | BT.2020 NCL |
| HDR10                  | BT.2020   | PQ      | BT.2020 NCL |
| HLG broadcast          | BT.2020   | HLG     | BT.2020 NCL |
| Legacy SD              | BT.601    | BT.709 or BT.601 | BT.601 |

Mixing them is fine *as long as the bitstream says so*. BT.2020
primaries with BT.709 TF is "wide-gamut SDR" — perfectly valid and
sometimes used. The decoder applies the inverse TF and inverse matrix
based on the signaled values, not on assumptions.

## 4.9 What the codec actually carries

The codec layer doesn't apply the TF. It carries TF-encoded values
through prediction, transform, quantization, and entropy coding. The
TF is *the* nonlinearity that determines what the codec sees:

- Quantization steps in TF-encoded space are *not* uniform in linear
  light. PQ-encoded values have small linear-light steps in shadow
  (where the eye notices error) and large linear-light steps in
  highlights (where the eye doesn't).
- This is why HDR encoding works at the same bitrates as SDR for the
  same perceptual quality, despite carrying ~100× the luminance range.
  The TF is doing the heavy lifting.
- The codec just needs to know which TF is in use, so its
  quantization matrix and rate control can be tuned for HDR if needed
  ([Chapter 9](ch09-quantization.md), [Chapter 13](ch13-rate-control.md)).

## 4.10 Further reading

- **[BT2100]** — the ITU recommendation that defines PQ and HLG
  together. Short, readable. The reference.
- **[ST2084]** — the SMPTE document that originally defined PQ.
- **[ARIBB67]** — the ARIB document that originally defined HLG.
- **[Poynton2012]** Chapters on gamma and transfer functions. The
  derivation of the BT.709 OETF/EOTF from first principles is here.
- **[BT709]** — for the SDR side, the canonical document.

For an engineer's introduction, the **Dolby Labs HDR White Papers**
(search "Dolby HDR white paper") and the **ITU-R BT.2390** report on
HDR-to-SDR mapping are practical, vendor-published explanations that
go into more depth on the rendering side than this chapter does.

The **ACES website** (search "ACES Academy Color Encoding") hosts the
authoritative ACES documentation, including all the IDT/RRT/ODT
shaping curves.

## 4.11 Exercises

1. Compute the PQ-encoded value V for L = 100 cd/m² (roughly SDR
   peak white). Then compute V for L = 1000 cd/m². How many code
   values (out of 1023, in 10-bit) separate these two? Compare to the
   number of code values separating 1 cd/m² and 10 cd/m².

2. Show that HLG and PQ produce roughly equal visual contrast in their
   lower half, but very different in their upper half. (Compare V vs
   L for both curves at L = 0.05, 0.5, 5, 50 cd/m².)

3. A stream is tagged BT.2020 / PQ / BT.2020-NCL. What does each tag
   tell the decoder, in plain language? What happens if the file
   stripped the `transfer_characteristics` field?

4. An SDR stream is sent to a Rec.2100/PQ-aware renderer with no
   transfer-characteristics signaling, and the renderer interprets the
   stream as PQ. Describe the visible result. (Hint: think about what
   V = 1.0 means under each TF.)

5. *In the codebase.* The ProRes codec doesn't carry HDR metadata
   itself; HDR ProRes carries the same `color_*` fields in the
   container (`moov.trak.mdia.minf.stbl.stsd.icpv.colr`). Use
   `ffprobe` on any HDR ProRes file and read out the four color-space
   fields. Convert the `colour_primaries` integer to a primaries name
   by hand using H.273.

6. The MaxCLL of a stream is 4000 cd/m². You're playing it on a
   1000-nit TV. The TV must tone-map. Sketch a knee curve from MaxCLL
   to peak that preserves shadow detail and rolls off highlights
   gently. (No closed form needed — just the shape.)

---

End of Part I. Next: [Chapter 5 — Pipeline Overview](ch05-pipeline.md).
