# Chapter 14 — Codec Architectures

> **Engineering takeaway:** Every modern video codec uses the same 10-
> stage pipeline. They differ in *what each stage allows*. This chapter
> is the column-by-column tour you've been promised since Chapter 1:
> what does H.264 put in each stage? HEVC? AV1? VP9? MPEG-2? VVC? Each
> codec's "personality" comes from the combination of its choices —
> H.264 emphasizes broad compatibility, HEVC pushes efficiency at
> hardware cost, AV1 pushes efficiency at software cost, VVC pushes
> further. By the end of this chapter you should be able to read a new
> codec spec and immediately recognize which slot each tool fits into.

This chapter pulls together everything from Chapters 5–12 and applies
it to specific codecs. We won't re-explain *how* intra prediction or
quantization work — that's Part II. Instead we'll tour each codec's
*choices* at each stage.

The goal is the mental ability to look at any new codec and ask the
right questions: "What's its block size range? Its intra mode count?
Its entropy coder? Its loop filter chain?" — and quickly orient.

## 14.1 The codec timeline

```text
1993  MPEG-2          DVD, broadcast SD                ITU/ISO joint
1996  H.263           video conferencing               ITU
2003  H.264 / AVC     ubiquitous (streaming/Blu-ray/  ITU/ISO joint
                     mobile/most everything)
2010  VP8             open alternative to H.264        Google
2013  HEVC / H.265    UHD streaming, broadcast        ITU/ISO joint
2013  VP9             YouTube, modern open codec      Google
2018  AV1             modern open codec               AOM
2020  H.266 / VVC     latest ITU/ISO codec, ~50% over HEVC
2024  ECM/EVC, MPEG5  next-gen, in research / early deployment
```

Each new codec generation gains roughly **50% BD-rate** over the
previous at similar quality — at the cost of 5–10× more decode
computation. That's the central tradeoff. Compression engineering
trades CPU cycles for bits.

The two parallel families:

- **ITU/ISO MPEG**: H.262/MPEG-2 → H.264 → H.265 → H.266 → ...
- **Open / royalty-free**: VP8 → VP9 → AV1 → AV2 (in development).

The open codecs are typically a generation or so behind the latest
MPEG codec (AV1 is roughly between HEVC and VVC in efficiency) but
are royalty-free, which matters enormously for web/open-source
deployment.

## 14.2 MPEG-2 (the baseline)

The codec that built DVD, digital TV, and Blu-ray Standard Definition.
Still in active use 30 years later for broadcast.

| Stage              | MPEG-2's choice                                       |
|--------------------|--------------------------------------------------------|
| Framing            | MPEG-2 transport stream (TS) packets                  |
| Block size         | 8×8 only                                              |
| Macroblock size    | 16×16                                                 |
| Intra prediction   | None! Only the DC value of prior block is predicted   |
| Inter prediction   | One reference frame (P) or two (B), full-pixel and    |
|                    | half-pixel MV                                         |
| Transform          | 8×8 floating-point DCT (specified loosely)            |
| Quantization       | Default matrices for intra and inter; QP scalar       |
| Entropy coding     | Variable-length Huffman codes (table-based)           |
| Loop filter        | None                                                  |
| GOP structure      | Closed GOPs with I/P/B                                |

**Distinguishing features:**

- No intra prediction (except DC). Each block is transformed and
  encoded independently within an I-frame.
- The DCT is specified "approximately" — different implementations
  produce slightly different results, which is acceptable for the
  codec's quality range.
- No loop filter; blockiness is visible at low bitrates.
- Compression gains over MPEG-1 mostly from B-frames and inter
  prediction.

MPEG-2 still ships in many DVB transmissions and DVDs. It's important
to know it exists; you won't implement a new MPEG-2 decoder in 2026.

## 14.3 H.264 / AVC (the ubiquitous one)

The codec that won the world. As of 2026, ~80% of all video on the
internet is H.264.

| Stage              | H.264's choice                                        |
|--------------------|--------------------------------------------------------|
| Framing            | NAL units in Annex B (live) or length-prefixed (MP4)  |
| Block size         | 4×4 or 8×8 (High Profile) DCT-like                    |
| Macroblock size    | 16×16, partitioned to 16×8, 8×16, 8×8, 8×4, 4×8, 4×4 |
| Intra prediction   | 9 modes for 4×4 (and 8×8 in High Profile);            |
|                    | 4 modes for 16×16; planar for chroma                  |
| Inter prediction   | Up to 5 reference frames; ¼-pel MV; B-frames; weighted |
|                    | bi-prediction; hierarchical B                         |
| Transform          | 4×4 integer transform (bit-exact); 8×8 in High Profile |
| Quantization       | QP scale doubling every 6; per-block QP delta;        |
|                    | matrix in High Profile                                |
| Entropy coding     | CAVLC (Baseline) or CABAC (Main/High)                 |
| Loop filter        | Adaptive deblocking (per-edge boundary strength + filter) |
| Profiles           | Baseline, Main, High, High 10, High 4:2:2, High 4:4:4 |

**Distinguishing features:**

- Multiple intra prediction modes (vs MPEG-2's DC-only).
- Integer-bit-exact transform (vs MPEG-2's approximate DCT).
- CABAC for High Profile — much better compression than CAVLC.
- Adaptive deblocking — modulates filter strength based on block
  context.
- Up to 5 reference frames in DPB — better matches for inter
  prediction.

**BD-rate.** ~50% better than MPEG-2 at the same quality.

**Profiles.** Most streaming uses Main or High (with CABAC). Baseline
exists for legacy mobile and is essentially deprecated. High 10 adds
10-bit content; High 4:4:4 adds 4:4:4 chroma.

**Where it lives now.** Default streaming codec (HLS, DASH), Blu-ray
HD, broadcast HD. Will dominate for another 5+ years until HEVC/AV1
fully replace it.

## 14.4 HEVC / H.265 (UHD enabler)

The successor to H.264, designed for 4K/8K. Compressed ~50% better
than H.264 at the cost of ~2-5× decode complexity.

| Stage              | HEVC's choice                                         |
|--------------------|--------------------------------------------------------|
| Framing            | NAL units, similar to H.264 with different types       |
| Block size         | 4×4, 8×8, 16×16, 32×32 transforms                     |
| CTU size           | 16×16, 32×32, or 64×64 (configurable)                 |
| Partitioning       | Quad-tree CTU → CU → PU → TU; flexible                |
| Intra prediction   | 35 modes (33 directional + DC + planar)                |
| Inter prediction   | Up to 16 reference frames; ¼-pel; AMVP; merge mode;    |
|                    | weighted bi-prediction                                 |
| Transform          | 4×4, 8×8, 16×16, 32×32 integer DCT approximations     |
| Quantization       | QP scale doubling every 6; matrix per size/type        |
| Entropy coding     | CABAC (only — no CAVLC)                                |
| Loop filter        | Deblocking + SAO (Sample Adaptive Offset)              |
| Profiles           | Main, Main 10, Main Still Picture, Range Ext           |

**Distinguishing features:**

- **Variable block sizes** — the encoder chooses block size per region,
  matching content complexity. Smooth regions use large blocks
  (32×32 transforms); detailed regions use small blocks (4×4).
- **Merge mode** — copy a neighbour's MV with no delta. ~5% of BD-rate
  gain over H.264.
- **SAO** — second in-loop filter that adjusts pixel values per
  classification.
- **Quad-tree partitioning** — recursive subdivision of CTUs gives more
  flexibility than H.264's fixed macroblock split.

**BD-rate.** ~50% better than H.264 at same quality.

**Where it lives.** UHD streaming (Netflix HEVC tier), Blu-ray UHD, HDR
broadcast. Royalty patent situation is complicated; some streamers
avoid HEVC for this reason (preferring AV1).

## 14.5 VP9 (Google's open codec)

Roughly H.264.5 — generation between H.264 and HEVC in efficiency,
royalty-free.

| Stage              | VP9's choice                                          |
|--------------------|--------------------------------------------------------|
| Framing            | IVF for raw, in WebM container otherwise              |
| Block size         | 4×4 to 64×64 with quad-tree                            |
| Intra prediction   | 10 modes (DC, V, H, D45/135/207/63, TM-predictor)     |
| Inter prediction   | Up to 8 reference frames; ⅛-pel MV; segment_id        |
| Transform          | 4×4 to 32×32 DCT or ADST                               |
| Quantization       | Per-segment QP; scalar                                 |
| Entropy coding     | Boolean coder (range coder); contexts per symbol type  |
| Loop filter        | Deblocking filter                                      |
| Profiles           | Profile 0, 1, 2, 3                                     |

**Distinguishing features:**

- Range coder (forerunner of AV1's symbol coder).
- ⅛-pel MV precision (finer than H.264/HEVC's ¼-pel).
- TM (TrueMotion) intra predictor — VP9's variant of Paeth.
- Used heavily by YouTube; supported by Chrome, Firefox.

VP9 is being phased out in favor of AV1 in Google's ecosystem but
remains widely deployed.

## 14.6 AV1 (the modern open codec)

Released 2018. The current state-of-the-art royalty-free codec.

| Stage              | AV1's choice                                          |
|--------------------|--------------------------------------------------------|
| Framing            | OBU (Open Bitstream Unit) — typed packets             |
| Block size         | 4×4 to 128×128 superblocks; recursive partitioning     |
| Intra prediction   | 56+ modes (8 directional with angle delta + DC + Paeth + |
|                    | Smooth_V/H + recursive intra + CfL + palette + IBC)   |
| Inter prediction   | Up to 7 reference frames; ⅛-pel MV; merge variants;   |
|                    | warped motion; OBMC; global motion                    |
| Transform          | DCT, ADST, IDTX, hybrid at 4×4 to 64×64                |
| Quantization       | Per-segment QP; finer step granularity                 |
| Entropy coding     | Range coder + multi-symbol entropy                     |
| Loop filter        | Deblocking + CDEF + Loop Restoration                   |
| Profiles           | Main, High, Professional                               |

**Distinguishing features:**

- **Maximum mode variety** — every stage has more options than HEVC.
- **CfL** (chroma from luma) — predicts chroma as α × luma + β.
- **Affine and warped motion** — captures zoom, rotation, perspective.
- **Three loop filters** — deblock + CDEF (ringing) + LR (detail).
- **Intra block copy** — like motion compensation within the current
  frame, great for screen content.

**BD-rate.** ~30% better than H.264; ~10–20% better than HEVC at the
same quality.

**Where it lives.** YouTube, Netflix (AV1 tier), Chrome browser; iOS
17+ has hardware decode. Expected to grow rapidly as hardware support
spreads.

## 14.7 H.266 / VVC (the latest)

Released 2020. Roughly 50% better than HEVC at the same quality,
3-10× more decode complexity. Decoder cost is the main barrier.

| Stage              | VVC's choice                                          |
|--------------------|--------------------------------------------------------|
| Framing            | NAL units (similar to HEVC's pattern)                  |
| Block size         | 4×4 to 128×128 quad-tree + triple-tree                |
| Intra prediction   | 67 modes (65 directional + DC + planar) + MIP +        |
|                    | matrix-based intra prediction (learned!)              |
| Inter prediction   | Up to 16 references; affine motion; AMVR; geometric    |
|                    | partitioning; decoder-side MV refinement (DMVR)        |
| Transform          | Multiple transform sets (MTS); LFNST                  |
| Quantization       | Same shape as HEVC but with more flexibility           |
| Entropy coding     | CABAC (refined)                                        |
| Loop filter        | Deblocking + SAO + ALF (Adaptive Loop Filter)         |
| Profiles           | Main 10, Main 10 4:4:4, etc.                          |

**Distinguishing features:**

- **Matrix-based intra prediction (MIP)** — neural-network-derived
  prediction matrices applied per block. The most "ML" thing in any
  released codec.
- **DMVR** (decoder-side MV refinement) — the *decoder* refines the
  encoder's MV using optical flow on already-decoded pixels.
- **Triple-tree partitioning** — splits 1/3 and 2/3 instead of just
  in half, giving finer adaptation.
- **ALF** — a Wiener filter with bitstream-signaled coefficients.

**Where it lives.** Still emerging; some Samsung TVs support hardware
decode; software support in FFmpeg is improving. Expected to be the
HEVC successor for broadcast and streaming once decoder availability
catches up.

## 14.8 At-a-glance comparison

| Feature              | MPEG-2 | H.264 | HEVC | VP9 | AV1 | VVC |
|----------------------|--------|-------|------|-----|-----|-----|
| Block size max       | 8×8    | 16×16 | 64×64| 64×64|128×128|128×128|
| Intra modes          | 1      | 9–35  | 35   | 10  | 56+ | 67+ |
| Inter ref frames     | 2      | 16    | 16   | 8   | 7   | 16  |
| MV precision         | ½-pel  | ¼-pel | ¼-pel| ⅛-pel|⅛-pel|⅛-pel|
| Entropy              | VLC    | CAVLC/CABAC | CABAC | Range | Range | CABAC |
| Loop filters         | none   | deblock | deblock+SAO | deblock | deblock+CDEF+LR | deblock+SAO+ALF |
| Year                 | 1993   | 2003  | 2013 | 2013| 2018| 2020|
| Relative bitrate¹    | 4×     | 2×    | 1×   | 1.3× | 0.85×| 0.5×|

¹ For the same quality. HEVC = 1× baseline. Lower is better.

## 14.9 Codec selection in 2026

What does an engineer ship with today?

- **H.264** — ubiquitous fallback. Every device supports it. Use for
  maximum compatibility, especially at low resolutions.
- **HEVC** — when you control distribution and want better quality.
  Patent situation makes adoption uneven; Apple devices love it, web
  is mixed.
- **AV1** — for new infrastructure, royalty-free, modern browsers.
  Decoder cost still meaningful in software; hardware decode is
  spreading.
- **VP9** — legacy YouTube content; phased out for AV1 in new
  pipelines.
- **VVC** — early adoption only; will become important by 2027–2028.

A typical streaming infrastructure ladder in 2026:
- H.264 baseline for ancient devices.
- H.264 main/high for SD/HD streaming.
- HEVC for UHD on supported devices.
- AV1 for modern browsers and devices that hardware-decode it.
- VP9 phased out.

## 14.10 What translates to what

When you read code for one codec, what transfers to others?

- **Bitstream I/O** — exp-Golomb in H.264/HEVC/VVC, range coder in
  VP9/AV1. The *idea* is the same; the mechanics differ.
- **Pipeline shape** — universal. The 10 stages apply.
- **Quantization** — same general structure (QP + matrix); details
  differ.
- **Intra prediction** — same neighbourhood and basic ideas; mode
  count and angle resolution grow.
- **Motion compensation** — sub-pel filters differ in tap count and
  coefficient; MV prediction grows more elaborate.
- **Entropy coding** — biggest jump. CAVLC → CABAC → range coder is a
  major shift in implementation.
- **Loop filters** — deblock is universal; HEVC adds SAO; AV1 adds
  CDEF and LR; VVC adds ALF.

If you learn one codec deeply, the next takes ~30% of the effort.

## 14.11 Where this lives in the workspace

[`codec_status.md`](../codec_status.md) tracks which codecs are
implemented in this workspace.

ProRes is the only fully-implemented codec currently. Adding H.264
would require modules for the full pipeline. AV1 adds another set of
modules. The pipeline shape stays; each codec adds its specific
choices.

## 14.12 Further reading

- **[Wiegand2003]** — H.264 overview.
- **[Sullivan2012]** — HEVC overview.
- **[Chen2020]** — AV1 overview.
- **[Bross2021]** — VVC overview.
- **[Ohm2012]** — apples-to-apples comparison of all major codecs
  through HEVC.
- **[Daede2020]** — IETF codec comparison methodology.

The respective codec specs are freely downloadable; they're the
authoritative reference for each codec's specific choices.

## 14.13 Exercises

1. **Identify the codec.** Given a stream with these features, what
   codec is it? (a) 4×4 transform, CAVLC, 16-byte macroblocks, no
   loop filter. (b) 128×128 superblocks, ⅛-pel MV, CDEF + LR.
   (c) 8×8 block-only, two reference frames, no intra prediction.

2. **Translate.** You've implemented H.264 intra prediction (9 modes
   for 4×4, 4 for 16×16). What additional modes would you need to
   add for HEVC (35 modes for variable block sizes)?

3. **Compression gain.** If HEVC is 1.0× baseline and AV1 is 0.85×
   for the same quality, what would a 10 Mbps H.264 stream be in
   AV1? In HEVC?

4. **Decode complexity.** Why does VVC have 3–10× more decode
   complexity than HEVC? (Hint: list the features VVC adds — MIP,
   DMVR, ALF, triple-tree — and consider their per-block cost.)

5. *(Reading.)* Pick any two codecs from the table in §14.8. For
   each row of the table, name a specific design decision that
   produces the listed value. (e.g., "AV1's 56+ intra modes include
   8 directional × 7 angle deltas + non-directional + special cases.")

6. **Codec selection.** Design a streaming ladder for: (a) a global
   VOD service with widely varying client capability, (b) a
   teleconferencing application with low-latency requirements.

---

Next: [Chapter 15 — Audio Coding](ch15-audio.md).
