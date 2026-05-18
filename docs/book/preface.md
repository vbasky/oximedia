# Preface

This book exists because every other resource on video coding sits at
one of two extremes.

At one end is FFmpeg: 10,000 commits a year of working code, optimized
to the cycle, with comments that say "see spec §8.5.3" without telling
you what §8.5.3 is or why the spec wrote it that way. Read it long
enough and you'll learn the *what*; the *why* is a separate book that
was never written.

At the other end is the standards documents themselves — ITU-T H.264,
H.265, H.266, the AV1 bitstream specification. They are
extraordinarily precise descriptions of decoder behavior. They are
also written for an audience that already understands the field, by
authors whose first priority is leaving no behavior unspecified. They
do not motivate. They do not compare codecs. They do not explain why
the H.264 deblocking filter looks like it does or why AV1 has 56 intra
modes instead of 9.

Between those extremes is a stack of academic textbooks. Iain
Richardson's H.264 book is the canonical example: thorough,
well-illustrated, and accurate. Sze/Budagavi/Sullivan's HEVC book is
the same for HEVC. Charles Poynton's *Digital Video and HD* is the
color science bible. These are excellent books and you should read
them — Chapter 22 of this volume (the reading list) points at all of
them.

The gap I'm writing into is this: somebody who has just learned a
codec from one of those books still has trouble *engineering* one. The
codec books are organized around the standard they're explaining;
they don't tell you that the rate-distortion Lagrangian is one
equation that explains every encoder choice in every codec, or that
the DPB management rules are the same shape across H.264 and HEVC even
though the specs describe them differently, or that the difference
between "this codec is broken" and "this codec is fine" is most often a
color tag in a container, not a bug in the decoder.

So this book is organized around the *engineering primitives* — the
ideas you re-use across codecs. Information theory comes first because
it constrains every choice that follows. Then color, because color
bugs are the most common production bug. Then the hybrid pipeline,
stage by stage, with each stage cross-referenced to the codecs that
make non-obvious choices at that stage. Then specific codec
architectures, but only after you've seen all the stages they're built
from. Then transport, hardware, and production reality.

## Who this is for

A working software engineer who wants to contribute decoder code to
this workspace (or another like it) and currently treats the inside of
a codec as opaque. The most common reader I'm writing for has shipped
streaming infrastructure — HLS, DASH, CMAF, ABR ladders — and knows
the container/transport layers well, but has never opened a NAL unit.
Some readers will be coming from the encoder side, or from a
DSP/signal processing background; the structure of the book works for
all three, with different chapters serving as the entry point.

**No math prerequisites for the implementer track.** Strong
programming skill in any C-family language is enough. This workspace
is Rust; pseudo-code and concrete examples assume basic Rust literacy,
but the ideas transfer to C / C++ / Go without trouble. If you can
read a binary protocol spec patiently and turn it into working code,
you have the only skill that matters.

The theoretical foundations — information theory, rate-distortion,
the math of why codecs are shaped the way they are — live in **Part
VI** and are explicitly optional. They're there for readers who, after
shipping a working decoder, want to understand *why* the choices look
the way they do. Skip them on a first read and lose nothing
implementation-wise.

If you're coming from a DSP / signal processing background, you
already know more about transforms (Chapter 8) and quantization
(Chapter 9) than you need. Skim Parts I and II until you hit something
that isn't familiar. Parts III, IV, and V are where the new material
is for you.

## What this book is not

- It is not a tutorial on using FFmpeg from the command line. The
  internet has thousands of those; none of them help you understand
  what FFmpeg is doing.
- It is not a complete reference. Where a standards document covers
  something in 400 pages, this book covers it in five and tells you
  where the 400 pages are.
- It is not the latest news. By the time you read this, VVC will have
  matured further, AV1 hardware will be ubiquitous, and someone will
  have published a new perceptual quality metric that displaces VMAF.
  The fundamentals will be the same.
- It is not unbiased about implementation strategy. I work in a
  decoder workspace that emphasizes correctness-first, bit-exact
  conformance, and SIMD-aware scalar code. Other workspaces make
  different tradeoffs. Where I express a preference I'll say so.

## A note on citations

When this book makes a claim that isn't self-evident — "the H.264
deblocking filter uses a 4-tap filter on weak edges and a 5-tap on
strong" — there will be a numbered citation. The full bibliography
lives in [`bibliography.md`](bibliography.md). I have deliberately not
embedded URLs into citations: the URLs go stale, and an author + title
+ year + venue is enough for any search engine. The exception is
official ITU and AOM documents, which are freely downloadable and
which I'd want you to read first-hand.

## Acknowledgments

This book draws heavily on the codec implementations in the [oximedia
workspace](../../), particularly the [ProRes decoder](../prores_decoder.md),
which serves as the worked example for almost every chapter. The
companion documents in [`docs/`](..) (especially
[`codec_internals.md`](../codec_internals.md), [`wire_formats.md`](../wire_formats.md),
and [`rate_control.md`](../rate_control.md)) cover the same ground
from a different angle; you should read them as the engineering
counterparts to this book's theoretical chapters.

Onwards to [Chapter 0 — From HLS Segments to Pixels](ch00-from-segments-to-pixels.md).
