# Chapter 23 — Why Codecs Look the Way They Do

> **Engineering takeaway:** Every modern video codec — H.264, HEVC,
> AV1, even MPEG-2 — has the same pipeline shape: predict → transform
> → quantize → entropy code. That isn't a coincidence and it isn't
> fashion. It's the only way humans have figured out to approach a
> mathematical limit that Claude Shannon proved exists in 1948. This
> chapter explains the limit, why approaching it forces the pipeline
> to look the way it does, and what the magic equation `J = D + λR`
> has to do with every QP value in every bitstream you've ever seen.
> You can ship decoders without reading this. You can't *understand*
> them without it.

You've spent the last 22 chapters watching a decoder do work. You've
seen blocks get predicted, residuals get transformed, coefficients get
quantized, and bits get entropy-coded. You've seen six different codecs
make different choices in each of those stages — but you've probably
also noticed they're all making *the same kinds of choices*.

This chapter is the answer to a question you might have started
asking around Chapter 5: **why is this the shape? Why these stages,
in this order, in every codec?**

The short answer: because Claude Shannon proved a theorem in 1948
that constrains every possible compressor, and the pipeline shape is
what falls out when you try to constructively solve the optimization
problem Shannon's theorem describes. It's not the only possible
solution — it's just the one we've found in 75 years of trying.

This chapter has math, but not much. The point is intuition. If you
make it to the end you'll see one machine being built from one idea,
which is a satisfying way to close out a book.

## 23.1 What "information" really means

Here's a programmer's way to think about Shannon's central idea.

Imagine you have two arrays of bytes, both 1 MB long. Array A is full
of cryptographically random data. Array B is the source code of a
medium-sized C program.

- Array A: cannot be compressed. Every byte carries the full 8 bits of
  information.
- Array B: can probably be compressed to ~200 KB. Most bytes are
  predictable from the surrounding bytes (curly braces follow function
  signatures, `int` precedes variable names, English-language
  comments follow word-frequency patterns).

**The size of the file isn't the same as the amount of information in
it.** Array A's 8-bit bytes really carry 8 bits each; Array B's 8-bit
bytes carry maybe 1.5 bits each on average. Compressing Array B works
because gzip finds those redundancies and removes them. Trying to
compress Array A fails because there are no redundancies to find.

Shannon gave us a number — **entropy** — that quantifies this
intuition. For a random variable `X` that takes values `x₁, x₂, ..., xₙ`
with probabilities `p(x₁), p(x₂), ..., p(xₙ)`:

```text
H(X) = − Σ p(xᵢ) · log₂ p(xᵢ)        bits/symbol
```

Don't worry if the formula doesn't speak to you. The way to read it
is: "the entropy is the *expected number of bits* each symbol carries,
given how predictable each symbol is."

For Array A (uniformly random): H = 8 bits/byte.
For Array B (predictable program text): H ≈ 1.5 bits/byte.

That's the gap compression works in.

### Two coins, one rule

The textbook example. A fair coin lands heads 50% of the time:

```text
H = − (½ · log₂ ½ + ½ · log₂ ½) = 1 bit per flip
```

Tells you that to record a fair coin flip you need 1 bit minimum. That
matches intuition: heads = 0, tails = 1, done.

Now a biased coin that lands heads 99% of the time:

```text
H = − (0.99 · log₂ 0.99 + 0.01 · log₂ 0.01) ≈ 0.08 bits per flip
```

Wait, *0.08* bits per flip? You can't have a 0.08-bit value. What
this means is: **over many flips, on average, you only need 0.08
bits per flip to record the sequence**. If you flip the biased coin
1,000 times, you can store the result in about 80 bits — typically
by run-length-encoding the long runs of heads. Each individual bit
in your output stores fractional information about each individual
flip.

This is why the entropy number can be less than 1.

### Pixels are biased coins

Now take an 8-bit grayscale pixel. The set of possible values is
{0, 1, 2, ..., 255}. If pixels were uniformly distributed, H = 8 bits.
You couldn't compress at all.

They aren't uniform. In natural images, pixel values cluster heavily.
Most pixels are mid-range; pure black and pure white are rare. The
marginal entropy of a single pixel in a typical photograph is around
7.4–7.6 bits, depending on the image.

That's a small gap from 8. Not very exciting. **The big gap is in the
*conditional* entropy** — that's the next section.

## 23.2 The trick is conditioning on neighbours

If you tell me a pixel's value with no other information, I have to
spend 7.5 bits to record it. But if you tell me a pixel's value *and*
its left neighbour, the second pixel is suddenly much more
predictable.

In natural images, adjacent pixels are strongly correlated. If the
pixel to the left is value 142, the pixel to the right is *very
likely* to be in a narrow range around 142. Maybe 120–160 with high
probability. That narrow probability distribution has lower entropy
than the marginal distribution. Concretely:

- H(pixel) ≈ 7.5 bits
- H(pixel | left neighbour) ≈ 4–5 bits

The difference — about 3 bits per pixel — is exactly the gap
compression can exploit.

**This is what intra prediction does** (Chapter 6). The decoder
already knows the left and above neighbours. The encoder doesn't send
the current pixel; it sends the *prediction error* — the difference
between the actual pixel and a guess based on neighbours. If the guess
is good, the error is small and lives in a narrow distribution with
low entropy. Easier to compress.

**This is also what inter prediction does** (Chapter 7). The decoder
already has the previous frame in its DPB. The encoder doesn't send
this frame's pixels; it sends the motion vector saying "this block
matches the block 4 pixels to the left in the previous frame" plus a
small correction. Same trick: use what's already known to reduce the
entropy of what needs to be sent.

> **Every codec is a machine for exposing and exploiting conditional
> structure in the source.** This is the entire point of the predict
> stage in the pipeline. Once you see this, the existence of every
> intra and inter mode in every codec spec becomes obvious — they're
> just different ways of trying to maximize what you can predict from
> already-known information.

## 23.3 The lossless ceiling

So if compression is just about exposing conditional structure, can
we keep finding more structure and compress smaller and smaller?

No. Shannon's first theorem — the **source coding theorem** — says:

> No lossless compressor can compress a source X to fewer than H(X)
> bits per symbol on average. And good compressors can get arbitrarily
> close to H(X).

So if natural video has a conditional entropy of about 3–4 bits per
pixel (with the best possible prediction), then **lossless video
compression cannot go below 3–4 bits per pixel, period**.

For an 8-bit pixel, that means lossless compression tops out around
2× (compressing 8 bits to 4 bits per pixel). For 10-bit pixels with
better intra prediction, you might do slightly better, but you're
nowhere near the 100× compression streaming services achieve.

So how does H.264 get 100× compression? Or HEVC 200×? Or AV1 300×?

**By giving up losslessness.**

## 23.4 The bargain: lossy compression

Lossless says "give me back exactly what you started with." Lossy says
"give me back something *close*." That single relaxation buys you
orders of magnitude.

Two numbers now matter:

- **Rate (R)**: how many bits per pixel you spend.
- **Distortion (D)**: how different the decoded output is from the
  original.

You can trade them off. Spend more bits → less distortion. Spend fewer
bits → more distortion. The relationship between them — the **rate-
distortion function R(D)** — is what every encoder is trying to ride.

For a given codec and content, there's an R(D) curve like this:

```text
PSNR (quality)
   │
50 ─┤                    ●●●●●  ← near-lossless region
   │                  ●●
   │              ●●●
40 ─┤          ●●●         ← typical streaming
   │       ●●●
   │    ●●
30 ─┤  ●●                  ← low-quality
   │ ●
   │●
   └──────────────────────── Rate (bits/pixel)
   0.001  0.01   0.1   1.0
```

You move along this curve by changing the codec's quality dial — QP
for H.264, CRF in x264, etc. Lower QP → up and right (more bits, more
quality). Higher QP → down and left.

The R(D) curve is *not* the same for every codec or every piece of
content. Easy content (static talking head) has a curve far to the
left (cheap to encode). Hard content (high-motion sports) has a curve
far to the right (expensive). Better codecs have curves uniformly
above worse codecs — they get more quality at every bitrate.

### The "6 dB per bit" rule

For the simplest source model (independent Gaussian noise), Shannon
proved the R(D) function has a clean closed form:

```text
R(D) = ½ · log₂(σ² / D)
```

where σ² is the signal variance and D is the squared error you're
willing to accept.

In English: **doubling the signal precision you preserve costs ½ bit
per sample.** Equivalently, every 6 dB of PSNR costs about 1 bit per
sample.

This is roughly the rate at which real R(D) curves bend across the
quality range you actually use. And it explains a Chapter 9 surprise:
H.264's QP doubles the quantizer step size every 6 QP increments,
which approximately halves the rate. Every 6 QP = 6 dB step = 1 bit
per sample. The codec's quality dial is calibrated against Shannon's
theorem.

You didn't notice this in Chapter 9 because you didn't need to. But
the table `[10, 11, 13, 14, 16, 18]` and "shift left every 6" wasn't
chosen for convenience — it was chosen because Shannon said so.

## 23.5 The encoder's algorithm — and the magic equation

Now we get to the part of this chapter that explains every encoder
ever written. Here's the situation a video encoder faces.

For the current block, the encoder must pick:

- An intra mode (1 of 9 in H.264, 1 of 35 in HEVC, 1 of 56+ in AV1).
- A motion vector (out of many candidates).
- A reference frame index.
- A transform size.
- A quantization parameter delta.
- ...

Each choice has consequences. Each choice spends some number of bits
(R) and produces some distortion (D). The encoder needs to pick the
*best combination*. But "best" needs a definition — what's the right
tradeoff between R and D?

The answer is one of the most important equations in video coding,
proposed by Sullivan and Wiegand in 1998 [SullivanWiegand1998]:

```text
J = D + λ · R
```

Read this as: "the cost of a choice is its distortion plus lambda
times its rate."

What's λ? It's a **price** — specifically, the price you're willing
to pay in distortion units for each bit you save. With units of
"distortion per bit."

- **High λ**: bits are expensive. Save bits even if it hurts quality.
- **Low λ**: bits are cheap. Spend freely for better quality.

The algorithm becomes:

```text
For each block:
  For each candidate (mode + MV + transform + ...):
    Compute D (how much error this would produce)
    Compute R (how many bits this would cost)
    Compute J = D + λ·R
  Pick the candidate with minimum J.
```

This is **Rate-Distortion Optimization** (RDO), and it's the inner
loop of every modern encoder. It's also why this book has been talking
about quality and bitrate in the same breath: from the encoder's point
of view, they're tied together by λ.

### What does λ mean in practice?

When you set a CRF / CQP / QP value in an encoder, you're really
setting λ. The encoder then computes the QP per block as a function
of λ. The bitrate you get is whatever falls out at that λ.

When you set a bitrate target (CBR), the encoder has to *find* the λ
that produces the target rate. That's what rate controllers do
(Chapter 13). Multi-pass encoders run the encode twice: once to learn
the R-vs-λ behavior of each scene, then to pick the λ that hits the
bitrate target.

In x264 specifically:

```text
λ = 0.85 · 2^((QP − 12) / 3)
```

That formula was derived by experimenting with hundreds of H.264
encodes; the *shape* (exponential in QP) is what falls out from the
Gaussian R(D) theory above.

> **Every encoder choice in every codec is an answer to "what
> minimizes J = D + λR?"**. Once you see this, the existence of all
> those modes, all those block sizes, all those entropy contexts —
> they're all just expanding the candidate space the encoder gets to
> minimize over.

## 23.6 Why this *forces* the pipeline shape

OK, so we have a mathematical framework: minimize `D + λR`. How does
that constrain the codec to predict-transform-quantize-entropy-code?

Walk through it:

**1. You need a way to spend few bits where the source is
predictable.** This is the prediction stage. Without it, you'd have
to entropy-code raw pixel values, which carry near-marginal entropy.
Prediction exposes the conditional structure of the source.

**2. After prediction, your remaining "stuff to encode" is a residual
— prediction errors.** Residuals have a special structure: they tend
to be small (because the prediction was usually right) but spatially
correlated themselves (a region with bad prediction has a cluster of
non-zero residuals). To compress them, you want to *decorrelate* them
further. That's what the transform does — it converts spatial
residuals to frequency coefficients, which (for natural content) are
much more compactly distributed. Most coefficients are near zero;
energy concentrates in a few low-frequency ones.

**3. Now you have coefficients with a nice sparse distribution. But
they're still floats / large integers. To exploit the sparsity, you
want to *throw away* the small ones — they're below the perceptual
threshold of noticeability. That's quantization.** It's the only place
in the pipeline where information is irreversibly destroyed. The
amount destroyed is governed by QP, which is set by the rate
controller, which is solving the J = D + λR optimization globally.

**4. The quantized coefficients are a long sparse integer sequence —
mostly zeros, occasional small magnitudes. To get to entropy, you need
an entropy coder.** Variable-length codes for highly skewed
distributions, arithmetic / range coding to achieve near-entropy
when the distribution is precisely known.

**5. After entropy coding, the bitstream is at or near `H(X | side
info)` bits per sample.** The Shannon lower bound. You can't do
better without breaking math.

That's the entire pipeline, derived from the rate-distortion problem.
Every stage exists because skipping it would leave money on the table
relative to Shannon's bound. The order matters: predict (to expose
conditional structure) before transform (to decorrelate residuals)
before quantize (to throw away the perceptually irrelevant) before
entropy code (to approach the bound on what's left). Skip any stage
or reorder them, and you're farther from Shannon.

**This is the answer to "why the pipeline shape?"** It's not a
fashion. It's the constructive solution to the rate-distortion
optimization problem, and every codec from 1990 through today follows
this shape because no one has yet figured out a better way.

## 23.7 The three kinds of redundancy

Once you have the framework, you can categorize where the compression
gains come from:

| Redundancy | Where it lives                    | Codec mechanism                      | Typical leverage |
|------------|-----------------------------------|--------------------------------------|------------------|
| Spatial    | Adjacent pixels in a frame        | Intra prediction + transform         | 5–10×            |
| Temporal   | Adjacent frames                   | Inter prediction (MV) + DPB          | 10–50×           |
| Perceptual | Limits of human vision            | Chroma subsampling, quantization mat | 2–4×             |

Multiply them: 5×10×2 = 100× for the leanest case; 10×50×4 = 2000×
for the most compressible content with a top-tier codec. That range
matches what real codecs achieve.

The reason **MPEG-2 only gets ~20× compression** is that it doesn't
exploit any of these well: weak intra prediction, integer-pixel
motion only, no perceptual quantization matrices. The reason **AV1
gets ~500×** is that it pushes hard on all three (56 intra modes, ⅛-
pel motion, sophisticated quantization, plus better entropy coding).

## 23.8 Comparing codecs — BD-rate

Given two codecs, how do you say which is "better"? You'd think you'd
measure quality at one bitrate, but quality varies with bitrate, and
codecs compare differently at different points on the R(D) curve.

The industry-standard answer is **BD-rate** (Bjøntegaard-delta rate),
named after the engineer who proposed it [Bjontegaard2001].

The recipe:

1. Encode the same source with each codec at four QPs (covering the
   quality range you care about).
2. Measure the rate and quality (typically Y-PSNR) at each point.
3. Fit a smooth curve through each codec's (log-rate, quality) points.
4. Compute the average percentage rate difference between the two
   curves, over the quality overlap range.

Output: a single number. "Codec A has -40% BD-rate vs Codec B" means A
uses 40% fewer bits to achieve the same quality.

Numbers people throw around:

- H.264 → HEVC: ~50% BD-rate savings.
- HEVC → AV1: ~20% BD-rate savings.
- HEVC → VVC: ~50% BD-rate savings.

These are roughly consistent with the "one codec generation = 50%
fewer bits" pattern of the field.

### Why BD-rate beats single-point comparisons

If you compare codecs at just one quality point, you can be misled.
Codec A might be 10% better at high quality and 30% worse at low
quality. Codec B might be the reverse. BD-rate averages across the
range, which captures the codec's behavior across realistic
operating conditions.

The decoder side: this is just numbers in a paper to you. But when
you read a codec announcement claiming "30% better than HEVC," that
number is BD-rate or some variant of it, and you should reach for
[Daede2020] for the methodology details before believing it.

## 23.9 The lessons that apply to your code

OK, that was the theory. What's the take-home for someone writing or
reading decoder code?

1. **Every stage of the decoder pipeline exists for a mathematical
   reason.** When you're tempted to skip a stage as "optimization,"
   you're losing ground vs the Shannon bound. Don't.

2. **QP is the master quality dial because it controls λ.** Higher QP
   = higher λ = "bits are expensive, accept more distortion." The
   encoder's mode decisions, the rate controller's allocation, the
   pipeline's behavior — all flow downstream from this.

3. **The "6 dB per bit" rule shows up everywhere** because it falls
   out of Gaussian R(D). Every codec's QP-to-step mapping has the
   "doubles every 6" structure for this reason.

4. **Decoders don't optimize anything.** They just execute the
   encoder's decisions. All the J = D + λR work happens at encode
   time. As a decoder implementer, you read bits and produce pixels.
   This is why decoders are so much simpler than encoders.

5. **Compression gains slow over time.** Each codec generation finds
   ~50% better bits at the cost of ~3–10× more compute. The
   mathematical limit (Shannon's R(D) for natural video) is a hard
   barrier, and we're getting close. AV1 to a future codec might not
   be another 50% — probably more like 20–30%, with much more compute.

## 23.10 Further reading

If you want to go deeper into the theory:

- **[CoverThomas2006]** — *Elements of Information Theory*. Chapters
  2 (entropy), 3 (asymptotic equipartition), 10 (rate-distortion).
  The textbook everyone cites.
- **[Sayood2017]** — gentler than Cover & Thomas, with worked
  examples for each concept. The chapter on arithmetic coding is the
  clearest exposition in print.
- **[Shannon1948]** — the original paper. The first ten pages are
  the founding document of the field; worth reading once in your
  life.
- **[SullivanWiegand1998]** — the J = D + λR framework explained in
  context of H.263. Short, clear, foundational.
- **[Bjontegaard2001]** — two pages, defines BD-rate. Read it.

For lecture-style coverage:

- **Stanford EE398A** (Bernd Girod) — image and video compression
  course notes.
- **Fraunhofer HHI's YouTube lectures** by Thomas Wiegand and
  colleagues — covers H.264 / HEVC / VVC theory from the people who
  helped design them.

## 23.11 Exercises

1. **Make entropy concrete.** Pick any text file you have. Run
   `gzip` and note the compression ratio. Then compute the per-byte
   entropy of the file using Python (`collections.Counter` + the
   entropy formula). Compare. The compressor approaches entropy as a
   theoretical bound.

2. **The biased coin.** Compute the entropy of a coin that lands
   heads 90% of the time. How many flips can you record in 1 bit on
   average? In 10 bits?

3. **Pixel-to-residual.** A pixel has marginal entropy ~7.5 bits.
   After good intra prediction, its residual has entropy ~4 bits.
   For a 1920×1080 frame, what's the theoretical lossless storage
   cost in each case? In bytes per frame?

4. **6 dB per bit.** Your encoder produces a 5 Mbps stream at PSNR
   38 dB. By the rule of thumb, what bitrate would the same
   encoder need for PSNR 44 dB? For 32 dB?

5. **Lagrangian intuition.** You're encoding two scenes. Scene A is
   a static talking head; Scene B is fast sports. With a fixed
   target bitrate for the whole stream, which scene gets a higher
   QP (and thus higher λ)? Why?

6. **BD-rate.** HEVC has -50% BD-rate vs H.264. If you're streaming
   at 5 Mbps H.264, what bitrate of HEVC gives roughly the same
   quality? What does that imply about CDN cost savings?

7. **The Shannon limit.** Why does lossless video compression top
   out at 2–3× while lossy compression achieves 100×? Sketch the
   argument in two sentences.

8. *(Conceptual.)* If a new codec came out tomorrow claiming 90%
   BD-rate savings over AV1, would you believe it? What questions
   would you ask?

---

Next: [Chapter 24 — Why the Pipeline Stages Are in This Order](ch24-pipeline-rationale.md).
