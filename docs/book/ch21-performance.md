# Chapter 21 — Performance Engineering

> **Engineering takeaway:** A production software video decoder spends
> 80%+ of its time in three places: entropy decoding (serial,
> hard to parallelize), motion compensation (data-parallel, heavily
> SIMD'd), and loop filtering (data-parallel, also SIMD'd). Cache
> locality matters more than instruction count. SIMD dispatch should
> be runtime-detected. Threading is at frame, slice, or tile
> granularity. Don't optimize prematurely; profile, find the hot
> path, then SIMD it.

A codec decoder is a performance-sensitive system. Hardware decode is
fastest, but a software decoder still needs to be efficient enough to
process real-time video. This chapter covers what makes a software
decoder fast, where the cycles go, and the standard optimization
strategies.

## 21.1 Where the cycles go

For a typical H.264 4K decoder, time breaks down roughly:

| Stage              | Percent of total |
|--------------------|------------------|
| Entropy decoding   | 25–40%           |
| Motion compensation| 20–30%           |
| Inverse transform  | 8–15%            |
| Loop filtering     | 15–25%           |
| Intra prediction   | 3–8%             |
| Header parsing     | <1%              |
| Memory copies      | 5–10%            |

These percentages shift with codec and content:

- **HEVC**: similar to H.264 but with more time in loop filtering
  (SAO adds work).
- **AV1**: more time in entropy (range coder) and post-filters (CDEF +
  LR).
- **High motion content**: more motion comp time.
- **Static content**: less motion comp, more entropy (more zero
  coefficients to encode).

Profile your decoder. Don't assume.

## 21.2 SIMD is essential

Modern x86 and ARM CPUs have SIMD instructions that process 16–64
bytes per instruction:

- **x86**: SSE2 (16 bytes), AVX2 (32), AVX-512 (64).
- **ARM**: NEON (16 bytes), SVE (variable, up to 256+).

Stages that are SIMD-friendly:

- **Motion compensation filters** — 4-, 6-, 7-, 8-tap separable
  filters. Each output is a small dot product of integers.
- **Inverse transforms** — small matrices, regular access patterns.
- **Loop filters** — process pixel rows independently.
- **Intra prediction** — same pattern applied to many pixels.

A scalar 6-tap filter for one output pixel: ~6 multiplies + 5 adds +
1 shift. For 16 output pixels: 96 multiplies, 80 adds.

The same in AVX2: process 16 pixels in parallel. ~6 vector multiplies,
~5 vector adds, 1 vector shift. **~16× speedup.**

For 4K decoding this is the difference between "real-time on a
midrange CPU" and "drops frames on a high-end CPU."

## 21.3 SIMD dispatch

Don't compile-time-pick one SIMD level. Different machines have
different capabilities. Use **runtime dispatch**:

```rust
fn motion_compensate(input: &[u8], output: &mut [u8]) {
    match detect_cpu_features() {
        CpuFeatures::Avx2     => motion_compensate_avx2(input, output),
        CpuFeatures::Sse42    => motion_compensate_sse42(input, output),
        CpuFeatures::Neon     => motion_compensate_neon(input, output),
        _                     => motion_compensate_scalar(input, output),
    }
}
```

Detection happens once at startup; dispatch is a function pointer
lookup. The cost is minimal compared to the inner-loop savings.

The workspace's [`simd_dispatch.md`](../simd_dispatch.md) covers the
specific dispatch strategy used here.

Every SIMD variant **must produce identical output** to the scalar
version. Bit-exact testing across SIMD levels is mandatory.

## 21.4 Cache locality

Cache misses cost ~100–300 cycles each. Cache hits cost ~1 cycle. The
difference dominates everything else.

Decoder strategies:

- **Process blocks in raster order**, keeping the same row of pixels
  hot in L1 cache.
- **Reuse intermediate buffers**: a per-block 8×8 temporary for IDCT,
  reused for the next block.
- **Avoid copying frames**: keep frame data in one buffer; consume
  references in place.
- **Align allocations**: align frame buffers to cache line boundaries
  (typically 64 bytes).
- **Avoid pointer chasing**: array-of-struct vs struct-of-array can
  matter. Inner loops should be vectorizable.

The biggest cache wins are in motion compensation, where you're
reading from random positions in reference frames. Production decoders
prefetch upcoming reference data:

```rust
__builtin_prefetch(reference_frame_position + offset);
```

## 21.5 Threading models

Decoder parallelism is at different granularities:

### Frame-level (FFmpeg's "thread_type=frame")

Multiple frames decoded concurrently, by different threads. Easy
implementation, decent parallelism. Limited by inter-frame dependencies
(B-frame ordering, references).

### Slice-level

Within a single frame, different slices are decoded concurrently.
Requires the encoder to have used multiple slices per frame.

### Tile-level (HEVC, AV1)

Within a single frame, different tiles are decoded concurrently. Each
tile is independent (no cross-tile prediction). Parallelism scales
with tile count, which the encoder configures.

### WPP (Wavefront Parallel Processing, HEVC)

A within-frame parallelism scheme: each row depends only on the row
above being far enough along. So row N starts as soon as row N-1 has
finished N+2's worth of blocks. Pipeline-style parallelism.

### Production strategy

dav1d (AV1) uses tile-level + frame-level. x264/x265 supports all
flavors. FFmpeg lets you configure.

For a typical 4K 60fps stream, you probably need 4–8 threads to
keep up on modern hardware.

## 21.6 Memory bandwidth

A 4K 60fps decoder reads/writes ~6 GB/s of YUV data (input from
reference frames, output to frame buffer). Memory bandwidth becomes a
hard constraint at 8K and beyond.

Strategies:

- **NV12 vs YUV420p planar**: NV12 (interleaved Cb/Cr) has slightly
  better cache behavior than separate planes for some operations.
- **Tile-based memory layouts**: re-pack pixels into 16×16 or 8×8
  tiles to improve cache locality. Used in some hardware decoders.
- **Direct render output**: avoid CPU↔GPU copies by decoding into
  GPU-accessible memory.

For software decoders, the optimization frontier in 2026 is memory
bandwidth, not ALU performance.

## 21.7 Profiling tools

To know where time goes:

- **perf** (Linux) — sampling profiler, function-level.
- **Instruments** (macOS) — Time Profiler, Allocations,
  System Trace.
- **VTune** (Intel) — deep profiling with cache miss rates.
- **Apple Sampler** — built into Xcode, lightweight.
- **DTrace** (Solaris, BSD, macOS) — flexible tracing.

For per-pixel inner-loop tuning:

- **`rdtsc`** (x86) — manual cycle counting around small regions.
- **VTune's microarchitectural analysis** — pipeline stalls, cache
  misses, branch mispredicts.
- **LLVM-MCA** — static analysis of expected throughput.

Profile first. Optimize the slowest 1% of code. The other 99% is
already fast enough.

## 21.8 Energy efficiency

For mobile, energy matters as much as speed:

- **Hardware decode** is 10–100× more energy-efficient than software.
  Use it when available.
- **Frame rate matching**: don't decode at higher rates than display
  needs.
- **Memory access patterns** matter for energy: sequential reads are
  cheaper than random.
- **Avoid idle/wake cycles**: keep the decode pipeline filled to avoid
  CPU sleep/wake transitions.

A 4K 60fps software decode on a modern phone can drain the battery in
30 minutes. Same on hardware: 8+ hours.

## 21.9 Production performance targets

Rough targets for software decode:

- **H.264 1080p 30fps**: any modern x86 CPU does this easily, single
  thread.
- **HEVC 1080p 60fps**: needs SIMD, single thread possible.
- **AV1 1080p 30fps**: needs SIMD, may need 2+ threads.
- **H.264 4K 60fps**: 1–2 threads with good SIMD.
- **HEVC 4K 60fps**: 2–4 threads.
- **AV1 4K 60fps**: 4–8 threads, even with dav1d-quality SIMD.
- **HEVC 8K 60fps**: hardware required for sustained playback.

If you can't hit these targets, profile and SIMD. If hardware
decode is available, use it.

## 21.10 Where this lives in the workspace

This workspace prioritizes correctness first, then SIMD optimization
on the hot paths. See:

- [`simd_dispatch.md`](../simd_dispatch.md) for SIMD strategy.
- ProRes IDCT and entropy modules have scalar implementations
  that match SMPTE RDD 36 bit-exactly. SIMD variants exist as
  separate functions selected at runtime.

For new code:

1. Write the scalar version first. Verify it against conformance.
2. Profile to find the hot path.
3. SIMD only the hot path. Verify bit-exactness.
4. Repeat.

## 21.11 Further reading

- **dav1d documentation and codebase** — the gold standard for
  AV1 SIMD optimization. Read `src/x86/itx_avx2.asm` and friends.
- **x264 / x265 codebase** — H.264 / HEVC optimizations.
- **Intel Optimization Reference Manual** — for x86 SIMD intrinsics
  and pipeline details.
- **ARM NEON Programmer's Guide** — for NEON.
- **dav1d performance papers** — search "dav1d performance optimization."

## 21.12 Exercises

1. **Profile a decoder.** Run any video decoder (FFmpeg, mpv) on a 4K
   file with profiling tools enabled. Where does time go?

2. **SIMD speedup.** A scalar 6-tap filter takes 10ns per output
   pixel. SIMD'd to AVX2 processes 16 pixels in 20ns. What's the
   speedup? What's the bandwidth requirement?

3. **Bit-exactness.** Sketch a test that verifies a SIMD intra
   predictor matches the scalar version. (Hint: enumerate inputs +
   compare outputs.)

4. **Threading.** A 4K 60fps decoder needs 240 frames per second of
   single-threaded throughput. Your single thread does 80fps. How many
   threads do you need (assuming perfect scaling)?

5. *(Reading.)* Look at dav1d's `src/x86/loopfilter_avx2.asm`. Note
   how 32 pixels are processed per inner iteration. You don't need to
   understand the SIMD; just observe the parallelism.

6. **Cache analysis.** A 4K frame buffer is 12 MB. A 64KB L1 cache
   can hold ~1.5 rows of pixels. What memory access pattern minimizes
   cache misses during decode?

---

Next: [Chapter 22 — Workflow and Production Realities](ch22-workflow.md).
