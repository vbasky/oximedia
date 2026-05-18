# Video Codec Engineering — A Book

> A working engineer's textbook. Decoder-first. Implementation-oriented.
> Written for someone with strong programming background and *no* DSP or
> information-theory prerequisites. The goal is to take you from "I know
> HLS/DASH from the outside" to "I can contribute decoder code to a
> real codec workspace."

This directory contains the full-depth treatment that the
[`video_codec_engineering.md`](../video_codec_engineering.md)
curriculum points to. The curriculum is the map; this is the
territory.

## Who this is for

If you've shipped streaming infrastructure (HLS, DASH, CMAF), you've
already touched codecs from the outside. The container, the manifest,
the ABR — all familiar. What's been opaque is **what's inside the
segments** and what a decoder *does* with those bytes.

This book opens that black box. You'll come out the other side able
to read a bitstream byte by byte, understand what each stage of the
decode pipeline does, and contribute decoder code to this workspace
or another.

**Prerequisites.** Strong programming skill in any C-family language
(this workspace is Rust; pseudo-code and concrete examples assume
basic Rust literacy, but the ideas transfer). No math beyond high-
school algebra is required for the implementer track. Any time the
book reaches for heavier math, it's in a section marked `★` that you
can skip and still implement.

## How to read this book

There are two reading paths:

- **Implementer track (default).** Read Ch 0 → Ch 1 → Ch 2, then jump
  to the pipeline chapter (5–13) matching your current task. The
  theory chapters in Part VI are *optional*. Engineering takeaway
  boxes at the top of every chapter give you the gist.

- **Theory track.** Read in order, including all the `★` asides and
  Part VI. Pursue this if you want to know *why* codecs look the way
  they do, beyond just *how to implement* them.

Each chapter ends with:

- **Further reading** — numbered citations into the
  [bibliography](bibliography.md). Author + title + venue + year. No
  fabricated URLs.
- **Exercises** — usually one or two hands-on (read or modify code in
  this workspace, run a real tool) plus a few paper exercises for
  readers who like them.

## Table of contents

- [Preface](preface.md)

### Part I — Foundations (the decoder, end to end)

- [Chapter 0 — From HLS Segments to Pixels](ch00-from-segments-to-pixels.md)
- [Chapter 1 — Build a toy decoder](ch01-toy-decoder.md)
- [Chapter 2 — Bitstream I/O, exp-Golomb, and reading specs](ch02-bitstream-io.md)
- [Chapter 3 — Color, pixels, and human vision](ch03-color-and-vision.md)
- [Chapter 4 — Transfer functions and HDR](ch04-transfer-and-hdr.md)

### Part II — The Hybrid Block-Based Pipeline (stage by stage)

- [Chapter 5 — Pipeline overview](ch05-pipeline.md)
- [Chapter 6 — Intra prediction](ch06-intra.md)
- [Chapter 7 — Motion estimation and compensation](ch07-motion.md)
- [Chapter 8 — Transforms](ch08-transforms.md)
- [Chapter 9 — Quantization](ch09-quantization.md)
- [Chapter 10 — Entropy coding](ch10-entropy.md)
- [Chapter 11 — Loop filtering](ch11-loop-filter.md)
- [Chapter 12 — Reference picture management (DPB)](ch12-dpb.md)
- [Chapter 13 — Rate control (encoder-side reference)](ch13-rate-control.md)

### Part III — Codec Architectures

- [Chapter 14 — H.264, HEVC, AV1, VP9, MPEG-2, VVC contrasts](ch14-codec-architectures.md)
- [Chapter 15 — Audio coding](ch15-audio.md)

### Part IV — Containers, Transport, Hardware

- [Chapter 16 — Container formats](ch16-containers.md)
- [Chapter 17 — Streaming protocols (HLS/DASH/CMAF/LL-HLS/WebRTC)](ch17-streaming.md)
- [Chapter 18 — Hardware acceleration](ch18-hardware.md)

### Part V — Engineering Practice

- [Chapter 19 — Bit-exact conformance](ch19-conformance.md)
- [Chapter 20 — Quality metrics](ch20-quality.md)
- [Chapter 21 — Performance engineering](ch21-performance.md)
- [Chapter 22 — Workflow and production realities](ch22-workflow.md)

### Part VI — Why Codecs Look the Way They Do

The theoretical foundations behind the practical decoder work in
Parts I–V. Read these once you've made it through the rest — they
explain *why* the pipeline has its specific shape, what one equation
drives every encoder decision, and how rate controllers actually pick
QPs.

The voice matches the rest of the book: programming analogies first,
math only when it earns its keep.

- [Chapter 23 — Why Codecs Look the Way They Do](ch23-information-theory.md)
- [Chapter 24 — Why the Pipeline Stages Are in This Order](ch24-pipeline-rationale.md)
- [Chapter 25 — Rate Control: The Math Behind QP Selection](ch25-rate-control-theory.md)

### Back matter

- [Bibliography](bibliography.md)
- [Appendix A — A complete CABAC trace](appendix-a-cabac-trace.md)
- [Appendix B — A complete 4×4 IDCT walk-through](appendix-b-idct.md)
- [Appendix C — Glossary](appendix-c-glossary.md)

## A note on scope

This is approximately 100 printed pages of implementer-focused
content (Parts I–V), plus another 30–50 pages of optional theory
(Part VI). It is deliberately not the ~800-page format of Richardson,
Sze/Budagavi, or the ITU specifications themselves — those exist, and
this book points at them when you need them. What this book tries to
add is the *connective tissue*: the why-it-is-shaped-this-way reasoning
that the standards documents omit because they're optimized for being
implementable, not learnable, and the practical bitstream-reading
skill that no textbook teaches.

## Status

**Book complete.** ~68,000 words / ~190–220 typeset pages.

- ✅ Front matter (preface, README, bibliography with ~60 citations)
- ✅ Part I — Foundations (Ch 0–4)
- ✅ Part II — The Hybrid Block-Based Pipeline (Ch 5–13)
- ✅ Part III — Codec Architectures (Ch 14–15)
- ✅ Part IV — Containers, Transport, Hardware (Ch 16–18)
- ✅ Part V — Engineering Practice (Ch 19–22)
- ✅ Part VI — Why Codecs Look the Way They Do (Ch 23–25)
- ✅ Appendix A — A Complete CABAC Trace
- ✅ Appendix B — A Complete 4×4 IDCT Walkthrough
- ✅ Appendix C — Glossary

The book is ready to read. Start with the [Preface](preface.md), then
[Chapter 0](ch00-from-segments-to-pixels.md).
