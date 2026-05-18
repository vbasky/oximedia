# Chapter 20 — Quality Metrics

> **Engineering takeaway:** Compression is lossy. To compare encoders,
> compare codecs, or tune your pipeline, you need a way to measure
> *how lossy*. PSNR is the simple universal metric; SSIM and MS-SSIM
> add perceptual weighting; VMAF is the modern Netflix-developed
> learned metric that correlates best with human ratings. Subjective
> testing is the gold standard but expensive. Pick the right metric
> for the task, and never trust a single number — quality has multiple
> dimensions.

A decoder produces YUV pixels. An encoder threw away information to
make the bitstream small. Quality metrics let you quantify *how much*
information was lost and how visible that loss is.

This chapter covers the standard objective metrics, when to use each,
and the role of subjective testing.

## 20.1 PSNR — the workhorse

**Peak Signal-to-Noise Ratio**. For each frame:

```text
MSE = (1/N) × Σ (original[i] - decoded[i])²
PSNR = 10 × log₁₀(MAX² / MSE)
```

Where MAX is the peak pixel value (255 for 8-bit, 1023 for 10-bit).

Result: a single number in dB. Higher = closer to original.

### Typical PSNR values

- **>50 dB**: visually indistinguishable from original (near-lossless).
- **40–50 dB**: high quality, suitable for production masters.
- **35–40 dB**: streaming high tier, no visible artifacts on careful
  inspection.
- **30–35 dB**: streaming low tier, subtle artifacts visible.
- **<30 dB**: clearly degraded.

The "6 dB per bit" rule from rate-distortion theory: doubling the
bitrate roughly adds 6 dB of PSNR.

### Per-component PSNR

You can compute PSNR for each component (Y, Cb, Cr) separately:

```text
Y-PSNR = PSNR computed on luma plane only
U-PSNR, V-PSNR = same for chroma planes
PSNR_HVS = weighted combination (often 6 Y + 1 U + 1 V) / 8
```

Most codec comparisons report Y-PSNR; chroma PSNRs are reported when
chroma quality is at stake (color critical content).

### Limitations

PSNR's biggest flaw: it doesn't correlate well with perception. Two
encoded files with the same PSNR can look different — one with
visible blockiness, one with smooth blur. PSNR can't tell them apart.

This is why PSNR alone isn't enough for modern quality work.

## 20.2 SSIM and MS-SSIM

**Structural Similarity** improves on PSNR by considering luminance,
contrast, and structure in *local windows*:

```text
SSIM(x, y) = ((2μ_x μ_y + C₁)(2σ_xy + C₂)) /
             ((μ_x² + μ_y² + C₁)(σ_x² + σ_y² + C₂))
```

Where μ is local mean, σ is local std-dev, σ_xy is local covariance,
and C₁/C₂ are stabilizing constants. Average SSIM over all windows
gives the per-frame score, in [0, 1] (higher = better).

**MS-SSIM** (Multi-Scale SSIM) computes SSIM at multiple resolutions
(scales) and combines, capturing both fine and coarse structure.

### Typical values

- **MS-SSIM > 0.98**: high quality.
- **0.95–0.98**: streaming high tier.
- **0.90–0.95**: streaming low tier.
- **<0.90**: visible degradation.

MS-SSIM correlates better with perception than PSNR does. It tends to
penalize blur appropriately (PSNR doesn't), and it's still cheap to
compute.

## 20.3 VMAF — Netflix's learned metric

**Video Multi-method Assessment Fusion** is Netflix's open-source
metric, released 2016. Trained on subjective scoring data from
thousands of hours of content.

VMAF combines several "elementary features":

- **VIF** (Visual Information Fidelity) — at multiple scales.
- **DLM** (Detail Loss Metric).
- **TI** (Temporal Information) — frame-to-frame difference.

A trained ML model (originally SVR, more recently a fixed regression
formula) combines these into a single score in [0, 100], where:

- 100 = same as original.
- 80–95 = imperceptibly degraded for typical viewers.
- 60–80 = "acceptable" streaming quality.
- <60 = visibly degraded.

### Why VMAF matters

VMAF correlates with subjective scores much better than PSNR or
SSIM — typically Spearman correlation 0.9+ vs 0.6–0.8 for older
metrics.

It's now industry standard for Netflix-driven content. Other
streamers have adopted it too.

### Variants

- **VMAF Standard**: 1080p reference. Default.
- **VMAF NEG**: tuned for content with negative impressions (artifacts
  rather than smooth degradation).
- **VMAF 4K**: tuned on 4K subjective data.

The reference implementation is `vmaf` (CLI tool, also a library), with
input requirements (YUV files at matching dimensions).

## 20.4 Subjective testing

The gold standard. Real humans, real opinions.

### Standard procedures

- **DSCQS** (Double-Stimulus Continuous Quality Scale): viewer sees
  original then encoded; scores quality on a continuous scale.
- **ACR** (Absolute Category Rating): viewer sees encoded only; rates
  on a discrete scale (e.g., 1–5).
- **DCR** (Degradation Category Rating): viewer sees both; rates how
  degraded the encoded copy is.

Standardized in **ITU-R BT.500** and **BT.510**.

### When to use

- Final QA before releasing a major encoding change.
- Calibrating learned metrics (this is how VMAF was trained).
- Comparing closely-matched encoders/codecs where objective metrics
  diverge.

### Caveats

- Expensive (lots of viewers, careful viewing conditions, time).
- Variable (different viewers, different content, different days).
- Hard to scale (you can't do subjective testing on millions of files
  in CI).

## 20.5 BD-rate — the codec comparison metric

For comparing two codecs, you compute the **Bjøntegaard-delta rate**:
the average percentage bitrate change at equal quality, integrated
over a quality range.

```text
1. Encode source with each codec at 4 QPs (covering a quality range).
2. Compute quality (PSNR or VMAF) for each encode.
3. Fit a cubic through the points for each codec.
4. Compute the integral of the rate difference over the quality range.
5. Express as percentage.
```

Result: "Codec A has -30% BD-rate vs Codec B" means A uses 30% fewer
bits at the same quality.

You can compute BD-rate against any objective metric:

- **BD-PSNR**: traditional, well-established but PSNR-limited.
- **BD-SSIM** / **BD-MS-SSIM**: perceptually-weighted alternative.
- **BD-VMAF**: modern, most informative.

When papers report BD-rate without qualification, assume BD-PSNR.

## 20.6 When PSNR / SSIM / VMAF disagree

A real situation: encoder A has higher PSNR than encoder B, but lower
VMAF. Which is "better"?

Almost certainly **B**: VMAF better predicts perception. A is producing
output that's mathematically closer to the original but with less
visually pleasing characteristics — typically because A preserves
high-frequency noise that VMAF correctly identifies as not visible.

Modern encoder tuning specifically optimizes for VMAF (or AQ-modes
that VMAF approximates) over PSNR. This is intentional.

When the metrics disagree:

1. Look at the actual content — visual inspection is cheap.
2. Trust subjective scores or VMAF over PSNR.
3. Consider that the "right" answer depends on the use case (broadcast
   vs streaming vs gaming).

## 20.7 Per-shot, per-segment quality control

Modern encoding pipelines compute per-segment VMAF (or PSNR) and:

- **Encoder QC**: flag any segment with VMAF below threshold.
- **Per-shot rate control**: allocate bits based on per-shot complexity
  to maintain target VMAF.
- **Quality-based ABR**: client picks tier based on VMAF, not raw
  bitrate.

This is the cutting edge of streaming quality control. Worth knowing
exists, even if you're not building it.

## 20.8 Where this lives in the workspace

ProRes is intra-only with no aggressive quantization choices — quality
metrics aren't a major concern. For testing decoder correctness, PSNR
of decoded YUV vs. reference is sufficient (typically very high, > 50
dB, since ProRes is mostly perceptually lossless).

For inter-coded codecs (when added), quality testing pipelines would
include:

- PSNR / SSIM comparison against reference encodings.
- VMAF for end-to-end quality validation.
- Subjective testing for major codec/encoder changes.

## 20.9 Further reading

- **[Wang et al. 2004]** — original SSIM paper.
- **Netflix Tech Blog** — VMAF announcement and follow-ups. Several
  papers across 2016–2024.
- **ITU-R BT.500** — methodology of subjective assessment.
- **VQM** (Video Quality Metric) — an older perceptual metric, still
  used in some broadcast contexts.
- **NetVC IETF working group documents** — codec comparison methodology
  including which metrics to use.

For practical comparison work, the `ffmpeg-quality-metrics` Python
tool and `vmaf` CLI are the standard implementations.

## 20.10 Exercises

1. **PSNR by hand.** For an 8-bit grayscale image where the encoded
   version differs from the original by exactly 1 in every pixel,
   what's the PSNR? (MSE = 1, MAX = 255.)

2. **PSNR vs SSIM.** Two encoders A and B produce the same MSE on a
   test image, but A's errors are concentrated in a smooth area and
   B's are spread evenly. Which encoder will produce higher SSIM?
   Why?

3. **BD-rate interpretation.** A codec change shows -10% BD-PSNR but
   -25% BD-VMAF. What's happening? Is this a good change?

4. **VMAF for streaming.** Your streaming service has a quality
   ladder where each tier targets a specific VMAF. Sketch how you'd
   adjust encoder QPs to hit those targets without going over or
   under.

5. *(Reading.)* Install `vmaf` (from `vmaf-installer` or build from
   source) and run it on any pair of files (reference + encoded YUV).
   Note the output metrics.

6. **Subjective test design.** You're comparing two HEVC encoders.
   Design a subjective test using DSCQS that would give statistically
   meaningful results in < 4 hours of viewer time.

---

Next: [Chapter 21 — Performance Engineering](ch21-performance.md).
