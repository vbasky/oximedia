# Chapter 25 — Rate Control: The Math Behind QP Selection

> **Engineering takeaway:** A rate controller is a closed-loop system
> that picks the QP for each frame (and sometimes each block) to hit
> a bitrate or quality target while keeping the bitstream valid for
> downstream decoders. Internally it uses the Lagrangian J = D + λR
> from Chapter 23 to evaluate candidate QPs, with feedback from
> already-encoded frames driving its choice for the next frame. This
> chapter walks through the actual algorithm — how λ gets picked, how
> the buffer constraints interact, why two-pass works. The math is
> mostly the math you saw in Chapter 23, applied operationally.

In Chapter 13 you saw the *modes* of rate control: CBR, VBR, CRF,
CQP. They're the user-facing names. In Chapter 23 you saw the
*equation* every encoder is solving: minimize J = D + λR. This
chapter connects the two: how does an encoder, in real time, pick the
QP that produces the right J?

The connection is a *control system*. The encoder makes a guess at
QP, encodes a frame, measures what came out, adjusts. Over many
frames, the feedback converges toward the target. The math is the
math of closed-loop control, applied to compression.

## 25.1 The QP-to-λ relationship

Recall from Chapter 23 that **λ is the "price" of a bit in distortion
units**. Higher λ = bits are expensive = save them. Lower λ = bits
are cheap = spend freely.

QP, in turn, is the user-facing knob that determines how aggressive
the quantization is. Higher QP = more loss per coefficient = fewer
bits per block.

These two concepts are tightly linked because **the right QP for a
given λ is determined by the rate-distortion theory**.

In H.264, the relationship (derived empirically and used by x264) is:

```text
λ = 0.85 · 2^((QP − 12) / 3)
```

This isn't an arbitrary formula. It falls out of:

1. The Gaussian R(D) curve: R = ½ log₂(σ²/D), so D = σ² · 2^(-2R).
2. The Lagrangian first-order condition: dD/dR = -λ.
3. The H.264 quantizer's design: step size doubles every 6 QP.

Working through the calculus (which you don't need to follow in
detail), you get: **λ is approximately proportional to the squared
quantization step**. In H.264, the step doubles every 6 QP, so λ
doubles every 3 QP — explaining the `2^((QP-12)/3)` shape.

For the encoder's optimization to make sense, you need *consistent*
λ across the frame. Whatever QP the encoder picks for the frame
defines the λ at which all the J = D + λR evaluations are done.

## 25.2 The simplest rate controller: constant QP

The baseline. Pick a QP at start; use it everywhere; never adapt.

```rust
fn rate_control_constant(qp: u8) -> u8 {
    qp
}
```

That's it. No optimization, no feedback. Use this when you want
predictable behavior for testing, or when you can afford uniformly
distributed bits.

The result: variable bitrate. Easy scenes use few bits; busy scenes
use many. The output file size is whatever falls out.

## 25.3 CRF: constant rate factor

CRF is "constant *perceived* quality" — slightly more adaptive than
CQP. It still picks a base QP per frame, but adjusts:

1. **Frame type offset**: I-frames get lower QP (worth more bits;
   referenced by many P/B frames). B-frames get higher QP (less
   referenced).
2. **AQ adjustment**: per-block QP delta based on visual masking
   properties.

```rust
fn rate_control_crf(crf: f32, frame_type: FrameType) -> u8 {
    let base = (crf * 1.0).round() as u8;
    match frame_type {
        FrameType::I => base - 2,
        FrameType::P => base + 1,
        FrameType::B => base + 3,
    }
}
```

The encoder also applies adaptive quantization (AQ) — adjusting QP
per block based on perceptual considerations. The result: variable
bitrate, more consistent perceived quality.

CRF doesn't use feedback from prior frames. The current frame's QP is
purely a function of the CRF value and the frame type. That's why CRF
is "open loop" — easy to implement, but doesn't hit a bitrate target.

## 25.4 CBR / VBR: the feedback loop

CBR and VBR introduce **feedback**. The encoder has a target bitrate.
It must adjust QP per frame so that, over a window of time, the
average bitrate matches the target.

The mechanism: a **buffer model**, the same VBV (Video Buffer
Verifier) model that the HRD uses (Chapter 12). Conceptually:

```text
buffer_fill = buffer_fill + (bits_arriving) - (bits_for_this_frame)
            = buffer_fill + (rate / fps) - bits_for_this_frame
```

If the buffer is filling toward overflow, the encoder needs to spend
more bits per frame (raise quality at this rate). If the buffer is
draining toward underflow, the encoder needs to spend fewer bits per
frame (raise QP, lose quality, save bits).

In code, very roughly:

```rust
fn rate_control_cbr(
    target_bitrate: u32,
    fps: f32,
    buffer_fill: u32,
    buffer_target: u32,
    last_qp: u8,
) -> u8 {
    let bits_per_frame_avg = (target_bitrate as f32 / fps) as u32;
    let buffer_pressure = buffer_fill as f32 / buffer_target as f32;

    if buffer_pressure > 1.1 {
        last_qp.saturating_sub(1)        // overflowing → spend more
    } else if buffer_pressure < 0.9 {
        last_qp.saturating_add(1)        // underflowing → spend less
    } else {
        last_qp                          // close to target
    }
}
```

Real implementations are far more sophisticated — gain scheduling, PID-
style controllers, look-ahead predictions of upcoming complexity — but
the core is "look at the buffer, adjust QP accordingly."

### Why simple controllers oscillate

A naïve controller using just buffer fill responds slowly. If
complexity spikes (busy scene), the controller spends bits, the
buffer empties, the controller raises QP, the buffer fills, etc. The
result: QP oscillates with the content, producing visibly varying
quality.

Production controllers add:

- **Look-ahead**: peek at the next N frames' complexity to anticipate.
- **VBV buffer reservation**: avoid running the buffer to within 100ms
  of underflow.
- **Per-frame complexity estimation**: use SATD or DCT-energy proxies
  to predict bit cost before encoding.

x264's rate controller is the canonical "production grade" example;
its `encoder/ratecontrol.c` is hundreds of lines of feedback logic.

## 25.5 Two-pass rate control

The big insight: if you could *know* each frame's R(D) curve before
encoding, you could solve for the optimal λ analytically — pick the
λ that makes the integral of R(λ) over all frames equal the budget,
then encode each frame at that λ.

Two-pass does exactly this:

- **Pass 1**: encode the whole file at constant QP (or constant λ),
  but only collect statistics — per-frame complexity, per-scene
  characteristics, per-frame bit cost. Don't keep the output.
- **Pass 2**: encode again. Use the pass-1 statistics to solve for
  per-segment λ that hits the bitrate target while maintaining
  uniform quality.

The result: better quality at the same bitrate than single-pass, by
~5–15% BD-rate. Worth it for VOD.

### How pass 2 solves for λ

Roughly: each scene has a measured R-vs-QP function from pass 1. The
encoder wants a target total bitrate. So it picks a global λ such
that:

```
sum over scenes [ R(scene, λ) ] = total_target_bits
```

This is a 1D root-finding problem. x264 uses bisection or Newton's
method on λ. Each evaluation of the sum is a closed-form computation
based on pass-1 statistics.

Per-scene, then per-frame, then per-block (with AQ), the encoder
applies the λ-derived QP. Multi-pass extends this to 3+ passes for
marginal additional gain.

## 25.6 Adaptive quantization: per-block QP

Within a single frame, AQ adjusts QP per block based on perceptual
factors. The motivation: human vision is *non-uniform*:

- **Brightness**: errors in dark areas are more visible (the eye
  notices banding).
- **Variance**: errors in busy textured areas are *less* visible
  (masking by complexity).
- **Edge proximity**: ringing near edges is highly visible.

x264's `--aq-mode` implements several variants:

- **Mode 1**: variance-based. Lower QP where local variance is low.
- **Mode 2**: auto-variance — adaptive to content.
- **Mode 3**: variance-based with a bias toward dark areas.

The effect: a brightness-uniform frame might use QP 22 globally; with
AQ, dark patches get QP 20 and busy patches get QP 24. Same average
bitrate, better perceived quality.

The decoder reads each block's effective QP (base + delta) and
dequantizes accordingly. AQ is invisible from the decoder's view.

## 25.7 Where the J = D + λR framework actually appears

OK so the rate controller picks QP per frame and per block. But the
encoder is *also* making other choices — which prediction mode to use,
which transform size, which motion vector. Each of those choices also
has R and D contributions.

For each choice, the encoder evaluates:

```
J_candidate = D_candidate + λ · R_candidate
```

Pick the candidate with minimum J. This is **Rate-Distortion
Optimization (RDO)**.

The encoder uses *the same λ* derived from the QP for the frame. So
the rate controller's QP choice doesn't just affect quantization
directly — it propagates to every coding choice (mode, MV,
partitioning) made within that frame.

This is why setting CRF in x264 controls everything: the CRF picks
QP, QP picks λ, λ drives RDO, RDO drives mode choices, mode choices
drive the bitstream.

## 25.8 The HRD constraint

The bitstream must be decodable in real time on a hypothetical decoder
with bounded memory. This means:

- **CPB buffer**: a hypothetical input buffer that fills at the
  target rate and empties as frames are decoded. Must never
  underflow (decoder runs out of bits) or overflow (encoder produces
  more bits than the buffer can hold).
- **DPB**: must respect the level's maximum frame count.

For CBR, the rate controller's job is essentially "keep the CPB
within its bounds while approaching the rate target." For VBR with
HRD constraints, the constraint is looser — the CPB can fluctuate
more, but still bounded.

The HRD is *abstract* — it's a model, not a real decoder. But its
constraints are real: a bitstream that violates HRD won't play on
real-time decoders. The encoder's rate controller is essentially
solving an optimization with the HRD as a hard constraint.

In CBR/VBR pseudocode:

```rust
fn pick_qp_with_hrd(target_qp: u8, cpb_state: &CpbState) -> u8 {
    if cpb_state.would_underflow_at_qp(target_qp) {
        increase_qp_to_avoid_underflow(target_qp, cpb_state)
    } else if cpb_state.would_overflow_at_qp(target_qp) {
        decrease_qp_to_avoid_overflow(target_qp, cpb_state)
    } else {
        target_qp
    }
}
```

This is the safety net. The rate controller picks a target; the HRD
check clamps if needed.

## 25.9 Per-shot rate control (modern)

Netflix-style per-shot rate control treats each shot as an independent
optimization unit:

1. **Detect shot boundaries** (heuristic: scene cuts, large frame-to-
   frame delta).
2. **For each shot, encode at multiple QPs** (a pilot pass).
3. **Choose per-shot QP** that hits a target VMAF or PSNR.
4. **Encode the final** with per-shot QP.

The result: each shot is optimally allocated. Static dialogue gets
fewer bits; explosions get more. Same total bitrate, much better
perceived quality.

This is essentially per-shot two-pass rate control with VMAF (or
similar) as the quality target instead of PSNR.

## 25.10 The mathematician's view, briefly

If you really want the full math:

The constrained problem: minimize D subject to total R ≤ B.

The Lagrangian relaxation: minimize D + λR, where λ is chosen so
the constraint is satisfied.

Lagrangian duality (Karush-Kuhn-Tucker conditions) guarantees that
*for some λ*, the unconstrained problem's minimum equals the
constrained problem's minimum. So the optimization decomposes:

```text
min over all choices: D + λR
```

For each block independently. The local optima compose into a global
optimum at the right λ. The encoder's job is then to find that λ.

In practice: the encoder picks λ from QP (using the empirical formula
of §25.1), and computes everything else relative to that. The rate
controller's job is to pick QP such that the resulting λ satisfies
the bitrate target.

It's beautifully elegant in theory. In practice, you implement an
approximate version with empirical tuning, and call it good enough
for production.

## 25.11 Where this lives in the workspace

For encoder rate control (when an encoder is added to this
workspace), the structure would be:

```text
crates/oximedia-encoder/src/rate_control/
  state.rs          ← buffer model, frame counters
  constant_qp.rs    ← CQP mode
  crf.rs            ← CRF / VBV mode
  cbr.rs            ← CBR with VBV
  two_pass.rs       ← stats collection + replay
  aq.rs             ← adaptive quantization
  lambda.rs         ← QP-to-λ conversion
```

For reference implementations:

- **x264**'s `encoder/ratecontrol.c` — the industry-standard production
  rate controller for H.264. Hundreds of lines of careful feedback
  logic.
- **x265**'s rate control — HEVC analog.
- **rav1e**'s `src/rate.rs` — Rust implementation for AV1.

The workspace's [`docs/rate_control.md`](../rate_control.md) covers
the practical engineering details. This chapter is the theoretical
basis.

## 25.12 The closing observation

Every rate control mode is a different answer to the same underlying
question: **what λ should I use?**

- **CQP**: ignore the question; just pick a QP.
- **CRF**: pick λ that produces "constant perceived quality," whatever
  that means for the encoder.
- **CBR**: pick λ that produces a target bitrate, adjusted by VBV
  feedback.
- **VBR**: pick λ that produces an average bitrate, with looser
  short-term constraints.
- **Two-pass**: solve for the optimal λ using pass-1 statistics.
- **Per-shot**: solve for per-shot λ with quality (not rate) as the
  primary target.

The math behind each is roughly the same. The implementations vary
in how cleverly they pick λ and how much feedback / look-ahead they
use.

## 25.13 Further reading

- **[SullivanWiegand1998]** — the original J = D + λR paper. Read
  this once.
- **[Wiegand2003]** §V — H.264 rate control.
- **[Sullivan2012]** §VIII — HEVC rate control extensions.
- **Netflix Tech Blog** — per-shot encoding posts (Aaron et al.).
- **x264 documentation** in `doc/ratecontrol.txt` of the source tree.
- **[DarkShikari]** blog posts on x264 rate control internals.

## 25.14 Exercises

1. **Buffer arithmetic.** A 5 Mbps CBR stream over a 5 Mbps channel
   with a 2-second VBV buffer (10 Mb capacity). The encoder produces
   a 1-second I-frame of 8 Mb. What's the buffer fill before and
   after this frame? Will it underflow before the next frame?

2. **CRF vs CBR.** Encode the same source with x264 at `--crf 22`
   and at a CBR matching the resulting bitrate. Compare the two
   files. Where do they differ in quality?

3. **The 6 dB rule.** Your encoder produces a 4 Mbps stream at PSNR
   40 dB. To get PSNR 46 dB (much higher quality), what bitrate do
   you need? (Hint: 6 dB per bit. So per-pixel bits roughly double.)

4. **Two-pass logic.** Two-pass works by collecting per-frame
   statistics in pass 1 and using them in pass 2. What specific
   statistics would you collect? (Hint: bit cost at default QP,
   SATD/SAD complexity, frame type.)

5. **AQ trade-off.** Adaptive quantization can *increase* QP in busy
   areas to save bits. Why doesn't this make the busy areas look
   worse? (Hint: think about visual masking.)

6. **Per-shot rate control.** Sketch a per-shot rate controller. What
   inputs does it need? What outputs? Where does the math from §25.10
   appear?

7. **Production scaling.** Netflix encodes thousands of titles
   simultaneously. What computational steps in this chapter would
   you parallelize? Which would you cache?

---

End of Part VI. **End of the book.**

You can now ship decoder code, debug production codec bugs, and
understand the math behind compression. The pipeline is the pipeline;
the math is the math. Every codec that ever comes is a different set
of choices in the same framework.

For practical worked examples that didn't fit inline, see the
[Appendices](README.md#back-matter). For deeper references on any
topic, see the [Bibliography](bibliography.md).

For the workspace itself, start contributing at
[`codec_status.md`](../codec_status.md) — the codecs in development,
the open issues, the next steps.

Welcome to the field.
