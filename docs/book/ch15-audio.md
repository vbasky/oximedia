# Chapter 15 — Audio Coding

> **Engineering takeaway:** Audio codecs use a pipeline structurally
> similar to video — transform to frequency domain, exploit
> perceptual redundancy via psychoacoustic models, quantize, entropy
> code — but the *units* are different (samples in time, not pixels in
> space; frames are ~20ms windows, not still images). The streaming
> standard is AAC; the modern open codec is Opus; AC-3 / E-AC-3 are
> broadcast; FLAC is lossless. Streaming engineers should know AAC and
> Opus well enough to debug encode/decode chains; the rest is awareness.

This chapter is shorter than the video chapters because (a) most
oximedia-style workspaces are video-focused and (b) the underlying
ideas transfer from what you already know about transform coding.

## 15.1 What's different about audio

Audio is a 1D signal sampled at fixed rates (typically 44.1, 48, or
96 kHz). A 5-second stereo audio clip at 48 kHz is `2 × 48,000 × 5 =
480,000 samples`. At 16-bit PCM that's ~1 MB raw.

The pipeline is conceptually similar to video:

```text
PCM samples
  │
  ▼  transform (MDCT — Modified Discrete Cosine Transform)
  │
frequency-domain coefficients
  │
  ▼  psychoacoustic model (which coefficients can be thrown away?)
  │
quantized coefficients
  │
  ▼  entropy code
  │
compressed bitstream
```

Two key differences from video:

1. **Time-frequency transform.** Audio uses MDCT instead of DCT —
   *overlapping* windows so the boundaries between frames don't cause
   audible clicks. (Video doesn't worry about block boundary clicks
   in the same way.)

2. **Psychoacoustic model.** Human hearing is much more
   well-characterized than human vision in compression-relevant ways:
   masking (loud sounds hide nearby quiet sounds), absolute threshold
   of hearing (very quiet sounds aren't audible at all), critical
   bands (frequency resolution varies with frequency). Audio codecs
   exploit all these aggressively.

## 15.2 The audio "frame" — a frame is a window

In video, a frame is a still image. In audio, a **frame** (or
**packet**) is a short window of samples — typically 10–25 ms (about
480–1024 samples at 48 kHz). The encoder transforms this window to the
frequency domain, processes it, and outputs a chunk of compressed
bits.

Frame sizes are *not* aligned to fixed wall-clock intervals across
audio codecs:

- **AAC**: 1024-sample frames (~21 ms at 48 kHz).
- **Opus**: 2.5, 5, 10, 20, 40, or 60 ms frames (configurable).
- **MP3**: 1152-sample frames (~24 ms).
- **AC-3**: 1536-sample frames (~32 ms).

Containers (MP4, MKV, fMP4) carry these packets with PTS/DTS
matching the video timing model from Chapter 12.

## 15.3 AAC — the streaming standard

**Advanced Audio Coding** is the audio counterpart to H.264 in
streaming. Ubiquitous, well-supported, royalty-paid.

### Pipeline

```text
PCM samples → window selection → MDCT → TNS (temporal noise shaping)
            → MS/IS (mid-side, intensity stereo) → quantization
            → Huffman / scale-factor coding → bitstream
```

Key elements:

- **MDCT with two window sizes**: 1024-sample (long) and 128-sample
  (short). Long windows capture better frequency resolution for
  stable signals; short windows capture better time resolution for
  transients (like drum hits).
- **Window switching**: encoder detects transients and switches
  between window sizes mid-stream, with specific transition windows
  to avoid artifacts.
- **TNS** (Temporal Noise Shaping): adapts the noise envelope in time
  to match the signal's envelope, hiding quantization noise inside
  loud transient regions.
- **MS Stereo and Intensity Stereo**: exploits L/R correlation for
  stereo encoding.
- **Scale factors**: divides the spectrum into ~50 frequency bands,
  each with its own scaling factor (analogous to a per-frequency
  quantization step in video).
- **Huffman coding**: of quantized coefficients, with multiple
  codebooks selected per block.

### Profiles

- **AAC-LC** (Low Complexity): the streaming profile. ~96–256 kbps for
  stereo, often 128 kbps default.
- **HE-AAC** (High Efficiency): adds SBR (Spectral Band Replication)
  for very low bitrates (32–64 kbps for stereo).
- **HE-AAC v2**: HE-AAC + Parametric Stereo (PS) for even lower rates.

### Containers

AAC payloads ride in:

- **ADTS** (raw streaming) — each packet has a small header.
- **LATM** (broadcast) — alternative streaming wrapper.
- **MP4** — packed as sample boxes in the `mdat`.
- **fMP4** — same, in the CMAF segment structure.

### Where this matters for streaming engineers

In an HLS or DASH ladder, every video tier has a matching audio
track. Often it's a single AAC-LC 128 kbps track shared across all
video tiers; sometimes multiple bitrates. Audio sync (PTS alignment
with video) is the common bug area.

## 15.4 Opus — the modern open codec

Opus is a 2012 open audio codec optimized for everything from speech
(8 kHz, low latency) to high-quality music (48 kHz, transparent
encoding).

### Hybrid architecture

Opus combines two earlier codecs:

- **SILK** (from Skype): linear prediction (LPC) optimized for speech.
- **CELT** (Constrained Energy Lapped Transform): MDCT-based,
  optimized for music.

The encoder switches modes (or runs both in hybrid mode) based on
content. Inside CELT specifically:

```text
samples → CELT MDCT → bands (logarithmic, ~25 bands) → energy
       → pyramid vector quantization → range coder → bitstream
```

CELT's bit allocation is per-band per-frame, with the encoder
optimizing globally for J = D + λR.

### Key features

- **Low latency**: minimum 2.5 ms frame size (5 ms total latency).
  Makes Opus the choice for WebRTC.
- **Wide quality range**: from 6 kbps (intelligible speech) to 510 kbps
  (transparent stereo).
- **Royalty-free**: BSD-licensed, designed for the open web.
- **Built-in stereo coupling, multichannel support, FEC (forward error
  correction).**

### Where it lives

WebRTC (every modern teleconferencing app uses Opus). Discord. Some
streaming uses (replacing AAC where possible). Spotify Web Player
(streamed Opus).

For HLS, Opus is supported in fMP4/CMAF (RFC 7587 specifies the
payload) but legacy clients won't decode it; AAC is still the
ladder's default.

## 15.5 AC-3 and E-AC-3 (broadcast)

Dolby Digital, used in:

- DVDs and Blu-rays.
- ATSC, DVB, ARIB broadcast (every TV's audio decoder has AC-3
  built-in).
- Streaming services that target broadcast workflows.

**AC-3** is the 1992 codec; **E-AC-3** (also called Dolby Digital Plus)
is the 2005 extension with better quality and bitrate flexibility.

Conceptually similar to AAC:

- MDCT-based.
- Per-band scale factors.
- Huffman-coded.

Adds:

- **Coupling**: combines high-frequency content across channels,
  saving bits.
- **Rematrixing**: similar to MS Stereo for >2-channel content.
- **6.1 / 7.1 / Atmos support** in E-AC-3 (with object-based audio
  via the SMPTE 2098 metadata extension).

For streaming: E-AC-3 is a common second audio track in HLS for
content from broadcast workflows. CMAF supports it as a sample entry
type.

## 15.6 MP3 and FLAC (legacy and lossless)

**MP3** is from 1993, predates AAC. Still widely deployed; standardized
as MPEG-1 Layer III. Conceptually: subband filter bank + MDCT + Huffman.
Slightly less efficient than AAC at the same bitrate. You'll encounter
it in legacy content; you'll rarely encode new MP3.

**FLAC** (Free Lossless Audio Codec) is the lossless audio standard.
~2:1 compression on typical music. Used for archival, high-end audio
distribution. Decoder pipeline:

```text
PCM samples → linear prediction → residual → Rice coding → bitstream
```

Simpler than the lossy codecs above; no transform, no quantization.
The lossless guarantee is the value.

## 15.7 The audio decoder pipeline (analog to video)

For each frame in the bitstream:

1. **Parse the frame header.** Sample rate, channel count, frame size,
   profile, possibly bitstream parameters.
2. **Entropy decode.** Coefficients, scale factors, side info.
3. **Dequantize.** Multiply coefficients by per-band step sizes.
4. **Inverse MDCT.** Reconstruct time-domain samples.
5. **Apply overlap-add.** Blend with the previous frame's MDCT output
   (the "overlap" of the M*DCT* in MDCT).
6. **Apply post-processing.** SBR upsampling for HE-AAC, sample-rate
   conversion if needed.
7. **Emit PCM samples** to the audio output buffer.

The decoder produces a stream of float (or fixed-point) PCM samples
at the codec's native sample rate, which the audio system mixes and
plays.

## 15.8 Container considerations

Audio samples in MP4 / CMAF live in the same `mdat` as video, just
in a separate track. The container plumbing is identical to video
(see Chapter 16).

For HLS legacy TS:

- Audio frames are packed in PES packets, same as video.
- Multiple audio tracks (different languages, different codecs) appear
  as separate PIDs.

For DASH / CMAF:

- Audio tracks are separate adaptations in the manifest.
- Each segment is an fMP4 file with audio samples in `mdat`.

The streaming engineer's view: audio is "a track that runs alongside
video, with the same timing model." Most of the time, this is enough.

## 15.9 What you'd implement if you wrote an audio decoder

For AAC-LC (the simplest streaming-relevant codec):

```text
crates/oximedia-codec/src/aac/
  framer.rs     ← ADTS / LATM / MP4-sample framing
  parser.rs    ← frame header parsing
  huffman.rs   ← Huffman table-based decoding
  dequant.rs   ← scale-factor application
  mdct.rs      ← inverse MDCT (1024 or 128 samples)
  overlap.rs   ← overlap-add and window selection
  mod.rs       ← top-level orchestration
```

This is ~1500–2500 lines for AAC-LC, much smaller than a video codec.

For Opus, the SILK + CELT hybrid runs ~5000 lines because of the two
sub-codecs and the mode switching logic. The reference is in `opus/`.

## 15.10 Further reading

- **[BosiGoldberg2003]** — *Introduction to Digital Audio Coding and
  Standards*. The AAC bible. Covers MDCT, psychoacoustic models,
  scale factors, all foundational material.
- **[RFC6716]** — the Opus specification (also freely available from
  Mozilla / Xiph).
- **ISO/IEC 14496-3** — the AAC standard.
- **AC-3 spec** — A/52 from ATSC.
- **FLAC documentation** — at xiph.org/flac.
- The **Xiph.org wiki** is a good entry point for audio codec
  internals from the open-source perspective.

For a practical engineer's introduction, **JJ Bunn's** audio
compression tutorial pages and the Opus design papers (Valin et al.)
are clear, concise, and freely available.

## 15.11 Exercises

1. **PCM size.** A 10-minute stereo 48-kHz 24-bit PCM file is how
   large? At what AAC-LC bitrate would the equivalent compressed file
   be ~1/10 the size?

2. **Window size tradeoffs.** Why does AAC use 1024-sample windows
   for stable content and 128-sample windows for transients?
   (Sketch the time-frequency tradeoff.)

3. **MDCT vs DCT.** Why do audio codecs use MDCT instead of the DCT
   you saw in video? (Hint: think about what happens at the boundary
   between two non-overlapping DCT windows.)

4. **Opus mode selection.** Opus's encoder picks between SILK,
   CELT, or hybrid mode per frame. Predict which mode it would
   choose for: (a) clean speech at 12 kbps, (b) symphonic music at
   192 kbps, (c) speech with background music at 32 kbps.

5. *(Reading.)* In FFmpeg, look at `libavcodec/aacdec.c`. Identify
   the function that performs the inverse MDCT. Note that it's
   shared with other transform codecs in `mdct15.c`. Same code,
   different codecs.

6. **Streaming sync.** An HLS playlist has video at 30 fps and AAC
   audio at 1024-sample frames (sampling rate 48 kHz). Compute:
   what's the audio packet duration? Will video and audio packets
   align cleanly at segment boundaries?

---

End of Part III. Next: [Chapter 16 — Container Formats](ch16-containers.md).
