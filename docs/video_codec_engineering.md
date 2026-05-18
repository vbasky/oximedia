# Video Codec Engineering — Comprehensive Curriculum

> **Audience.** Anyone who wants to be a *serious* video codec engineer.
> Not "I can integrate FFmpeg" — actually understand what a decoder is
> doing at the bit level, why every modern codec has the shape it does,
> and where the open problems are.
>
> **Companion docs** (all in this repo). This is the map; each topic
> below points at the existing doc that covers it in depth, and fills
> in the gaps where there isn't one yet:
>
> | Existing | Covers |
> |---|---|
> | [`codec_internals.md`](codec_internals.md) | Generic block-coding pipeline, color/chroma, intra/inter prediction, transforms, quantization, entropy coding overview, loop filtering, rate control overview, profiles/levels, audio basics, container vocabulary |
> | [`wire_formats.md`](wire_formats.md) | H.264 NAL framing, RTP, SDP, HTTP Digest auth, CoreFoundation refcounting, NV12, CMTime, VideoToolbox callback model |
> | [`prores_decoder.md`](prores_decoder.md) | A complete decoder walk-through — ProRes 422 from compressed bytes to 10-bit YUV samples, with worked examples for every stage |
> | [`rate_control.md`](rate_control.md) | Rate control deeper |
> | [`codec_status.md`](codec_status.md) | Per-codec honest implementation status |
> | [`ffmpeg_parity.md`](ffmpeg_parity.md) | Strategic gap analysis vs FFmpeg |
> | [`simd_dispatch.md`](simd_dispatch.md) | SIMD strategy in the workspace |
> | [`onboarding.md`](onboarding.md) | First-day reading order |

This doc adds the parts that aren't in any of those: the *theory*
(information theory, RD optimization), the *deep mechanics* (CABAC,
DPB, HDR math), the *specific codec architectures* (H.264 / HEVC /
AV1 / VP9 / MPEG-2 contrasts), and the *production reality* (workflow,
QC, broadcast vs streaming, conformance).

Read this top-to-bottom or jump into a section. Every section is
self-contained.

---

## Part 1 — Information theory and rate-distortion

Compression is the engineering practice of an idea from information
theory: a signal carrying H bits of information per sample cannot be
losslessly compressed below H bits per sample on average. The
**entropy** H is a property of the signal's probability distribution.

### Shannon entropy

For a discrete random variable X with possible outcomes x₁ … xₙ
and probabilities p(xᵢ):

```text
H(X) = -Σ p(xᵢ) · log₂ p(xᵢ)        bits per symbol
```

Intuition: a coin flip (p = ½, ½) has H = 1 bit per flip. A heavily
biased coin (p = 0.99, 0.01) has H ≈ 0.08 bits per flip — you can
encode 1000 flips in 80 bits because runs of heads are predictable.

For video: pixel values are nowhere near uniform. Adjacent pixels are
highly correlated, so the conditional entropy H(pixel | neighbours)
is much smaller than the marginal entropy H(pixel). Every codec is a
machine for exploiting that conditional structure.

### Rate-distortion theory

Lossy compression trades **rate** (bits/sample) against **distortion**
(error between original and reconstructed). The R(D) curve plots the
minimum achievable rate for each distortion level. Real codecs operate
*above* the theoretical R(D) curve — closing that gap is the central
engineering problem.

**The Lagrangian formulation.** Every encoding decision (which mode,
which MV, which quantizer) is a choice with rate cost R and distortion
cost D. The encoder minimizes:

```text
J = D + λ · R
```

λ is the "exchange rate" between bits and distortion. Small λ → spend
bits freely for quality. Large λ → save bits aggressively.

This single equation drives every encoder choice. RDO (Rate-Distortion
Optimization) means: for each candidate, compute J, pick the minimum.

**Bjøntegaard delta.** When comparing two codecs, the metric of choice
is BD-rate: average percentage bitrate savings at equal quality across
a range of operating points. "AV1 gives 30% BD-rate savings over H.264"
means: at any given quality, AV1 uses ~30% fewer bits.

### Three kinds of redundancy

| Kind | Where | Codec mechanism |
|---|---|---|
| Spatial | Adjacent pixels in a frame | Intra prediction + transform + quantization |
| Temporal | Adjacent frames | Inter prediction (motion comp) + reference picture buffer |
| Perceptual | Human vision limits | Chroma subsampling, perceptual quantization, HVS-shaped quant matrices |

Modern codecs (H.264, HEVC, AV1) achieve ~1000× compression by
exploiting all three. Pure-spatial codecs (ProRes, DNxHR, JPEG) only
exploit spatial + perceptual and top out around ~10×.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §0.

---

## Part 2 — Color, pixels, and human vision

Codec engineers spend more time on color than they expect to. Getting
it wrong shows up as washed-out skin, banded skies, or shifted hues —
all bugs that look like decoder bugs but are actually color-space
mismatches.

### Color spaces and primaries

A "color space" is defined by three things:

1. **Primaries**: the (x,y) chromaticities of R, G, B in CIE 1931 space.
2. **White point**: what (x,y) point counts as pure white — usually
   D65 (6504 K) for TV / video, D50 (5003 K) for print.
3. **Transfer function**: how RGB code values map to linear light
   (covered in §4).

Standard sets:

| Space | Primaries | White | Used for |
|---|---|---|---|
| BT.601 | NTSC/PAL TV primaries | D65 | SD broadcast (1953–) |
| BT.709 | Same as sRGB primaries | D65 | HD broadcast / streaming SDR |
| BT.2020 | Wide-gamut "UHD" primaries | D65 | UHD / 4K / HDR |
| DCI-P3 | DCI digital cinema primaries | DCI white (~6300 K) | Digital cinema |
| Display P3 | DCI-P3 primaries | D65 | Apple displays |
| ACES AP0 | "Beyond visible" — primaries outside spectral locus | D60 | ACES working space (master) |
| ACES AP1 | Slightly larger than BT.2020 | D60 | ACES practical working space |

**Why this matters.** The same R/G/B values mean *different colors* in
each space. Encoded BT.709 content displayed as BT.2020 looks
washed-out (the gamut is interpreted as bigger than it is). The
container/bitstream signals which space via VUI fields and SEI
messages; getting these tags wrong is one of the most common color
bugs in production.

### YCbCr derivation

RGB is bad for compression — channels are correlated. Convert to
**YCbCr** (often called YUV, though strictly YUV is the analog
version): luma + two chroma differences. The conversion matrix depends
on the primaries:

```text
BT.709:
  Y  =  0.2126·R + 0.7152·G + 0.0722·B
  Cb = (B - Y) / 1.8556
  Cr = (R - Y) / 1.5748

BT.601:
  Y  =  0.299·R  + 0.587·G  + 0.114·B
  Cb = (B - Y) / 1.772
  Cr = (R - Y) / 1.402

BT.2020:
  Y  =  0.2627·R + 0.6780·G + 0.0593·B
  ...
```

After conversion, Cb and Cr are offset so that 0 maps to 128 (neutral
gray). The Y/Cb/Cr derivation is signaled by the **matrix_coefficients**
field of VUI / H.273.

### Chroma subsampling

Your eye has ~5× more luminance receptors than color receptors. So
codecs throw away color resolution:

| Notation | Y resolution | C resolution | Total samples/pixel |
|---|---|---|---|
| 4:4:4 | full | full | 3 |
| 4:2:2 | full | ½ horizontal | 2 |
| 4:2:0 | full | ½ both | 1.5 |
| 4:1:1 | full | ¼ horizontal | 1.5 |

4:2:0 is universal for streaming and broadcast (-50% data). 4:2:2 is
the broadcast professional standard. 4:4:4 is post-production /
graphics / screen content.

**Sample positions matter.** In 4:2:0, where exactly does the chroma
sample sit relative to the 2×2 luma block? Three conventions exist:
MPEG-1 (centered), MPEG-2 (top-left, standard for H.264 / HEVC /
AV1), DV (top-left). Misalignment causes 1-pixel color shifts on
gradients.

### Bit depth and range

Each component is stored as 8-bit, 10-bit, or 12-bit integers. 10-bit
matters more than just "more colors" — it's the *intermediate
arithmetic precision* that lets gradients reconstruct without
banding. HDR essentially requires 10-bit because PQ's perceptual
quantization concentrates code values where banding would otherwise
be visible.

**Range** is independent of bit depth:

| Range | Y values (10-bit) | Cb/Cr values (10-bit) |
|---|---|---|
| Limited / Video | 64–940 | 64–960 |
| Full / PC | 0–1023 | 0–1023 |

Mixing them shows up as "washed-out" (limited treated as full) or
"crushed" (full treated as limited). Always signal via VUI's
`video_full_range_flag`.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §1.

---

## Part 3 — Transfer functions and HDR

The **transfer function** (TF) maps stored code values to actual
photometric quantities (cd/m² for HDR, normalized 0..1 for SDR). It's
the difference between "scene-referred" (how much light is there?) and
"display-referred" (what should the panel emit?).

### Why TFs aren't linear

Linear light would waste code values: doubling the brightness from
100 cd/m² to 200 cd/m² is the same visual step as 200→400, but in
linear coding that uses twice as many code values. The TF concentrates
codes where the eye notices steps.

### SDR TFs

**sRGB / Rec.709**: roughly a power-law gamma of 2.2 (the actual sRGB
curve has a small linear segment at the bottom):

```text
sRGB encode:
  if linear < 0.0031308:
      encoded = linear · 12.92
  else:
      encoded = 1.055 · linear^(1/2.4) − 0.055
```

That's what gets stored in your PNG / JPEG / SDR video.

### HDR TFs

**PQ (Perceptual Quantizer, BT.2100, SMPTE ST 2084)** — absolute
metric. Code values 0..1023 (10-bit) map to *actual luminance* in
cd/m² from 0 to 10000:

```text
PQ encode (V is normalized [0,1], L is cd/m²):
  Lm = L / 10000                                    ← normalize
  V = ((c1 + c2·Lm^m1) / (1 + c3·Lm^m1))^m2

  c1 = 0.8359375
  c2 = 18.8515625
  c3 = 18.6875
  m1 = 0.1593017578125
  m2 = 78.84375
```

The decoder applies the inverse to recover L. Mapping is absolute:
code 500 always means the same luminance in nits, regardless of who
encoded it.

**HLG (Hybrid Log-Gamma, BT.2100, ARIB STD-B67)** — relative.
Maintains backwards compatibility with SDR for the low half of the
range, then logs into HDR for the upper half:

```text
HLG encode (V is normalized [0,1], scene linear value):
  if scene <= 1/12:
      V = sqrt(3·scene)
  else:
      V = a·ln(12·scene − b) + c

  a = 0.17883277
  b = 0.28466892
  c = 0.55991073
```

HLG is relative — an HLG display shows the upper half scaled to its
peak brightness, so the same HLG signal looks correct on a 600-nit
panel or a 4000-nit panel.

### HDR formats

| Format | Carrier | Metadata |
|---|---|---|
| HDR10 | BT.2020 + PQ | Static (per-stream): MaxCLL, MaxFALL, MaxDisplayMastering |
| HDR10+ | BT.2020 + PQ | Dynamic (per-scene): tone-mapping curves, color volume |
| Dolby Vision | BT.2020 + PQ | Per-frame RPU (Reference Picture Unit) metadata; profiles 4/5/7/8/8.1/9 vary by base layer and metadata |
| HLG10 | BT.2020 + HLG | No metadata needed (HLG is display-adaptive) |

The metadata is **separate from the pixel data**. Dropping HDR10+ or
DV metadata still gives you valid HDR10 output; you just lose
per-scene tone mapping.

### Tone mapping

When you have to show HDR on an SDR display (the most common case),
you must compress the luminance range. **Tone mapping curves:**

- **Reinhard**: `L' = L / (1 + L/Lmax)` — simple, no flat shoulder
- **Hable / Uncharted 2**: filmic curve with shoulder and toe
- **ACES**: industry-standard cinema curve
- **BT.2390**: ITU's HDR-to-SDR recommendation, with a knee curve and
  parameter for target peak

Encoders may embed Mastering Display Color Volume (BT.2100 / SEI) to
help downstream tone mappers do better.

### ACES pipeline

ACES (Academy Color Encoding System) is the industry pipeline for
preserving color intent end-to-end:

```text
camera RAW
   │
   ▼  IDT (Input Device Transform) — per-camera matrix
   │
ACES2065-1 (AP0 working space)
   │
   ▼  LMT (Look Modification Transform) — creative grade
   │
ACES AP1 working space
   │
   ▼  RRT (Reference Rendering Transform) — picture rendering
   │
OCES (Output Color Encoding Specification)
   │
   ▼  ODT (Output Device Transform) — per-display
   │
display-referred output (Rec.709 SDR / BT.2100 HDR / DCI-P3 cinema)
```

Codecs don't usually carry ACES tags directly, but a serious engineer
needs to know that the *grade* happens in ACES space and the final
deliverable is what ends up in your bitstream.

---

## Part 4 — The block-based hybrid pipeline (recap)

Every modern video codec — H.264, HEVC, AV1, VP9, MPEG-2 — uses the
**block-based hybrid pipeline**. It has the same shape across codecs;
they differ only in the specific choices for each stage.

```text
Encoder
─────
   block → predict (intra or inter) → residual → transform → quantize
                                                      │
                                                      ▼
                                                 entropy code → bitstream

   (reconstruct in parallel: dequant → IDCT → +prediction → loop filter
    → store in DPB, exactly as decoder will see it)

Decoder
─────
   bitstream → entropy decode → dequantize → inverse transform → residual
       │                                                              │
       ▼                                                              │
   side info (mode, MV, partitioning)                                  │
       │                                                              │
       └──────────────────── form prediction ───────────────────┐    │
                                                                ▼    ▼
                                                          reconstruct (pred + res)
                                                                │
                                                                ▼  loop filter
                                                                │
                                                                ▼  store in DPB
                                                                │
                                                                ▼  emit frame
```

Each stage has decades of research behind it. The next several parts
do deep dives.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §2.

---

## Part 5 — Intra prediction

For keyframes (and intra-coded blocks within inter frames), there's
no reference frame. Prediction must come from inside the current
frame — from **already-decoded neighboring blocks**.

### H.264 — 9 modes for 4×4

Modes 0–8 cover vertical, horizontal, DC, and six diagonal
directions. The encoder picks per-4×4-block; the decoder gets the
mode index in the bitstream.

### HEVC — 35 modes for variable block size

Same idea, 33 directional + DC + planar, applied to 4×4 / 8×8 / 16×16
/ 32×32 blocks via quad-tree partitioning.

### AV1 — 56+ modes including special cases

- 8 directional modes (V, H, D45, D135, D117, D153, D207, D63)
- DC, Paeth, Smooth, Smooth_V, Smooth_H
- Recursive intra (sub-block prediction from already-decoded
  sub-blocks of the same parent)
- Chroma-from-luma (CfL): predict chroma as a linear function of luma
- Palette mode (for screen content with few unique colors)
- Intra block copy (IBC) — like motion compensation but within the
  current frame (great for screen content)

### Most-probable-mode signaling

The mode index itself costs bits. Codecs predict the predictor: assume
the current block's mode equals the mode of the neighbour above or
left. If correct (frequent for smooth gradients), only a 1-bit "yes"
flag needs to be sent.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §3.

---

## Part 6 — Motion estimation and compensation [DEEP DIVE]

For non-key frames, inter prediction is where most of the bitrate
savings come from. The encoder finds the best-matching block in a
previously decoded frame; the decoder receives the (reference index,
motion vector) and copies that block out as the prediction.

### Sub-pel motion

Real motion isn't whole-pixel. H.264 supports **¼-pixel precision**,
AV1 supports **⅛-pixel**. To fetch at a fractional offset, the decoder
**interpolates** integer-grid pixels:

```text
H.264 luma half-pel: 6-tap filter (1, -5, 20, 20, -5, 1) / 32
H.264 luma ¼-pel:    average of integer and half-pel samples
HEVC luma:           7-tap filter, longer for higher quality
AV1 luma:            multiple filter sets (smooth / sharp / regular)
```

The filter coefficients are specified bit-exactly so every decoder
agrees. Mismatched filters → drift → visual corruption that compounds
across the GOP.

### Motion vector prediction

A motion vector is up to 22 bits (H.264) or more. That's a lot to
transmit per block. Codecs **predict the MV** from neighbouring blocks
and only transmit the *delta*.

| Mechanism | Codec | What it does |
|---|---|---|
| Median predictor | H.264 | Predict MV as median of (left, above, above-right) MVs |
| AMVP | HEVC | Build a list of MV candidates from spatial+temporal neighbours; transmit an index + delta |
| Merge mode | HEVC, AV1 | Copy the MV from a neighbour entirely — no delta at all |
| MV refinement | AV1, H.266 | Decoder-side MV refinement (DMVR), optical-flow-based correction |

In merge mode, the encoded cost is just the merge candidate index (a
few bits) — no MV bits, no reference index bits. For a panning camera
where every block has the same MV, this is essentially free.

### B-frames and bi-prediction

A **B-frame** can predict from two references — typically one past
and one future. The two predictions are then averaged (or weighted
averaged). Bi-prediction gives the best compression because:

1. Smooth motion is captured even more accurately (two predictions
   bracket the true motion).
2. Occlusions are handled gracefully (when one reference predicts
   poorly, the other typically picks up the slack).

**Generalized bi-prediction (GBi)** in HEVC / AV1: instead of 50/50
weighting, the encoder signals one of 5 weight pairs (e.g. ¼-¾,
⅜-⅝). Often gains 0.5–1% BD-rate.

### B-frames and PTS/DTS divergence

To decode a B-frame, the decoder needs both references already. So the
bitstream interleaves frames non-monotonically in time:

```text
Display order:  I  B  B  P  B  B  P  …
Frame number:   0  1  2  3  4  5  6
Decode order:   I  P  B  B  P  B  B  →  0, 3, 1, 2, 6, 4, 5
```

This is why every container carries **PTS** (presentation timestamp,
display order) and **DTS** (decode timestamp, bitstream order). With
no B-frames, PTS = DTS. With B-frames, they diverge.

### Hierarchical B-frames

Modern encoders use **hierarchical B**: a structured layering where
deeper-level B-frames reference other B-frames. Example with hierarchy
depth 3 (8-frame mini-GOP):

```text
Decode order:    I0  P8  B4  B2  B6  B1  B3  B5  B7
                 │   │   │   │   │   │   │   │   │
Layer:           0   0   1   2   2   3   3   3   3
```

- Layer 0 (I, P) is the "anchor" — coded with most bits.
- Layer 1 (B4) references I0 and P8.
- Layer 2 (B2, B6) references the layer below.
- Layer 3 (B1, B3, B5, B7) references the layer-2 frames.

Why hierarchical? **Graceful quality degradation under bandwidth
pressure** — drop layer 3 first (quality goes from 8 fps to 4 fps in
the decoded view, not catastrophic). Used heavily in adaptive
streaming.

### Encoder-side ME search

The *encoder* has to find good MVs. Naive full search at ±32 px in a
1080p frame = millions of candidates per block — far too slow. Real
encoders use:

- **Diamond search**: start at the predicted MV; check 4 candidates
  in a diamond pattern; move to whichever has lowest SAD/SATD; repeat
  until no improvement. Fast but can miss true minimum.
- **Hexagonal search**: 6-point candidate pattern. Slightly slower,
  better quality.
- **EPZS (Enhanced Predictive Zonal Search)**: x264's default.
  Combines candidate prediction (MV neighbours), zonal pattern search,
  and adaptive thresholding. Near-optimal at low cost.
- **TESA / UMHexagonS**: even more elaborate, for x264's "slower"
  presets.
- **Hierarchical search**: search at low resolution first to find
  approximate MV, then refine at full resolution. Allows wider search
  windows cheaply.

The decoder doesn't care which algorithm the encoder used — it just
receives the final MV.

### Affine motion (HEVC SCC, AV1, H.266)

Pure translation can't capture rotation, zoom, or shear. **Affine
motion** uses 2–4 control-point MVs to define a more general transform:

```text
6-parameter affine (3 control points):
  Δx = a + b·x + c·y
  Δy = d + e·x + f·y
```

Used for camera rotation, dolly shots, and CGI panning where strict
translational MVs would split a region into many small blocks.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §4.

---

## Part 7 — Transforms

The forward transform packs the residual's energy into a few low-
frequency coefficients. The inverse transform undoes it.

### DCT-II — the workhorse

```text
1-D N-point DCT-II:
  X[k] = Σ_{n=0..N-1} x[n] · cos((2n+1)·k·π / (2N))
```

For N=8 and image content, ~95% of a block's energy ends up in the
top-left ~16 coefficients. The other 48 quantize to zero. That's the
sparsity entropy coding exploits.

### Integer DCT in H.264 (4×4)

H.264 uses an **integer approximation** of DCT-II for bit-exact
reconstruction:

```text
        ┌                ┐
        │  1   1   1   1 │
   H =  │  2   1  -1  -2 │
        │  1  -1  -1   1 │
        │  1  -2   2  -1 │
        └                ┘

   Y = H · X · Hᵀ                    (residual block X → coefficients Y)
   X = Hᵀ · Y · H                    (decode side: coefficients Y → residual X)
```

Factor-of-2 entries mean the transform isn't quite orthonormal; the
missing scale gets folded into the quantization matrix.

### HEVC and AV1 — more sizes, more transforms

**HEVC** adds 8×8, 16×16, 32×32. The encoder picks per-TU (transform
unit) based on local complexity. Larger transforms compact smooth
regions better; smaller transforms handle edges without ringing.

**AV1** adds rectangular transforms (4×8, 8×16, 16×32) and an
*expanded transform set*:

| Transform | Where |
|---|---|
| DCT | All sizes |
| ADST (Asymmetric Discrete Sine Transform) | One direction, for intra prediction residuals which have asymmetric error distribution |
| FLIPADST | ADST flipped, for residuals from the opposite direction |
| IDTX (Identity transform) | "Don't transform" — for screen content where pixels are already nearly independent |

The encoder picks per-block. Decoder receives the transform-type
index. Up to 16 combinations of (row, col) transforms per block.

### Transform skip

For screen content (computer screens, graphics, text), residuals
already have low spatial correlation — applying DCT actually *hurts*
sparsity. **Transform skip** mode says "just quantize and entropy-
code the residual directly, no transform." HEVC SCC extension and
AV1 support this.

### Hadamard transform

Pure ±1 matrix:

```text
H = (1/√2) · ┌─────┐
            │ 1  1 │
            │ 1 -1 │
            └─────┘
```

Fast — only additions and subtractions, no multiplies. Used:

- For SATD (Sum of Absolute Transformed Differences) in encoder motion
  estimation — a proxy for the post-transform cost.
- In AV1's chroma-from-luma for the luma DC.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §5.
ProRes-specific worked example: [`prores_decoder.md`](prores_decoder.md) §13.

---

## Part 8 — Quantization

The **only** lossy stage. Everything else is invertible.

### Scalar quantization

```text
q[k] = round( Y[k] / step(k) )      ← encoder
Y'[k] = q[k] · step(k)              ← decoder reconstruction
```

Reconstruction error magnitude ≤ step(k)/2. Choose step(k) per
coefficient (higher for high-frequency, where the eye is less
sensitive) — that's the quantization matrix.

### QP and the 6-step-doubling rule

Codecs don't transmit raw step sizes. They transmit a small integer
**QP** (quantization parameter). The mapping is logarithmic — step
size doubles every 6 QP units:

```text
H.264:
  step ≈ 2^((QP - 4) / 6)

  QP  0 → step ~0.625
  QP  6 → step  1.25
  QP 12 → step  2.5
  QP 24 → step 10
  QP 36 → step 40
  QP 51 → step 224         (H.264 max)
  QP 63 → step ~896        (HEVC max, supports 10-bit / 12-bit)
```

Doubling step ≈ halving bitrate ≈ roughly +3 dB PSNR loss. QP is *the*
knob a streaming encoder turns.

### Quantization matrices (revisited)

```text
default H.264 intra luma 8×8:
  6  10 13 16 18 23 25 27
 10 11 16 18 23 25 27 29
 13 16 18 23 25 27 29 31
 16 18 23 25 27 29 31 33
 18 23 25 27 29 31 33 36
 23 25 27 29 31 33 36 38
 25 27 29 31 33 36 38 40
 27 29 31 33 36 38 40 42
```

Big quantizers at high frequency = throw bits away where the eye
doesn't notice. Each codec ships defaults; encoders may signal custom
matrices in the SPS/PPS.

### Dead-zone

Standard rounding rounds 0.4 to 0 and 0.6 to 1. **Dead-zone**
quantizers expand the zero band:

```text
q[k] = sign(Y[k]) · max(0, (|Y[k]| − dead_zone) / step)
```

For dead_zone = step/4, values in (−step/4, +step/4) round to 0 even
if they'd normally round to ±1. Kills low-amplitude noise; modest
quality cost; significant bitrate savings.

### RDOQ — Rate-Distortion Optimized Quantization

Standard quantization is greedy: round each coefficient
independently. **RDOQ** is smarter — it considers the joint cost of
quantizing all coefficients in a block together, using actual
entropy-coding bit costs.

For example: rounding a coefficient up vs down may change the *run-
length* of zeros, which has a non-local effect on bit cost. RDOQ
considers this; standard quantization doesn't.

Typical gain: 5–15% BD-rate at moderate encoder complexity. Default
on in x264, x265, libvpx, libaom.

### Adaptive quantization (AQ)

Vary QP per region of a frame based on visual sensitivity:

- **Variance-AQ**: lower QP in smooth regions (less masking), higher
  QP in high-variance/textured regions.
- **Psy-AQ** (x264): perceptual-vision-tuned variant. Tries to
  preserve texture "look" by biasing toward retaining noise/grain.
- **AQ-Mode 3** (x265): bidirectional — also raises QP in dark
  regions where the eye is more forgiving.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §6,
[`rate_control.md`](rate_control.md).

---

## Part 9 — Entropy coding: the deep dive

This is where the bits get squeezed out. Every other stage just
arranges data for entropy coding to compress.

### The hierarchy

```text
unary < Huffman / VLC < Golomb-Rice < arithmetic / range coding
```

- **Unary**: write n 1s then a 0. Optimal only if p(0) = ½.
- **Huffman / VLC**: integer-bit codes per symbol. Asymptotically
  optimal in the limit but inefficient when probabilities aren't
  powers of ½.
- **Golomb-Rice**: parametric, good for geometric distributions (the
  residual coefficient distribution).
- **Arithmetic / range coding**: fractional-bit precision, approaches
  the entropy bound arbitrarily closely.

→ ProRes-specific Golomb-Rice walk: [`prores_decoder.md`](prores_decoder.md)
§9.

### Arithmetic coding — the underlying idea

Encode a sequence of symbols as a single (fractional) number in [0, 1).
For each symbol, narrow the interval proportional to the symbol's
probability:

```text
interval = [0, 1)
encode 'A' (p=0.6):  interval = [0.0, 0.6)
encode 'B' (p=0.3):  interval = [0.0, 0.6) × proportions →
                     interval = [0.36, 0.54)
encode 'A' (p=0.6):  interval = [0.36, 0.54) × [0.0, 0.6) →
                     interval = [0.36, 0.468)
...
```

After encoding all symbols, transmit *any* number in the final
interval. Smaller-probability sequences → narrower intervals → more
bits needed to identify them. Bits-per-symbol approaches Shannon's
H(X) asymptotically.

### Range coding (the integer version)

Real implementations use **range coding** — integer-only arithmetic
coding with periodic *renormalization* to keep precision finite.

```text
state: (low, range)        ← represents the current interval [low, low+range)

for each symbol:
    sub_range = range · cumulative_freq[symbol] / total_freq
    low += sub_range_below_symbol
    range = sub_range_of_symbol

    while range < threshold:
        emit top bit of low
        shift low and range left by 1                ← renormalization
```

Why renormalize? Without it, range shrinks to zero quickly. By
emitting the top bit (which is now "decided") and left-shifting,
range grows back to working precision.

Range coder is the entropy core of AV1 / VP9 / Daala. Less context-
heavy than CABAC; more SIMD-friendly.

### CABAC — H.264 Main+, HEVC

**Context-Adaptive Binary Arithmetic Coding.** The most successful
entropy coder in modern video. Three pieces:

#### (1) Binarization

Every syntax element is mapped to a sequence of binary bits. Common
binarizations:

| Binarization | Use |
|---|---|
| Truncated unary (TU) | Bounded small integers |
| Truncated Rice (TR) | Coefficient magnitudes |
| k-th order exp-Golomb (EGk) | Large magnitudes after TR prefix |
| Fixed length (FL) | Mode indices |

A typical syntax element like `coeff_abs_level_minus1` uses a TR
prefix and an EG3 suffix.

#### (2) Context modeling

For each bit position of each binarized syntax element, there's a
**context** — a state that tracks the probability of that bit being 0
vs 1 *in this position*. The context is selected based on neighbouring
syntax (e.g., context for `mb_type` depends on the `mb_type` of left
and above neighbours).

The probability is stored as a 6-bit state index pointing into a
64-entry table (`m_LPS_state`) of (MPS=most-probable-symbol, LPS
probability) pairs.

#### (3) The arithmetic coder

Standard range coder operating on binary symbols only (so the
"alphabet" is just {0, 1} weighted by the context's MPS/LPS
probability). On each bit:

```text
range_LPS = range × p_LPS              ← lookup table indexed by current state
                                         and 4 high bits of range
if symbol == MPS:
    range = range − range_LPS
else:
    low += (range − range_LPS)
    range = range_LPS

update_state(symbol)                   ← state machine: see m_next_state_LPS,
                                          m_next_state_MPS tables
renormalize()                          ← emit decided bits, shift left
```

The state machine on probability updates is the heart of the
adaptation — observing an MPS reduces the LPS probability; observing
an LPS increases it. Convergence after ~10–20 symbols.

#### CABAC bypass mode

For uniform-distribution bits (like signs), the context is fixed at
50/50, and the encoding/decoding skips the state machine. Faster than
regular mode. Used for sign bits, suffix bits of large coefficients,
some mode flags.

#### Slow start: initialization

Each slice resets all contexts to standardized initial values. The
init tables are indexed by SliceQP (which gives a starting probability
distribution shaped for that slice's expected statistics). H.264 has
~460 contexts; HEVC has ~150 contexts.

### AV1's range coder

AV1 simplified arithmetic coding from H.264/HEVC's CABAC:

- Range coder (multi-symbol) instead of binary CABAC
- Probability updated via *integer Laplace smoothing*, not the
  state-machine table approach
- More SIMD-friendly (fewer dependencies between operations)
- Less context — AV1 typically has ~5× fewer context bins than HEVC

Decoder complexity is one of AV1's selling points vs HEVC.

---

## Part 10 — Loop filtering

The reconstruction at this point has **block-boundary artifacts** —
discontinuities where independent 8×8 / 16×16 / 64×64 blocks meet.
Loop filters smooth these *inside the prediction loop* (hence "loop"),
so that future frames predict from the *filtered* output. Without
this, error propagates and compounds.

### Deblocking (H.264, HEVC)

For each block edge, sample 4 pixels each side:

```text
horizontal edge:
                       │
   p3  p2  p1  p0      │     q0  q1  q2  q3
       (block A)       │       (block B)
                       │
                       ↑ filter operates on these eight samples
```

Decide based on local activity and QP whether the discontinuity is a
real edge or coding artifact. If artifact, smooth across with a
low-pass FIR. If real edge, leave alone.

**Boundary Strength (BS)** parameter — controls filter strength:

- BS 0: skip (likely real edge, weak QP)
- BS 1: weak filter
- BS 2-4: progressively stronger
- HEVC: BS computed from MV difference, ref index difference,
  intra/inter status, transform skip flag.

### SAO — Sample Adaptive Offset (HEVC)

Post-deblock per-CTU pixel-level tweak:

- **Band Offset (BO)**: encoder picks 4 consecutive 8-value bands and
  signals an offset for each. Decoder shifts samples whose values
  fall in those bands. Fixes banding artifacts.
- **Edge Offset (EO)**: classify each sample's neighborhood as
  peak/valley/monotonic/etc. Add offset based on classification.
  Softens directional edges.

### CDEF — Constrained Directional Enhancement Filter (AV1)

Replaces SAO. Picks the dominant edge direction in an 8×8 block, then
filters *along* the direction (preserves edges) while smoothing
perpendicular noise. Strength signaled in the bitstream.

### Loop restoration (AV1)

Final pass. Two options per 256×256 region:

- **Wiener filter**: optimal linear filter learned from the encoder's
  RD search. 7×7 separable.
- **Self-guided filter**: edge-preserving, parameters computed from
  the decoded frame itself.

Closes the remaining quality gap to the original. Adds ~5% decoder
complexity for ~3% BD-rate gain.

### Film grain synthesis (AV1, H.273)

Some content (cinema-grade film) has visually important *grain*.
Compressing it destroys the grain (it looks like noise to the
encoder); the result looks "too clean."

Solution: strip the grain in the encoder, transmit a small
*parametric model* of the grain (AR coefficients + scaling table),
then **re-synthesize** grain in the decoder *after* loop filtering.

AV1 normative section 7.20; H.273 SEI message. Used by Netflix for
grain-heavy content.

→ Existing depth: [`codec_internals.md`](codec_internals.md) §8.

---

## Part 11 — Reference picture management (DPB)

The **Decoded Picture Buffer** is the small set of past frames the
decoder keeps around for use as references. Critical and often
misunderstood.

### POC — Picture Order Count

Each picture has a POC value indicating its *display order*. The
codec specifies:

- H.264: explicit POC LSB transmitted; the decoder maintains MSB
  state with wrap detection.
- HEVC: like H.264 but with more flexibility.
- AV1: simpler — display order is a per-frame field.

PTS for the frame = POC scaled to the timebase.

### Reference picture lists (L0, L1)

For B-frames especially, the decoder builds two reference picture
lists:

- **L0** (list 0): typically "past" references (lower POC).
- **L1** (list 1): typically "future" references (higher POC).

A B-frame's MV is `(L0_ref_idx, L0_mv)` and `(L1_ref_idx, L1_mv)`.

List construction is codec-specific:

| Codec | Mechanism |
|---|---|
| H.264 | Reference picture list reordering (RPLR) syntax + default order |
| HEVC | RPS (Reference Picture Set): each picture explicitly lists the references in its set |
| AV1 | Up to 7 references via per-frame ref_frame_idx[] |

### DPB sizing

The DPB has a maximum size (typically 4–16 frames). When full, an old
reference is **bumped out** via:

- Sliding-window MMCO (H.264 / HEVC): oldest reference dropped first.
- Explicit MMCO commands: encoder signals to drop specific frames.
- "Long-term reference" marking: a reference can be designated as
  long-term and persists until explicitly removed.

The DPB size cap is implied by profile/level. H.264 Level 4.1 (1080p)
allows 4 references; Level 5.1 (4K) allows 5; HEVC tier maxes around
16.

### Why this matters for decoder bugs

DPB bugs are the *worst* class of decoder bug. Symptom: video looks
fine at first, then progressively corrupts. Causes:

- Wrong reference picked from L0/L1 → wrong prediction → cascading
  error.
- Lost long-term reference → can't reconstruct future P-frames.
- DPB never bumped → references stale data from before a scene cut.

Conformance test suites are heavy on DPB stress cases.

---

## Part 12 — Rate control (production-grade)

→ Existing depth: [`codec_internals.md`](codec_internals.md) §9,
[`rate_control.md`](rate_control.md).

Highlights for the serious engineer:

### CBR, VBR, CRF revisited

- **CBR**: hit a fixed bitrate over short windows. Bumpy quality. Used
  for satellite, broadcast, live streaming with strict caps.
- **VBR**: hit an average target; bursts allowed. Used for VOD with
  bandwidth budget but quality priority.
- **CRF (Constant Rate Factor)**: hit a fixed *quality*. Bitrate
  varies entirely with content. Default for archival / VOD.

### HRD / CPB / VBV — the buffer model

Every codec defines a **Hypothetical Reference Decoder** model:

- A bitstream buffer (CPB / VBV) with a known size, filled at the
  channel rate.
- Each frame removes bits = its size from the buffer at its DTS.
- The encoder must guarantee the buffer never **underflows** (no bits
  to decode the next frame) or **overflows** (frame too big to fit).

Real-time encoders track CPB occupancy frame-by-frame; if it gets
near empty, they reduce per-frame bits (raise QP); if near full, they
spend more.

### Lookahead

Better encoders see N frames into the future before committing to the
current frame's QP. With lookahead:

- Allocate more bits to frames that will be referenced heavily by
  future frames.
- Recognize a scene cut before reaching it; allocate enough bits for
  the new I-frame.
- Mb-tree (x264): propagate "future reference cost" backward; lower
  QP for blocks that will be heavily referenced.

Lookahead frames cost latency and memory. ~24–60 frames typical for
VOD; ~0 for live.

### Two-pass encoding

For non-realtime workflows, **two-pass** dramatically improves
quality:

1. Pass 1: encode with rough QP, record per-frame statistics
   (complexity, MV usage, intra costs).
2. Pass 2: redistribute the *exact same total* bitrate across frames
   based on Pass 1's data — give bits to the hard frames, save on the
   easy ones.

x264, x265, libvpx, libaom all support two-pass. Used by every
Netflix / YouTube / Apple-encoded VOD asset.

---

## Part 13 — Codec architecture deep dives

### H.264 / AVC — the workhorse

- **Block size**: 16×16 macroblock, partitioned to 16×8, 8×16, 8×8,
  8×4, 4×8, 4×4 sub-blocks. Inter prediction at any of these sizes.
- **Transform**: 4×4 integer DCT for residuals; 8×8 added in High
  profile.
- **Entropy**: CAVLC (Baseline) or CABAC (Main/High).
- **Profiles**: Baseline (no B, no CABAC, no FMO/ASO), Main (full
  feature set), High (8×8 transform, custom quant matrices), High10
  (10-bit), High 4:2:2, High 4:4:4 Predictive (lossless mode).
- **NAL structure**: SPS, PPS, IDR slice, non-IDR slice, SEI, AUD,
  filler data. → [`wire_formats.md`](wire_formats.md) §1.
- **First widely-deployed codec to support B-frames + CABAC + ¼-pel
  motion**. ~50% better than MPEG-2 at the same quality.

### HEVC / H.265 — the evolutionary upgrade

- **Coding Tree Unit (CTU)**: 64×64 (configurable down to 16×16).
  Recursively split into Coding Units (CUs) via a quad-tree.
- **CU**: leaf of the quad-tree. Each CU has Prediction Units (PUs)
  and Transform Units (TUs).
- **PU partitioning**: 2N×2N, N×N, 2N×N, N×2N, plus 4 asymmetric
  modes (nL×2N, nR×2N, 2N×nU, 2N×nD).
- **TU partitioning**: separate quad-tree, sizes 4×4 to 32×32.
- **35 intra prediction modes**.
- **Reference picture sets** instead of MMCO.
- **Tiles**: rectangular regions of CTUs that decode independently
  (parallelism).
- **Wavefront parallel processing (WPP)**: each row of CTUs depends
  only on the row above, after the first 2 CTUs.
- **~50% bitrate savings vs H.264** at same quality. Patent-licensed
  by MPEG-LA, Via LA, and Access Advance pools — three separate pools.

### AV1 — the modern royalty-free leader

- **Superblock**: 128×128 (or 64×64). 10-way partition tree
  (horizontal/vertical/T-split etc.) — even more flexible than HEVC's
  quad-tree.
- **Transform set**: DCT, ADST, FLIPADST, IDTX × {horizontal,
  vertical} = up to 16 combinations.
- **Compound prediction**: weighted average of two predictions with
  flexible weights and masks (wedge, smooth, segmentation-based).
- **Recursive intra**, **chroma-from-luma**, **palette mode**, **intra
  block copy**.
- **CDEF + Wiener / self-guided loop restoration** in place of
  deblock + SAO + ALF.
- **Film grain synthesis** normatively defined.
- **Frame super-resolution**: encode at lower res, signal upscale
  filter.
- **Range coder** entropy (simpler than CABAC).
- **~30% better than HEVC** at moderate decoder cost. Royalty-free
  through the AOM.

### VP9 — the AV1 predecessor

- 64×64 superblocks with 4-way recursive partitioning.
- Simpler than AV1 (no compound prediction modes, fewer transforms).
- Range coder entropy.
- Used heavily by YouTube before AV1 deployment.

### MPEG-2 / H.262 — still relevant

- 8×8 DCT, 16×16 macroblocks, ¼-pel motion (in field/frame modes).
- Two-pass VLC entropy.
- I, P, B frames; no flexible partitioning.
- The format of **every DVD, Blu-ray (alongside H.264), ATSC 1.0
  broadcast, cable digital TV**.
- Patents *fully expired* — anyone can implement freely now.

### H.266 / VVC — the future

- Up to 128×128 CTU, deeper partitioning trees (quad-tree + binary
  tree + ternary tree).
- 67 intra prediction modes; multi-reference-line prediction.
- Decoder-side MV refinement (DMVR), bi-directional optical flow
  (BDOF), prediction refinement with optical flow (PROF).
- Adaptive loop filter, cross-component ALF.
- ~50% bitrate savings over HEVC.
- Standardized 2020; deployment slow because of complexity.

### Intra-only codecs

- **ProRes** (Apple) — 4:2:2 10-bit, every frame independent. →
  [`prores_decoder.md`](prores_decoder.md).
- **DNxHR / DNxHD (VC-3)** — Avid intermediate. Structurally very
  similar to ProRes. Profiles: LB / SQ / HQ / HQX / 444.
- **JPEG 2000** — wavelet-based intra-only. Used in digital cinema
  (DCI) and for high-end mastering.
- **Cineform** — GoPro. Wavelet-based. Open-sourced.
- **FFV1** — open-source lossless. Used by archival institutions.
- **Daala / Thor** — research codecs that fed into AV1.

---

## Part 14 — Audio coding deep dive

→ Existing depth: [`codec_internals.md`](codec_internals.md) §11
(brief).

Video engineers often pretend audio doesn't exist. Don't be that
engineer.

### PCM — the baseline

Raw sampled audio: amplitude values at the sample rate. Lossless,
incompressible by definition. 44.1 kHz × 16-bit × 2 channels = 1.4
Mbit/s. Everything else is built on this.

### Time → frequency: MDCT

Modern audio codecs work in the frequency domain. **MDCT (Modified
DCT)** splits the time signal into overlapping windows (e.g. 1024
samples), each transformed to 512 frequency coefficients.

The overlap is critical — adjacent MDCT windows overlap by 50%, and a
windowing function (sine or Kaiser-Bessel-Derived) tapers the edges
so that perfect reconstruction is possible after inverse MDCT +
overlap-add. **TDAC (Time-Domain Aliasing Cancellation)** is the
math that makes this work.

```text
Forward MDCT (N samples, output N/2 coefficients):
  X[k] = Σ_{n=0..N-1} x[n] · w[n] · cos(π/(N/2) · (n + (N/2+1)/2) · (k + 1/2))
```

Window types matter — a stationary tone uses a long window (1024
samples) for fine frequency resolution; a transient (drum hit) uses
short windows (128 samples) to avoid pre-echo.

### Psychoacoustic model

Two phenomena make compression possible:

1. **Frequency masking**: a loud tone at 1 kHz makes nearby
   frequencies (≈ 950–1100 Hz) inaudible. The "critical band" is the
   region masked; the **Bark scale** parameterizes critical bandwidth
   as a function of frequency.
2. **Temporal masking**: a loud sound masks quieter sounds before (~5
   ms) and after (~50–100 ms) it. "Pre-masking" and "post-masking".

The encoder's psychoacoustic model computes, per MDCT frame and per
frequency band, the **masking threshold** — the maximum quantization
noise that will go unnoticed. Bits are allocated to keep noise below
the threshold everywhere.

Different codecs use different psychoacoustic models — Fraunhofer's
MP3 model, the LAME model (open-source MP3), AAC's PE / NMR-based
allocation, Opus's CELT model. These models drive subjective
quality; codec quality differences at low bitrates are largely
psychoacoustic-model differences.

### AAC — Advanced Audio Coding

ISO/IEC 14496-3. Successor to MP3, ~30% more efficient.

**AAC-LC** (Low Complexity):

- MDCT with 1024-sample long windows / 128-sample short windows.
- 1024 frequency lines per long frame, organized into **scalefactor
  bands** (~50 bands per long frame, grouped by Bark width).
- Per-band scalefactor + Huffman-coded quantized coefficients.
- **TNS (Temporal Noise Shaping)**: applies an LPC filter in the
  frequency domain to shape quantization noise temporally — reduces
  pre-echo.
- **M/S stereo coding** (per band): code mid (L+R)/√2 and side
  (L−R)/√2 if more efficient.
- **Intensity stereo**: at high frequencies, code only one channel +
  per-band stereo scale.

**HE-AAC v1** (High Efficiency):

- AAC-LC base coding the low frequencies.
- **SBR (Spectral Band Replication)**: high-frequency content not
  coded explicitly. Instead, the decoder *replicates* it from the
  decoded low-band, with side info (envelope shape + noise levels).
- Halves bitrate at the cost of high-frequency exactness.

**HE-AAC v2**:

- HE-AAC v1 + **PS (Parametric Stereo)**: stereo image coded as a
  monaural signal + side information (IID, IPD, ICC).
- Further halving for stereo content at very low bitrates.

### Opus — the modern leader

RFC 6716. Combines two coding modes:

- **SILK** (from Skype): linear-prediction speech coder, good at
  6–24 kbps speech.
- **CELT** (Constrained Energy Lapped Transform): MDCT-based music
  coder.

The encoder switches modes or uses both layered, transparently.
Frame sizes from 2.5 to 60 ms (very low latency possible). Bitrates
from 6 kbps (speech) to 510 kbps (multichannel music).

Royalty-free, lower latency than AAC, better quality below 32 kbps.
WebRTC's standard codec.

### AC-3 / E-AC-3 (Dolby Digital)

ATSC mandatory audio. ~3 kHz frequency resolution via 256-line MDCT,
hybrid block-switching for transients. Bit-allocation by approximate
psychoacoustic model. 5.1 standard. E-AC-3 extends to 7.1+ with
higher bitrate ceilings.

Patents largely expired ~2017.

### Lossless audio: FLAC, ALAC

Same trick: LPC prediction + Rice entropy coding of residuals.

```text
Encoder:
  fit LPC[N] coefficients to a frame of samples
  compute residual r[n] = sample[n] − Σ LPC[k] · sample[n-k]
  entropy-code r[n] with Rice code

Decoder:
  reconstruct sample[n] = Σ LPC[k] · sample[n-k] + r[n]
```

Typical residual magnitude is much smaller than sample magnitude →
fewer bits. Compression ratios 50–70% on music. Bit-exact reconstruction.

### Speech codecs (CELP family)

For speech-only at very low bitrate (≤16 kbps): **CELP (Code-Excited
Linear Prediction)**. Models the vocal tract as an LPC filter; the
excitation signal that drives the filter is selected from a codebook.

Variants: ACELP (AMR-NB, AMR-WB, G.722.2), CS-ACELP (G.729). Optimized
for human speech; awful on music.

---

## Part 15 — Container deep dives

→ Existing depth: [`codec_internals.md`](codec_internals.md) §12.

### ISO Base Media File Format (ISOBMFF) — the family

MP4, MOV, 3GP, HEIF, CMAF are all **ISOBMFF**: a hierarchical tree of
"boxes" (also called "atoms"). Each box is `<size:32><type:fourcc>
<payload>`.

```text
Box hierarchy of a typical .mp4:

  ftyp                              ← file type (e.g. 'isom', 'mp42')
  moov                              ← metadata container
    mvhd                              ← movie header (timescale, duration)
    trak                              ← per-track metadata
      tkhd                              ← track header (id, duration)
      mdia
        mdhd                              ← media header (track timescale)
        hdlr                              ← media handler ('vide', 'soun')
        minf
          stbl                            ← sample table
            stsd                            ← sample description (codec config)
              avc1 / hvc1 / av01 / mp4a       ← per-codec entry
                avcC / hvcC / av1C / esds       ← codec-specific config
            stts                            ← decoding time-to-sample
            ctts                            ← composition time offset (PTS-DTS)
            stsc                            ← sample-to-chunk mapping
            stsz                            ← sample sizes
            stco / co64                     ← chunk offsets
            stss                            ← sync-sample table (keyframes)
    udta                              ← user data (metadata, tags)
  mdat                              ← raw media data (samples concatenated)
```

To get a sample's bytes:

1. Look up its index in `stts` to get its DTS (and `ctts` for PTS).
2. `stsc` says which chunk it belongs to and its position within.
3. `stco` gives the chunk's file offset.
4. `stsz` gives the sample's size.
5. Read those bytes from `mdat`.

**Fragmented MP4 (fMP4)** rearranges the file so chunks of samples
can be appended on the fly:

```text
ftyp
moov                          ← still here, but stsz/stco are empty/placeholder
moof                          ← movie fragment box
  mfhd                          ← fragment sequence number
  traf                          ← track fragment
    tfhd / tfdt / trun           ← per-fragment sample sizes + offsets
mdat                          ← samples for THIS fragment
moof + mdat                   ← next fragment
...
```

Used by HLS / DASH / CMAF for live streaming.

**HEIF/HEIC** = ISOBMFF carrying still images encoded with HEVC. The
`pict` track / `meta` box / `iref` references make it different from
a video MP4 but the box machinery is the same.

### Matroska / WebM (EBML)

EBML = Extensible Binary Meta Language. Variable-length encoded
elements:

```text
<element ID (var-int)> <length (var-int)> <payload>
```

Hierarchy of a typical .mkv:

```text
EBML                        ← header
Segment                     ← container of everything
  Info                        ← timecode scale, duration
  Tracks
    TrackEntry                  ← per-track info
      TrackNumber / TrackType / CodecID / CodecPrivate
  Cluster                     ← samples grouped by time
    Timecode                    ← cluster start time
    SimpleBlock                 ← one sample (codec data + relative timestamp)
  Cluster ...
  Cues                        ← seek index
    CuePoint → CueTime + CueTrackPositions
```

WebM is Matroska with restrictions (only VP8/9 + Opus/Vorbis).

### MPEG-TS (Transport Stream)

The broadcast / live streaming workhorse. Stream of fixed-size 188-
byte packets:

```text
Sync byte 0x47 | flags + PID | adaptation field | payload
   1 byte         3 bytes        variable          rest
```

PID (13-bit) identifies the stream. Special PIDs:

- 0x0000: **PAT** (Program Association Table) — lists program → PMT PID
  mapping.
- PMT PID (announced by PAT): **PMT** (Program Map Table) — lists
  this program's streams (video PID, audio PID, codecs).
- Stream PIDs: carry **PES (Packetized Elementary Stream)** payload
  with the actual codec data.

**PCR (Program Clock Reference)** is carried in the adaptation field
periodically, providing the master clock for jitter buffer
synchronization.

Decoder workflow: pick up PAT → find PMT PID → parse PMT → identify
video / audio PIDs → assemble PES payloads → feed to codec decoders.

Used for: ATSC, DVB, ARIB broadcast; HLS (HLS segments are typically
MPEG-TS); IPTV; SRT.

### MXF — broadcast container

SMPTE ST 377. KLV (Key-Length-Value) encoding throughout. Structure:

```text
File:
  Header Partition          ← contains preface, descriptors, mostly metadata
  Body Partition(s)         ← actual essence (video/audio data)
  Footer Partition          ← index tables, RIP (Random Index Pack)
```

Operational Patterns determine layout:

- **OP1a**: single file with all essence interleaved (most common for
  broadcast).
- **OP1b**: single file with separate-essence sections.
- **OPAtom**: one essence type per file (used by Avid Media Composer).

KLV: each block has a 16-byte UL (Universal Label) key, then BER-
encoded length, then payload. Self-describing.

Metadata-rich: track timing, source-clip references, descriptive
metadata, encryption, edit decisions can all be embedded.

### AVI (RIFF)

Legacy. Microsoft's late-90s container, still surfaces in old
archives. RIFF (Resource Interchange File Format) hierarchy:

```text
RIFF 'AVI '
  LIST 'hdrl'                ← header list
    'avih' main header
    LIST 'strl' (per-stream)
      'strh' stream header
      'strf' stream format
  LIST 'movi'                ← interleaved sample data
    '00dc' / '01wb' chunks    ← one sample each
  'idx1'                     ← simple sample index
```

24-bit timebase, no robust fragmentation, no PTS/DTS separation.
Carries any codec but legacy of MPEG-4 ASP, DivX, Cinepak.

### FLV — Flash Video

→ [`crates/oximedia-container/src/demux/flv/mod.rs`](../crates/oximedia-container/src/demux/flv/mod.rs)
and the FLV PR description.

### OGG

Sequence of pages (4096-byte chunks). Each page belongs to one
**logical bitstream** (identified by serial number). Inside pages:
packets, which become codec frames after reassembly.

**Granule positions** carry timing per page, codec-defined
interpretation. Vorbis = sample count; Theora = (frameno, keyframe-offset);
Opus = sample count at 48 kHz.

Native to Xiph codecs: Vorbis, Theora, FLAC, Opus, Speex.

---

## Part 16 — Streaming protocols

→ Existing depth: [`wire_formats.md`](wire_formats.md) §RTP, §SDP,
§TCP-interleaved, §HTTP Digest.

### HLS — HTTP Live Streaming

Apple's adaptive streaming protocol. Two files:

- **Master playlist** (.m3u8): lists available variants (bandwidth,
  resolution, codec).
- **Variant playlist** (.m3u8): lists segments for one variant.

Segments are typically **fMP4** or **MPEG-TS**, 2–6 seconds each.
Player fetches segments sequentially over HTTP, switching variants
based on measured bandwidth.

**Low-Latency HLS (LL-HLS)**: sub-segment "parts" with HTTP/2
server push or chunked transfer. Latency reduced from 30s to ~3s.

### DASH — Dynamic Adaptive Streaming over HTTP

ISO/IEC 23009-1. Vendor-neutral counterpart to HLS. **MPD (Media
Presentation Description)** is an XML manifest:

```xml
<MPD>
  <Period>
    <AdaptationSet>           ← e.g. "video"
      <Representation>          ← one bitrate variant
        <SegmentTemplate ... />
      </Representation>
    </AdaptationSet>
    <AdaptationSet>           ← e.g. "audio"
      ...
    </AdaptationSet>
  </Period>
</MPD>
```

Segments fetched the same way as HLS. Profiles:

- **OnDemand**: single-file representations + index segment.
- **Live**: time-based segment naming, MPD updated periodically.
- **Low-Latency** (LL-DASH): CMAF + HTTP chunked transfer.

CMAF unifies the segment formats across HLS and DASH (same fMP4
segments served to both).

### RTMP — Real-Time Messaging Protocol

Adobe's TCP-based live ingest protocol. Still dominant for live
streaming despite age (origin: Flash Player).

Handshake:

```text
client → C0 (1 byte: version)
client → C1 (1536 bytes: random)
server → S0 (1 byte)
server → S1 (1536 bytes)
client → C2 (1536 bytes: echo of S1)
server → S2 (1536 bytes: echo of C1)
```

After handshake, **chunk stream**. Variable-size chunks (default 128
bytes), each tagged with a chunk stream ID and a message type
(audio, video, command, etc.).

**AMF (Action Message Format)** carries command messages
(`connect`, `publish`, `play`). AMF0 / AMF3 serialize JavaScript-like
objects.

**Enhanced RTMP (E-RTMP)**: 2024 extension adding HEVC, AV1, VP9 via
FourCC-based codec IDs in the video header.

### WebRTC — interactive low latency

The actual cleanly-designed real-time stack:

- **ICE (Interactive Connectivity Establishment)**: find a connectable
  IP/port via STUN (echo server) and TURN (relay server).
- **DTLS-SRTP**: key exchange + RTP encryption.
- **SDP (Session Description Protocol)**: offer/answer for
  capability negotiation.
- **RTP**: media transport.
- **RTCP**: feedback (loss reports, jitter, NACK requests).
- **Bandwidth estimation**: built-in (TWCC, GCC).

**WHIP** (WebRTC-HTTP Ingest Protocol) and **WHEP** (WebRTC-HTTP
Egress Protocol): HTTP-based handshake to set up a WebRTC session.

### SRT — Secure Reliable Transport

UDP-based. Designed for unmanaged-network video contribution. Features:

- Sub-second latency over public internet.
- ARQ (Automatic Repeat-reQuest) for packet loss.
- AES-128/256 encryption.
- Bidirectional (caller / listener / rendezvous modes).

Spec'd by Haivision; reference implementation open-source.

### RIST — Reliable Internet Stream Transport

VSF (Video Services Forum) standard. Similar to SRT, broadcast-focused.
ARQ + FEC. Profiles: Simple, Main, Advanced.

### ST 2110 — uncompressed broadcast over IP

SMPTE standards (2110-10/-20/-30/-40):

- ST 2110-10: PTP synchronization.
- ST 2110-20: uncompressed video (raw pixels, no codec).
- ST 2110-30: uncompressed PCM audio.
- ST 2110-40: ancillary data (timecode, captions).

Replaces SDI (serial digital interface) in modern broadcast facilities.
Bandwidth: 1080p60 4:2:2 10-bit = 3 Gbit/s. Network switches must be
10 GbE / 25 GbE / 100 GbE with PTP support.

---

## Part 17 — Hardware acceleration

→ Existing depth: [`wire_formats.md`](wire_formats.md) on
VideoToolbox FFI conventions.

The decode/encode pipeline is one of the most computationally
expensive things modern devices do. Every CPU/GPU vendor ships
dedicated silicon ("fixed-function blocks") for H.264, HEVC, AV1
decode (and increasingly encode).

### The dispatch problem

You can't just "ship a hardware decoder" — there are 5+ different APIs
to talk to the hardware, and which one is available depends on the
platform:

| API | Platform | Vendor |
|---|---|---|
| VideoToolbox | macOS / iOS | Apple |
| NVENC / NVDEC | Linux / Windows | NVIDIA |
| QSV / oneVPL | Linux / Windows | Intel |
| VAAPI | Linux | open standard (Intel, AMD, NVIDIA all implement) |
| AMF | Linux / Windows | AMD |
| Vulkan Video | Linux / Windows | Khronos cross-vendor |
| MediaCodec | Android | OS framework |
| MediaFoundation / DXVA | Windows | OS framework |
| WebCodecs | Browsers | W3C |

A real production pipeline probes each at startup and picks the best
available. **OxiMedia's `-sys` crates** (see [`ffmpeg_parity.md`](ffmpeg_parity.md))
ship FFI bindings to all of these.

### Performance budget

A 1080p60 H.264 decode is ~150M block-ops/sec — within reach of a
single optimized SIMD core. 4K HDR HEVC decode is ~4× that; usually
benefits from hardware. 8K AV1 is ~16× and *requires* hardware on
non-data-center hardware.

Encode is typically 5–20× more expensive than decode.

### SIMD acceleration

For software decode/encode, SIMD is the biggest single optimization.
The hot loops are:

- **IDCT**: 8×8 / 16×16 / 32×32 matrix-multiply with constants.
- **Motion compensation**: 6-tap / 8-tap horizontal/vertical filters.
- **SAO/CDEF**: per-pixel offset/filter with branchless predicates.
- **CABAC**: notoriously *bad* for SIMD (sequential bit-dependent
  arithmetic). AV1's range coder is better but still sequential.

Architectures:

- **NEON** (ARM AArch64): 128-bit vectors.
- **SVE / SVE2** (ARM newer): scalable vectors.
- **AVX2** (x86-64): 256-bit vectors.
- **AVX-512** (x86-64): 512-bit vectors (limited deployment).

→ [`simd_dispatch.md`](simd_dispatch.md) for the workspace's SIMD
strategy.

### Vulkan Video — the cross-vendor future

`VK_KHR_video_decode_h264` / `VK_KHR_video_decode_h265` /
`VK_KHR_video_decode_av1` and their encode counterparts let a single
API drive H.264/HEVC/AV1 hardware blocks across NVIDIA / AMD / Intel
GPUs.

Trade-off: more verbose than vendor SDKs; no platform monoculture
penalties.

---

## Part 18 — Bit-exact conformance

Every standardized video codec specifies its decoder behavior **bit-
exactly**. Two correct decoders given the same bitstream must produce
**identical** pixel outputs — to the bit. Why?

1. **Encoder/decoder loop closure**. The encoder runs a copy of the
   decoder internally to know what the receiver will see. Drift between
   encoder's "predicted reference" and decoder's actual reference
   causes cascading error.
2. **Reference picture matching across implementations**. A
   conferencing system might have an Apple decoder on one side and a
   Linux libde265 on the other. Bit-exactness ensures pixel parity.
3. **Test infrastructure**. Codec conformance suites compare your
   decoder's output to a reference; "close enough" isn't acceptable.

### What enables it

- **Integer arithmetic** everywhere (no floating point).
- Specific rounding modes ("round to nearest, ties to even", "round
  half up", per stage).
- Bit-accurate tables (cosine constants, intra prediction filters,
  context init tables) shipped in the standard.

### Conformance test suites

Every codec has one:

- **H.264**: JVT-Allegro test vectors, ITU-T conformance suite.
- **HEVC**: ITU-T conformance suite, JCT-VC test vectors.
- **AV1**: AOM conformance suite (free, on GitHub).

A passing decoder produces bit-identical YUV output to the reference
for every test vector. Test counts:

- H.264: ~250 test vectors.
- HEVC: ~700.
- AV1: ~600.

### "Reference decoder"

ITU-T (for H.26x) and AOM (for AV1) publish **reference decoders** —
slow, readable, prioritize correctness over performance. These are
the authoritative implementations against which conformance is judged.

For H.264: ITU-T's JM (Joint Model). For HEVC: HM. For AV1:
`libaom`'s decoder + AOM ConformanceTest tool.

---

## Part 19 — Quality metrics

Codecs aren't optimized for low bitrate per se; they're optimized for
**quality at low bitrate**. Quality must be measurable.

### PSNR — the classic

```text
MSE = (1/(W·H)) · Σ (original[i,j] − decoded[i,j])²
PSNR = 10 · log₁₀(MAX² / MSE)           dB

For 8-bit: MAX = 255, so PSNR = 10·log₁₀(255²/MSE) = 20·log₁₀(255/√MSE)
For 10-bit: MAX = 1023
```

Higher is better. Typical values:

- 30 dB: visibly compressed but acceptable.
- 35 dB: hard to distinguish from original on consumer displays.
- 40 dB: visually identical.
- 50 dB: numerically near-lossless.

Problem: PSNR is poorly correlated with perceived quality.

### SSIM — Structural Similarity

Compares structural content:

```text
SSIM(x, y) = (2·μ_x·μ_y + C1) · (2·σ_xy + C2)
             ─────────────────────────────────
             (μ_x² + μ_y² + C1) · (σ_x² + σ_y² + C2)

μ = local mean, σ² = local variance, σ_xy = local covariance,
C1, C2 small constants.
```

Computed over local 8×8 windows, then averaged. Range [-1, 1], higher
is better.

Better correlated with perception than PSNR, but still imperfect.

### MS-SSIM

Multi-scale SSIM. Compute SSIM at multiple resolutions and combine.
Common in published research.

### VMAF — Video Multi-Method Assessment Fusion

Netflix's open-source metric. Computes multiple features
(motion-compensated DLM, VIF at multiple scales, temporal information)
and fuses them via a learned model.

Outputs a quality score 0–100. **VMAF-100 ≈ visually transparent
quality**, **VMAF-93 ≈ visually transparent for most viewers**, **VMAF
< 70 ≈ obviously degraded**. Best objective metric available today.

### Subjective testing

The gold standard. ITU-R BT.500 / BT.2100 procedures:

- **MOS (Mean Opinion Score)**: panel rates 1–5.
- **ACR (Absolute Category Rating)**: 1–5 scale.
- **DSCQS (Double Stimulus Continuous Quality Scale)**: side-by-side
  rating.

Used to validate objective metrics. Slow and expensive (~$1k/hour for
proper sessions with calibrated displays and trained viewers).

---

## Part 20 — Performance engineering

### Where the time goes

A typical software decoder spends time roughly:

| Stage | % of decode time |
|---|---|
| CABAC / entropy decode | 20–40% |
| Motion compensation (sub-pel filters) | 20–35% |
| Inverse transform (IDCT) | 10–20% |
| Loop filter (deblock + SAO + CDEF) | 15–25% |
| Memory transfers / copy | 5–15% |
| Bookkeeping (DPB, slice header, etc.) | 5–10% |

**Optimization order:** profile, then attack the biggest slice.

### Cache and memory layout

- **Plane stride alignment**: stride aligned to 64 bytes (cache line)
  → no split loads. SIMD-aligned (16 / 32) → faster `vld`/`vst`.
- **Reference frame organization**: store frames in NV12 (planar Y +
  interleaved UV) or YUV420p as the decoder's working format.
  Conversion costs add up.
- **Tile / slice locality**: process tiles to completion before moving
  on — better cache reuse than interleaving.

### Bit-reader optimization

The CABAC / range coder hot loop processes 1 bit at a time. Optimization:

- **Multi-symbol decode**: AV1 range coder decodes up to 16 symbols
  in parallel via lookup.
- **Renormalization batching**: defer the renorm shift until multiple
  decisions have been made; amortize the bit-extract cost.
- **Branch prediction hints**: MPS is observed 70–95% of the time.
  Mark `likely`/`unlikely` accordingly.

### SIMD patterns

For 8×8 IDCT (16-bit intermediate):

```text
NEON:
  Load 8 rows × 8 shorts each = 8 × 128-bit vectors
  Butterfly transform via vqdmulh, vqrshl
  Transpose 8×8 via vzip / vuzp / vtrn sequences
  Repeat for column pass

AVX2:
  16 shorts per 256-bit register → 2 rows per register
  Use vpmaddwd, vpsrad
  Transpose via vpunpcklwd / vpunpckhwd
```

Modern codec implementations spend 60–80% of their performance budget
in hand-tuned SIMD kernels. dav1d (the AV1 decoder) is notable for
exceptionally well-optimized SIMD across NEON, SSE2, SSSE3, AVX2.

### Error resilience and concealment

Decoders facing corrupt streams have two options:

1. **Abort** — return an error, stop. Fine for stored files.
2. **Conceal** — substitute plausible content (copy from neighbour /
   previous frame). Required for live broadcast where dropping a
   frame is worse than minor visual artifacts.

Concealment strategies:

- **Spatial**: interpolate the lost block from decoded neighbours.
- **Temporal**: copy the corresponding block from the previous frame
  (with zero MV or estimated MV).
- **Macroblock-mode-based**: use the most-probable mode for missing
  block headers and decode whatever residual data is available.

Resync points (slice / tile boundaries) limit concealment scope and
prevent error from propagating to the entire frame.

---

## Part 21 — Workflow and production realities

Codec engineers work in a larger ecosystem. Knowing the workflow makes
your decisions about codec features land correctly.

### NLE workflow (Non-Linear Editor)

Editor (Premiere, Resolve, FCP, Avid) ingests source media, presents
a timeline, and exports rendered media. For each cut, the editor
either:

- Plays original media unchanged (fast, but display GPU has to decode
  on-the-fly).
- Generates **proxies**: low-resolution intermediates for smooth
  scrubbing (ProRes Proxy / DNxHR LB / H.264).
- Renders effects to **intermediate masters**: ProRes 422 / DNxHR /
  Cineform, for further work.

Editors hate H.264 / HEVC as working format because every seek
requires decoding back to the previous keyframe (sometimes seconds
away). Intra-only codecs (ProRes, DNxHR) let you seek per-frame.

### Color grading workflow

Color grades go through:

1. **Conform**: load editor's EDL, link to camera-raw / log-format
   source media.
2. **Working space**: convert to ACEScct / Log-encoded RGB for grading.
3. **Grade**: artist manipulates curves, primaries, mattes.
4. **Master deliverables**: render to Rec.709 SDR / BT.2100 HDR /
   DCI-P3 / etc.

Codecs at this stage: intra-only 4:4:4 (DNxHR 444, ProRes 4444) or
uncompressed (DPX, OpenEXR sequences). **Never** distribution-codec
formats — re-quantizing colors twice destroys grade fidelity.

### Mastering vs delivery

**Master** = the highest-quality version, produced once, stored
forever. Bit depth 10/12, chroma 4:2:2 or 4:4:4, intra-only or
lightly-compressed. Format: ProRes HQ, DNxHR HQ/HQX, JPEG2000 MXF.

**Delivery** = encoded for distribution. Multiple variants per master:
4K HDR HEVC, 1080p H.264, ABR ladder for streaming. Highly compressed,
inter-coded, 4:2:0 8-bit. Format: H.264/AAC in fMP4, HEVC/EAC-3 in
fMP4 for HDR, AV1 increasingly.

Never re-encode delivery to delivery — quality only goes down. Always
re-encode from the master.

### Broadcast vs streaming vs cinema vs gaming

| Domain | Codec | Container | Latency | Quality target |
|---|---|---|---|---|
| OTA broadcast (US ATSC 1.0) | MPEG-2 | TS | ~5 s | Mass-market |
| OTA broadcast (US ATSC 3.0) | HEVC | ROUTE / DASH | ~5 s | UHD+HDR |
| OTA broadcast (EU DVB-T2) | H.264 / HEVC | TS | ~3 s | HD/UHD |
| Streaming VOD (Netflix, YouTube) | H.264, HEVC, AV1 | fMP4 / CMAF | ~30 s | Per-title optimization |
| Live streaming | H.264 (legacy) / HEVC / AV1 | HLS / DASH / RTMP | ~3–15 s | Stable bitrate |
| Conferencing | VP8 / VP9 / H.264 / AV1 | RTP | <100 ms | Real-time |
| Cloud gaming | H.264 / HEVC / AV1 | RTP / custom | <30 ms | Game-readable |
| Digital cinema | JPEG2000 | MXF | n/a | Reference quality |
| Game capture | H.264 / NVENC (live) | mkv / mp4 | Real-time | Low CPU cost |

### Quality control checks

A serious post pipeline runs automated QC:

- **Loudness**: BS.1770 / EBU R128 / ATSC A/85 measurement.
- **Levels**: peak / RMS / true-peak monitoring.
- **Sync**: A/V sync drift over the timeline.
- **Pixel errors**: frozen frames, dropped frames, scene-cut artifacts.
- **Closed captions**: present and synchronized.
- **HDR metadata**: MaxCLL/MaxFALL within range, Dolby Vision RPU
  validates.

→ The workspace's [`crates/oximedia-qc`](../crates/oximedia-qc/) and
[`crates/oximedia-metering`](../crates/oximedia-metering/) automate
these checks.

---

## Part 22 — Reading list and resources

### Books

- **Iain Richardson — "The H.264 Advanced Video Compression Standard"**
  — the clearest standalone H.264 reference.
- **Sze / Budagavi / Sullivan eds. — "High Efficiency Video Coding (HEVC):
  Algorithms and Architectures"** — same depth for HEVC.
- **Khalid Sayood — "Introduction to Data Compression"** — the DCT /
  entropy coding / quantization theory in isolation.
- **Marina Bosi & Richard Goldberg — "Introduction to Digital Audio
  Coding and Standards"** — the AAC bible.
- **Charles Poynton — "Digital Video and HD"** — color science +
  video signal theory from first principles. Essential.
- **Pascal Hartmann — "MXF: Media Exchange Format"** — broadcast
  container.

### Standards (read the actual spec)

- **ITU-T H.264** (free PDF) — the H.264 spec. ~800 pages, but
  legible.
- **ITU-T H.265** (free PDF) — HEVC.
- **ITU-T H.266** (free PDF) — VVC.
- **AV1 bitstream and decoding process** (AOM, free) — the AV1 spec.
- **ISO/IEC 14496-12** — ISOBMFF.
- **ISO/IEC 14496-3** — AAC.
- **RFC 6184** — H.264 RTP payload.
- **RFC 7798** — HEVC RTP payload.
- **RFC 6716** — Opus.
- **SMPTE ST 377** — MXF base.
- **SMPTE ST 2067** — IMF.
- **SMPTE RDD 36** — ProRes.

### Open-source codecs worth reading

- **FFmpeg / libavcodec** — the encyclopedic reference. Every codec.
- **x264** (H.264 encoder) — exceptionally clean, well-commented.
- **x265** (HEVC encoder).
- **libvpx** (VP8/9).
- **libaom** (AV1 reference encoder + decoder).
- **dav1d** (AV1 decoder) — best-in-class SIMD optimization.
- **SVT-AV1** (AV1 encoder by Intel/Netflix) — designed for parallelism.
- **rav1e** (AV1 encoder in Rust) — modern rewrites, readable.

### Tooling

- **FFmpeg / ffprobe** — diagnose any media file. Learn the CLI.
- **MediaInfo** — file inspection with a friendly UI.
- **Bitrate viewer / Codec Hawk** — visualize per-frame size.
- **VMAF** — Netflix's quality measurement tool.
- **dvtool / x265-info / av1-info** — bitstream introspection.
- **MP4Box** (GPAC) — ISOBMFF surgery.
- **MKVToolNix** — Matroska editing.

### Communities

- **Doom9 forum** — codec internals, hobbyist-pro overlap.
- **Hydrogen Audio** — psychoacoustic-models corner.
- **AOM and MPEG/JVET reflectors** — actual standards work.

---

## Reading order recommendation

If you started reading this document cold and want a practical path
to "serious codec engineer":

1. **Today**: Read this curriculum (Parts 1–4 and 14–18 for shape).
2. **Week 1**: Deep-read [`codec_internals.md`](codec_internals.md)
   and Part 6, 9, 11, 13 of this doc.
3. **Week 2**: Implement a toy decoder. Easiest target: PCM/WAV
   container + lossless audio. Then graduate to FFV1 video.
4. **Week 3**: Read x264's source. Focus on `encoder/me.c` (motion
   estimation), `common/dct.c` (transforms), `encoder/cabac.c`
   (entropy).
5. **Week 4**: Implement a fresh decoder against a real spec. Recommend
   AV1's CDEF or AV1's range coder as a focused unit. Or contribute to
   the parser layer of an existing decoder in this workspace.
6. **Month 2**: Read Iain Richardson cover-to-cover. Skim Sze/Budagavi.
7. **Month 3**: Implement a complete decoder for an intra-only codec —
   ProRes, DNxHR, or HuffYUV. The workspace's
   [`prores`](../crates/oximedia-codec/src/prores/) module is a worked
   example you can extend.
8. **Month 6**: Build (or read carefully) a full inter-coded decoder —
   probably for HEVC because the spec is approachable.
9. **Year 1**: Pick a real engineering problem in a real workspace —
   SIMD optimization, conformance testing, a new bitstream feature in
   an existing codec — and contribute it.

The shortest path from "reads about codecs" to "is a codec engineer"
is **shipping a working decoder**. There's no shortcut.
