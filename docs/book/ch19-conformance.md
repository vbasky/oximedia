# Chapter 19 — Bit-Exact Conformance

> **Engineering takeaway:** A conformant decoder produces output
> bit-identical to every other conformant decoder, given the same
> bitstream. This is not a stretch goal — it is the *definition* of
> being a decoder. Conformance testing means running thousands of
> known-good bitstreams through your decoder and comparing every
> output byte to a reference. If anything diverges, you have a bug.
> Build conformance testing into your decoder from day one; do not
> chase bugs by eye.

You've now seen what a decoder does. Building one is a lot of work; building one *correctly* is a different problem entirely. Conformance is the discipline that closes the gap.

## 19.1 What "bit-exact" means

For lossy codecs you'd expect "close enough" to be acceptable. It isn't. A decoder's output must match every other compliant decoder's *exactly*, byte for byte, sample for sample.

Why so strict? Because:

1. **Reference frames are shared state.** If your decoder reconstructs a frame even one pixel value different from what the encoder reconstructed (and the encoder used in its DPB), every subsequent inter-predicted block based on that frame will drift. The error compounds over hundreds of frames. Within a few seconds, the picture is visibly wrong.

2. **Interop testing requires equality.** When 100 different decoders all claim to be H.264 compliant, the only way to verify them is to require identical output on a battery of test bitstreams.

3. **Certification bodies require it.** Streaming services, Bluray, broadcast all have certification programs. "Bit-identical to reference decoder" is the spec.

The standard documents specify decoder behavior down to the exact integer arithmetic, shift sequences, and rounding rules — for this reason.

## 19.2 What you compare against

The spec specifies bitstream syntax and decoding *procedure*. To test your decoder you need:

- A **set of test bitstreams** that exercise the relevant code paths.
- A **reference decoder** known to produce correct output.
- A **comparison method** — usually MD5 / SHA hash per frame or per file.

### Reference decoders

- **JM** for H.264. The original reference implementation, written by the standards body. Slow, but bit-exact.
- **HM** for HEVC. Same role.
- **JEM / VTM** for VVC.
- **libaom-av1** for AV1. The AOM reference encoder/decoder.
- **FFmpeg / libavcodec** as a *secondary* reference. Not the authoritative spec, but heavily tested and conformant for major codecs.

When you decode a bitstream and get a different MD5 hash from the reference, you have a bug. Period. Trace, fix, retest.

### Test bitstream suites

- **JVT-VC** for H.264 — comprehensive test suite organized by feature (each NAL unit type, each profile, edge cases). Free to download.
- **HM conformance** for HEVC.
- **AOM conformance** for AV1. Smaller but growing.
- **FFmpeg fate-tests** — a large collection of test files plus expected hashes. Practical test harness for any codec.

A typical conformance suite has hundreds to thousands of bitstreams, organized by what they test (each prediction mode, each transform size, each entropy coder, edge cases like single-frame streams, single-pixel resolution, etc.).

## 19.3 The testing methodology

```text
For each test bitstream:
    1. Decode with your decoder. Write each frame as raw YUV.
    2. Decode with the reference decoder. Write each frame as raw YUV.
    3. Compute MD5 of each frame from both decoders.
    4. Compare.
    5. If different: this bitstream exposes a bug.
```

This is essentially a giant snapshot test. Run it on every change.

In practice you'd integrate this into CI:

- New PR? Run conformance suite.
- Any failure? Block the merge.

## 19.4 Per-stage testing

Whole-decoder conformance catches everything but is slow to diagnose. Add per-stage testing:

- **Bit reader**: unit tests with known bit sequences and expected values.
- **Intra prediction**: known input neighbours + mode → expected output.
- **Inverse transform**: known coefficients → expected pixel residuals.
- **Dequantization**: known coefficients × QP → expected dequantized values.
- **Loop filter**: known input frame → expected output frame.

When a conformance test fails, per-stage tests narrow the bug rapidly:

1. Whole-stream test fails → which frame?
2. Walk the failing frame's stages individually with test fixtures from a working stream → which stage is broken?
3. Stage-level test with that exact input → reproduce the bug minimally.

This is the difference between "spend 4 hours diffing YUV files" and "spend 20 minutes localizing to the right function."

## 19.5 Bit-exactness through SIMD

When you SIMD-optimize a decoder kernel (the typical motion compensation filter, for example), you must produce *exactly the same* bits as the scalar version. SIMD introduces:

- Different rounding (especially with vectorized adds).
- Different intermediate precision (avoid wider accumulators).
- Lane-dependent operation order.

Production decoders test SIMD by comparing SIMD vs scalar outputs on millions of randomized inputs. Any mismatch → SIMD bug.

The workspace's [`simd_dispatch.md`](../simd_dispatch.md) covers the runtime CPU feature detection and per-kernel dispatch. Even with multiple SIMD variants, *all* must produce identical output on the same input. The dispatcher just picks the fastest.

## 19.6 Common conformance bugs

A short list of what tends to break:

- **Emulation prevention byte handling**: NAL units in Annex B framing have `0x03` bytes inserted to prevent start-code emulation. Forget to strip them and your parser reads wrong values.
- **Bit-reader off-by-one**: reading 9 bits when the spec says 8, or vice versa. Often appears as totally wrong field values.
- **Integer transform rounding direction**: H.264's 4×4 IDCT has specific shift-and-round rules. Round the wrong way (`>> shift` vs `(x + 1 << (shift - 1)) >> shift`) and you get a small bias that compounds.
- **CABAC context state**: applying updates to the wrong context, or in the wrong order. Look at the spec's bin-decode procedure and follow it line by line.
- **Reference picture marking**: wrongly removing a reference that's still in use, or keeping one that should be released. DPB chaos.
- **Per-block QP delta**: forgetting to apply qp_delta cumulatively (it's a *delta* from the previous block's QP, not absolute).
- **Slice boundary state reset**: many codecs reset CABAC state at slice boundaries. Forget to and the next slice is garbled.

## 19.7 When you find a bug

The discipline:

1. **Reproduce minimally.** Find the smallest input that triggers the bug — ideally a single frame or block.
2. **Find the divergence point.** Use the reference decoder + your decoder, dump intermediate values at each pipeline stage. The first stage where outputs differ is where the bug lives.
3. **Read the spec carefully** for that stage. Almost always the bug is "I missed a clause."
4. **Fix and verify.** Re-run the whole conformance suite — not just the failing test. Sometimes a fix breaks something else.
5. **Add the minimal test case to your regression suite.**

This loop dominates serious decoder development. Get good at it; the time you save by fast diagnosis is the time you spend shipping.

## 19.8 Conformance and SIMD: a small story

A real example, anonymized: a SIMD'd H.264 motion compensation 6-tap filter was off by ±1 on 0.001% of pixels. The decoder passed all single-frame tests. Production playback failed after ~3 seconds — pictures progressively desaturated, then went green. The bug: integer overflow in a 16-bit accumulator that the scalar code avoided through wider intermediates but the SIMD version did not.

The fix took 10 minutes once located. Locating it took two days because nothing in single-frame tests caught it. The drift only appeared after the bad reference frame was used for many subsequent predictions.

Lesson: **conformance tests must include long-sequence tests**, not just per-frame tests.

## 19.9 Where this lives in the workspace

This workspace's conformance approach (see [`codec_status.md`](../codec_status.md) for status):

- ProRes has reference test fixtures from SMPTE RDD 36.
- Per-stage unit tests in each module's test file.
- (Future) Full bitstream conformance tests via fate-tests-style integration.

For your own decoder work:

```text
tests/
  conformance/
    h264/
      JVT-A001.bin     ← test bitstream
      JVT-A001.md5     ← expected per-frame hashes
      ...
  unit/
    intra_test.rs      ← per-stage tests
    transform_test.rs
    ...
```

Run conformance on every change. It's the only sustainable way to ship a correct decoder.

## 19.10 Further reading

- **[Wiegand2003]** Appendix B — H.264 conformance test methodology.
- **JVT conformance test set** — search "JVT conformance H.264 download." Free from ITU/ISO.
- **AOM AV1 test set** — at aomedia.org.
- **FFmpeg fate-tests README** — explains the test harness everyone uses.

## 19.11 Exercises

1. **Trace a bug.** Imagine your H.264 decoder outputs the first 100 frames correctly, then visible artifacts. What classes of bug might cause this pattern? (Hint: think about what state accumulates across many frames.)

2. **Set up a conformance run.** Pick any codec your tooling supports (ffmpeg + reference decoder + a test bitstream). Compute the MD5 of each frame from both. Compare.

3. **Per-stage isolation.** Your H.264 IDCT is producing wrong output for some blocks. How would you build a unit test that exercises the IDCT in isolation? (Hint: dequantized coefficients + expected residual.)

4. *(Reading.)* Look at FFmpeg's `tests/fate.sh` or equivalent. How does it manage test suites? Note the simple "input → expected hash → pass/fail" structure.

5. **SIMD verification.** You've written a SIMD intra prediction for vertical mode. How do you verify it matches the scalar version on every possible input? (Hint: enumerate or randomized comparison testing.)

6. **Conformance vs subjective**. A decoder produces output that looks fine but doesn't match the reference. Is it broken? Why or why not?

---

Next: [Chapter 20 — Quality Metrics](ch20-quality.md).
