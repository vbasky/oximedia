# Chapter 13 — Rate Control (Encoder-Side Reference)

> **Engineering takeaway:** Rate control is the encoder's policy for
> choosing QP — per frame, per slice, sometimes per block — to hit a
> target bitrate or quality. The decoder never does rate control; it
> just reads whatever QP the encoder signaled. But understanding rate
> control modes (CBR / VBR / CRF / CQP) is essential for working with
> encoded files: file size, quality consistency, and seek behavior all
> derive from the rate control mode the encoder used. The workspace's
> [`rate_control.md`](../rate_control.md) covers production rate
> control in detail; this chapter is the structural overview that
> places it in context.

This is an *optional* chapter for decoder implementers. If you only
work on decoders, you can skip it. If you encode video (even just for
testing your decoder), you'll want to understand rate control modes
because they shape every file you produce.

If you're encoding directly in this workspace, the production
discussion in [`docs/rate_control.md`](../rate_control.md) is where
the deeper detail lives.

## 13.1 What rate control optimizes

The encoder has a problem: at any moment, given the input frame plus
all previously decoded reference frames, it must pick a QP (and other
parameters) for the current frame or block. The goals, ordered:

1. **Hit a bitrate target** (or quality target, depending on mode).
2. **Maintain quality consistency** — large fluctuations look worse
   than uniformly mediocre quality.
3. **Stay within buffer constraints** — the HRD says the bitstream
   must be decodable in real time on a decoder with bounded memory.
4. **Minimize encoding time** — secondary, but real.

Different rate control modes weight these goals differently.

## 13.2 The basic rate control modes

### CQP — Constant QP

The simplest. The encoder uses the same QP for every block of every
frame.

**Pros:** Deterministic. Easy to implement and test. Same QP everywhere
means the same "quality" everywhere (in a coarse sense).

**Cons:** Bitrate fluctuates wildly. A static scene at QP=20 takes few
bits; an action scene at the same QP takes many. File size is
unpredictable. Used for testing and analysis, not production.

### CRF — Constant Rate Factor (x264/x265)

CRF is "constant *perceived* quality" — the same as CQP, but with
per-frame adjustments to QP based on frame type (I/P/B) and complexity.

```text
For complex scenes: lower QP (more bits, better quality).
For static scenes:  higher QP (fewer bits, still good quality
                    because perception is forgiving).
```

The user picks a single CRF value (typically 18–28 for x264; higher
is more aggressive). The encoder picks QPs to maintain that perceptual
quality.

**Pros:** Quality is consistent across scenes. Files are smaller than
CQP at the same average quality. The user has one knob.

**Cons:** Bitrate is still unpredictable. File size can vary by 5–10×
between content of the same duration.

Used for archival, video-on-demand, and streaming where adaptive
bitrate handles the rate variation.

### CBR — Constant Bit Rate

The encoder targets a specific bitrate every second. Within a buffer
window (typically 1–2 seconds), the bitrate is approximately the
target.

**How:** The encoder adjusts QP per frame to hit the rate target. In
busy scenes, raise QP (lose quality, save bits). In quiet scenes, lower
QP (better quality, fewer bits).

**Pros:** Predictable bitrate. Buffer constraints can be tight.

**Cons:** Quality fluctuates. Busy scenes look worse than quiet ones.

Used for broadcast (where rate is fixed by channel allocation) and
some adaptive streaming ladders.

### VBR — Variable Bit Rate

A target *average* bitrate over the whole stream, with bursting
allowed for difficult content.

The encoder uses higher rates for action scenes, lower for static, but
maintains an average over a long window (often the whole file).

**Pros:** Better quality consistency than CBR. Lower average bitrate
for similar quality.

**Cons:** Bitrate is not constant; downstream buffering must absorb
the variation.

Used for VOD streaming, AVOD where average rate matters for cost
budgeting.

### ABR — Average Bit Rate

Similar to VBR with stricter average target. Most "ABR ladder"
streaming uses this.

### Two-pass and multi-pass

The encoder runs once to collect statistics (per-frame complexity,
optimal QP estimates), then runs again with that information to make
better decisions. Multi-pass extends to 3+ passes for marginal gains.

**Pros:** Best quality at a given bitrate target. Standard for VOD
where encode time isn't critical.

**Cons:** Cannot be done in real time. Adds significant encode time.

## 13.3 Per-frame QP decisions

Within a rate control mode, the encoder picks per-frame QP based on
frame type:

```text
I-frame QP base:   QP_base
P-frame QP:        QP_base + 1 to +3
B-frame QP:        QP_base + 3 to +6 (higher because B-frames are
                                       less referenced)
```

The exact offsets depend on the mode. For CRF, offsets are larger
(higher QP for B-frames where quality matters less). For CBR, offsets
are smaller (more uniform quality across frame types).

### Why offset by frame type?

I-frames are referenced by every subsequent frame in the GOP. Quality
errors in I-frames propagate. → Lower QP on I-frames.

P-frames are referenced by subsequent P-frames and B-frames. Errors
propagate, but less than I-frame errors. → Slightly higher QP.

B-frames are typically not referenced (in non-hierarchical GOPs).
Errors in B-frames don't propagate. → Higher QP, fewer bits.

In hierarchical B configurations, deeper-level B-frames get even
higher QP because they're least referenced.

## 13.4 Adaptive quantization (AQ)

Within a single frame, the encoder can adjust QP per block. **Adaptive
quantization** uses perceptual cues:

- **Brightness AQ** — lower QP in dark areas (where banding is visible)
  and raise it in bright areas (where masking hides errors).
- **Variance AQ** — lower QP in areas with low variance (smooth, easy
  to see quantization) and raise in high-variance areas (busy texture
  masks errors).
- **Edge AQ** — lower QP near sharp edges (where ringing artifacts are
  most visible).

x264's `--aq-mode` is the canonical implementation. Different values
enable different combinations.

The decoder reads each block's effective QP (base + delta) and
dequantizes accordingly. AQ is invisible to the decoder.

## 13.5 Per-shot rate control (modern)

Netflix and others have moved to **per-shot rate control**: detect
scene boundaries, then allocate bits per shot based on its complexity
and importance. Within a shot, use a constant CRF or QP; across shots,
let the rate vary.

The benefit: scenes that are easy to encode well at lower bitrate
release bits to scenes that need more.

This requires shot detection and a higher-level orchestration. The
decoder still doesn't care — it just reads QPs.

## 13.6 The HRD revisited

The HRD (Hypothetical Reference Decoder, see Chapter 12.10) imposes
constraints on the bitstream that the rate controller must respect:

- **CPB don't-overflow**: bits arrive at a target rate; the buffer
  fills as bits arrive and empties as frames are decoded. Never let it
  overflow.
- **CPB don't-underflow**: never let it run empty before the next
  frame is needed.

For CBR, the rate controller is essentially solving "how do I keep the
HRD CPB at the target fill level?" For VBR, the constraint is looser.

The decoder doesn't compute the HRD; it trusts the encoder. But every
codec's profile/level has a maximum HRD buffer size that the bitstream
must respect.

## 13.7 What the decoder sees

To make this concrete: from the decoder's perspective, the rate
controller's work shows up as:

- The slice header's QP value.
- Per-block `qp_delta` adjustments.
- Frame-type designation (I/P/B).

That's it. The encoder might be doing CBR with VBV constraints, or
CRF with AQ, or per-shot allocation — the decoder reads the QP and
moves on.

This is why a decoder is a much simpler thing than an encoder. The
encoder solves a constrained optimization problem in real time; the
decoder just executes the encoder's decisions.

## 13.8 Where this lives in the workspace

The workspace's [`docs/rate_control.md`](../rate_control.md) is the
production-grade discussion — modes, the actual algorithms used in
mainstream encoders, buffer model arithmetic, two-pass mechanics. Read
it after this chapter for the implementation depth.

This workspace is decoder-first, so it doesn't currently have an
encoder rate controller. If/when an encoder is added, the rate control
module would be:

```text
crates/oximedia-encoder/src/rate_control/
  cbr.rs
  vbr.rs
  crf.rs
  aq.rs (adaptive quantization)
```

For studying production rate control implementations:

- **x264** `encoder/ratecontrol.c` — the canonical reference for
  CRF, 2-pass, and AQ implementations.
- **x265** `source/encoder/ratecontrol.cpp` — HEVC equivalent.
- **rav1e** `src/rate.rs` — Rust implementation of CRF and CBR for
  AV1.
- **SVT-AV1** rate control — designed for parallelism, used in
  Netflix's encoding pipeline.

## 13.9 Further reading

- **[SullivanWiegand1998]** — the J = D + λR framework that
  underlies all modern rate control.
- **[Wiegand2003]** §V — H.264 rate control (high level).
- **Netflix Tech Blog** posts on per-shot rate control and
  per-title encoding strategies.
- **x264 documentation** on CRF semantics and AQ modes (in the source
  tree under `doc/`).
- **[DarkShikari]** blog posts on x264 rate control implementation
  details.

The production discussion in this workspace's
[`docs/rate_control.md`](../rate_control.md) is where to go for the
detailed mechanics.

## 13.10 Exercises

1. **Mode selection.** You're encoding a 1080p HEVC stream for:
   (a) a fixed-bandwidth broadcast channel, (b) Netflix's ABR ladder,
   (c) a YouTube upload, (d) a livestream over WebRTC. Which rate
   control mode fits each?

2. **CRF semantics.** Encode the same source with `x264 --crf 18`
   and `x264 --crf 28`. Predict (without running) the ratio of file
   sizes. (Hint: a 10-point CRF change is roughly 6.6 QP, which is
   roughly 2× bitrate per the doubling rule.)

3. **Two-pass logic.** Why does two-pass encoding produce better
   quality than single-pass at the same bitrate target? (Hint: think
   about what information the encoder has access to in each pass.)

4. **AQ tradeoff.** Adaptive quantization raises QP in busy
   textured areas and lowers it in smooth dark areas. Why is this a
   net quality win even though the busy areas now have *more*
   quantization noise?

5. *(Reading.)* Look at x264's `encoder/ratecontrol.c`. Find the
   function that computes per-frame QP given the target bitrate.
   Note the use of historical statistics (per-frame complexity from
   previous frames). This is the "VBV" buffer model in action.

6. **HRD constraint.** A 5 Mbps stream over a network that delivers
   bits at exactly 5 Mbps. The CPB size is 2 seconds of bits = 10
   Mbits. What's the longest a single difficult frame can be without
   underflowing the CPB? (Sketch the buffer arithmetic.)

---

End of Part II. Next: [Chapter 14 — Codec Architectures (H.264, HEVC, AV1, VP9, MPEG-2, VVC)](ch14-codec-architectures.md).
