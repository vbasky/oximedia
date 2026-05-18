# Chapter 24 — Why the Pipeline Stages Are in This Order

> **Engineering takeaway:** The pipeline goes predict → transform →
> quantize → entropy code, in exactly that order. This isn't
> arbitrary. Each stage transforms the data into a form that's better
> suited for the *next* stage to do its work. Skip a stage and the
> rest doesn't work as well. Swap two stages and the math falls apart.
> This chapter explains why each stage is where it is — not by quoting
> theorems, but by walking through what would go wrong if you tried it
> differently.

In Chapter 23 you saw why each stage exists: each one approaches
Shannon's bound by exploiting a specific kind of redundancy. But why
in this particular order? Why couldn't you, say, entropy-code raw
pixels and then transform the result?

The answer is that the stages compose. Each one is designed to take
the output of the previous stage as its input. Mess with the order
and you're handing the next stage data it's not prepared for.

This chapter walks through each stage and asks: "What would happen if
you moved it?" By the end you'll see why this specific four-stage
ordering is essentially forced.

## 24.1 Why prediction goes first

The first thing the codec does — before any transform, any
quantization, any entropy coding — is *predict*. Why?

Because **prediction reduces what you have to encode**. After
prediction, you're not encoding pixel values; you're encoding the
*errors* in your prediction. Those errors are statistically smaller
and more concentrated near zero than the original pixels. Everything
downstream — transform, quantization, entropy coding — works better
on smaller, more concentrated values than on big, spread-out pixel
values.

### What if you didn't predict first?

Imagine a codec that just transforms and entropy-codes raw pixels,
with no prediction. What would happen?

Raw natural image pixels look like a distribution centered around
middle gray, spread over [0, 255]. The 2D DCT of a typical 8×8 patch
of raw pixels has *most of its energy in the DC coefficient* (block
average brightness) and significant energy spread across the AC
coefficients.

Now do the same on the *residual* after intra prediction. The DC
coefficient is small (because the prediction got close to the actual
average). The AC coefficients are *much* smaller and *much* more
clustered near zero (because the prediction got the local structure
roughly right).

After quantization, the no-prediction version has 30–40 non-zero
coefficients per block. The with-prediction version has 4–8. The
entropy coder spends 4–10× more bits on the no-prediction version.

That's a direct compression cost. **Prediction is the single biggest
lever in the entire pipeline**, and it has to come first because
everything else benefits from working on residuals.

### What if you predicted *after* the transform?

Could you transform raw pixels first, then "predict" the transform
coefficients from neighbours' transform coefficients?

You'd think this could work, but it doesn't well. The neighbours'
transform coefficients aren't a great predictor of the current
block's coefficients, because the DCT of a block depends sensitively
on the block's exact contents — small changes in the spatial domain
become medium-sized changes in the frequency domain. The prediction
would be weak.

Some codecs (notably JPEG 2000) try variants of this and end up with
worse compression than the predict-first design. It's been tried; it
doesn't win.

So: **prediction first, in the pixel domain, where neighbours are
strongly correlated**.

## 24.2 Why the transform comes after prediction

After prediction, you have *residuals* — a small block of mostly-near-
zero values with occasional larger values where the prediction missed.

Why transform these instead of just quantizing them directly?

### Spatial vs frequency representation

The residual block, in the spatial domain, has a particular property:
the non-zero values are *clustered*. A region where the prediction
missed has a cluster of non-zero residuals; smooth regions have flat
zero residuals.

This clustering is *not* the same thing as "sparse." Sparse means
"few non-zero values"; clustered means "the non-zero values are next
to each other." A spatial residual block has both, but the spatial
layout matters less for compression than the *frequency content*.

If you transform the residual to frequency:

- **Smooth residuals** concentrate energy in low frequencies (top-
  left of the coefficient block).
- **Sharp residuals** spread energy across frequencies but with
  strong directionality.
- **Noise-like residuals** spread uniformly.

The key benefit: **after transform, the energy is concentrated in a
few coefficients** (typically the top-left corner — DC and low-
frequency AC). The other 50+ coefficients are tiny or zero.

Quantization will throw away all the tiny coefficients. After
quantization, you have a *sparse* coefficient block — mostly zeros,
a few non-zeros, and they're predictably in the top-left.

The transform converted clustering in space into sparsity in
frequency. That sparsity is what the entropy coder can exploit.

### What if you quantized spatial pixels directly?

Could you quantize the residuals directly, without transform?

Yes, and some codecs (transform-skip mode in HEVC/AV1) do exactly
this for certain blocks where it helps. But for most natural content,
it's worse. Why?

Spatial residuals are *correlated* with their neighbours. If
residual[i][j] is 5, residual[i+1][j+1] is probably also small. After
quantization, the spatial residual values are still correlated —
which means the entropy coder can't fully exploit their distribution.

After transform, the coefficients are *decorrelated* — the value of
one coefficient doesn't tell you much about the value of another. The
entropy coder can model each coefficient's distribution independently
and get closer to entropy.

So: **transform decorrelates and concentrates**. Quantization is more
effective on transformed coefficients than on raw spatial residuals.

## 24.3 Why quantization comes after transform

Now you have transform coefficients. Why quantize them rather than,
say, quantize first then transform?

Two reasons.

### Reason 1: perceptual weighting

The transform separates information by frequency. Human vision is
*much* more sensitive to errors in low frequencies than in high
frequencies. A small error in the DC coefficient changes block
average brightness — instantly visible. A small error in a high-
frequency coefficient changes fine texture — almost invisible.

Quantization in the transform domain lets you exploit this directly.
You apply *different* quantization steps to different frequencies — a
matrix of step sizes that's small in low frequencies (preserve
precision) and large in high frequencies (throw away precision).

This is the quantization matrix (Chapter 9). It only makes sense in
the transform domain. In the spatial domain there's no concept of
"low frequency vs high frequency" for a single pixel.

### Reason 2: the math is reversible

Quantizing transform coefficients (dividing by step, rounding) is a
simple integer operation. The corresponding dequantization
(multiplying by step) is also simple.

If you tried to quantize then transform, the quantization step would
introduce noise into the spatial residual, the transform would
re-distribute that noise across frequencies, and the decoder would
have to undo the transform first, then dequantize, then add to
prediction. The error analysis gets messy. The transform/quantize
ordering aligns with how the encoder and decoder naturally compute.

### What if you quantized at multiple stages?

Some codecs (JPEG XS, intermediate formats) do quantization at
multiple stages, with different precisions. This adds complexity
without much benefit; it doesn't compose well with the rate-distortion
optimization framework. The mainline codecs settled on a single
quantization step.

So: **quantize once, in the transform domain, with frequency-
weighted step sizes**.

## 24.4 Why entropy coding comes last

After quantization you have a sparse integer sequence — most values
zero, occasional non-zeros, distributions heavily skewed toward small
magnitudes. Now you need to convert this sequence into bits.

Why is entropy coding last? Because every prior stage has been
designed to *produce* the kind of distribution that entropy coding
loves.

- Prediction → sparse, concentrated values.
- Transform → energy in a few coefficients.
- Quantization → most coefficients exactly zero, surviving ones small.

The entropy coder takes that highly structured distribution and
*reaches the Shannon bound* — it uses the minimum bits per symbol
that the source distribution allows.

If you put entropy coding earlier — say, entropy-code the raw pixels
— you'd be entropy-coding a high-entropy distribution. You'd use lots
of bits. The other stages would never get to do their work.

If you skipped entropy coding entirely — just dump the quantized
coefficients as fixed-width integers — you'd waste 30–50% of the
bits, because the actual entropy of the coefficient sequence is much
lower than its fixed-width representation.

So: **entropy coding last, because it's the only stage that exploits
the actual probability distribution of what's left**.

## 24.5 The order matters: an "if reversed" thought experiment

Let's flip the pipeline backwards: entropy decode → dequantize → IDCT
→ add prediction. Why is this the decoder's order? Because each step
*undoes* what the encoder did, in reverse order of when it happened.

The encoder did:
```
predict → subtract prediction (residual) → transform → quantize → entropy encode
```

The decoder does the inverse, in reverse:
```
entropy decode → dequantize → inverse transform → add prediction
```

If you tried to dequantize before entropy-decoding, you'd be operating
on a bitstream that hasn't been unpacked into integers yet. Doesn't
work. If you tried to inverse-transform before dequantizing, you'd
inverse-transform quantized integers, which would produce wrong
spatial residuals.

The decoder order is dictated by the inverse-of-encoder constraint.
The encoder order is dictated by the "each stage prepares for the
next" principle of §§24.1–24.4.

## 24.6 The loop filter — the only post-pipeline addition

The deblocking filter (and SAO, CDEF, LR — Chapter 11) doesn't fit
neatly into the predict-transform-quantize-entropy framing. Where
does it belong?

It's a *post-pipeline* operation: applied *after* reconstruction (per
block, per slice, per frame), and *before* the frame becomes a
reference.

The motivation: the per-block quantization in stage 6 creates
discontinuities at block boundaries. Adjacent blocks were quantized
independently, so their boundary pixels don't always line up. The
result is visible "blockiness" — small steps at block boundaries.

The loop filter smooths these. It's *not* part of the basic rate-
distortion machinery; it's a fix for an artifact of block-based
coding. Without it, you'd see ugly blocks at low bitrates. With it,
the artifact gets smeared into something less objectionable, AND
subsequent frames using this one as reference see a cleaner reference
(which helps inter prediction).

That's why it's "in-loop": the filter is applied before storing in
the DPB, so future frames see filtered references. Pure post-
processing (filtering only at display time) would lose this benefit.

## 24.7 Why this whole framework is "block-based"

A subtler question: why does every modern codec divide images into
*blocks* at all? Why not, say, transform the entire image at once, or
use a continuous adaptive partition?

Block-based has won for practical reasons:

1. **Local adaptation**: different parts of the image have different
   characteristics (smooth sky, busy foliage, sharp text). Blocks let
   you pick different modes per region.
2. **Spatial locality**: nearby pixels are correlated, but pixels far
   apart aren't. Predicting from neighbours requires local context;
   blocks define what "local" means.
3. **Implementation tractability**: fixed-size operations are
   hardware-friendly. SIMD instructions process N pixels at a time.
   Caches line up with block sizes.
4. **Random access**: you can decode any block once you have its
   prerequisites. The whole frame isn't a single monolithic
   computation.

Alternatives have been tried. **JPEG 2000** uses wavelet transforms
on the whole image at once, with no blocks. It gets slightly better
compression on some content but is much harder to implement
efficiently. **DCT-only codecs** with no prediction (the early
JPEG model) have block boundaries but no inter-block prediction —
they compress worse.

The block-based hybrid pipeline (with the additions modern codecs
have made) is, empirically, the sweet spot.

## 24.8 What this means for the codec engineer

Once you internalize the pipeline rationale:

1. **You can read any new codec spec quickly.** Whether it's H.266,
   AV2, or something new in 2030, it'll have predict-transform-quantize-
   entropy in that order. You'll only have to learn the codec's
   *specific choices* at each stage.

2. **You can debug bugs faster.** Knowing what each stage *should*
   produce helps you trace where things went wrong. "The output is
   correct after prediction but wrong after transform" tells you
   exactly where to look.

3. **You can have informed opinions about new codecs.** When someone
   announces "AV2 is 50% better than AV1," you can ask the right
   questions: "What's their R(D) curve? What new tools at which
   stage? What's the decoder cost?"

4. **You understand the limits.** Shannon's bound is a hard ceiling.
   Compression gains slow as we approach it. The next codec
   generation will probably gain 20%, not 50%, and at higher decode
   cost. Plan accordingly.

## 24.9 The future of the pipeline

What might change in future codecs?

**More aggressive prediction**. Neural-network-based prediction (like
VVC's MIP mode) shows promise. Adaptive prediction that learns from
local statistics could close the prediction-quality gap.

**Learned transforms**. Could a neural-network-trained transform
outperform the DCT for natural images? Maybe — though the gain might
not justify the cost (slower decode, harder to standardize).

**End-to-end learned codecs**. Some research codecs use neural
networks for *the entire pipeline*, trained end-to-end on
reconstruction quality. Early results are promising for very low
bitrates; the standard "neural codec" of 2030 might not have the
predict-transform-quantize-entropy structure at all.

But for now, in 2026, the standard structure dominates because it's
the best implementation of Shannon's theory we know. New codecs add
more options at each stage; they don't usually change the stages
themselves.

## 24.10 Further reading

- **[Wiegand2003]** §III — H.264 architecture as the canonical example
  of the pipeline.
- **[Sullivan2012]** §II — HEVC architectural overview.
- **[Chen2020]** §1–2 — AV1's pipeline organization.
- **[Wallace1991]** — the original JPEG paper. JPEG's predict-only-DC
  + DCT + zigzag + Huffman is the spiritual ancestor of every modern
  codec's pipeline.

## 24.11 Exercises

1. **The reverse-engineering exercise.** Pick a codec you've never
   used (maybe VP10, AV2, or some other emerging codec). Try to guess
   its pipeline structure from the codec's stated features. You should
   find: predict-transform-quantize-entropy in some form.

2. **What does prediction buy you?** Take a real video file. Encode
   it with x264 at default settings, then with x264 with `--no-pskip
   --no-mbtree --analyse none` (disables most prediction). Compare
   file sizes. The ratio gives you a sense of prediction's value.

3. **Quantize before transform?** Conceptually sketch what would
   happen if you quantized residuals in the spatial domain instead of
   transform domain. Where does the perceptual matrix go?

4. **Entropy coding's role.** If you removed the entropy coder
   entirely (output each coefficient as a fixed-width integer), how
   much bigger would your file be? Why?

5. **Block-based vs whole-image.** What's the advantage of block-
   based coding over JPEG 2000's whole-image wavelet transform?

6. **Looking ahead.** What if codecs of 2035 don't use the predict-
   transform-quantize-entropy pipeline? What would replace it?

---

Next: [Chapter 25 — Rate Control: The Math Behind QP Selection](ch25-rate-control-theory.md).
