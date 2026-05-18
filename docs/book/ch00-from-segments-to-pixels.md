# Chapter 0 — From HLS Segments to Pixels

> If you've shipped HLS or DASH, you have already touched video codecs
> from the outside. The manifest, the segments, the ABR ladder — all
> familiar. What you may not have done is opened a segment and read
> what's inside, byte by byte. This chapter does exactly that. By the
> end you'll know what a NAL unit is, what tools crack one open, and
> what the rest of this book is *for*.
>
> **Engineering takeaway:** the bitstream is a serialized data
> structure with a published spec. It is not magic; it is bytes laid
> out in fields, and you can read it with the same patience you read
> any binary protocol. Internalize this and codec work stops feeling
> like dark art.

## 0.1 The layer cake you already know — and the one you don't

Modern streaming has, very roughly, four layers stacked on top of
each other:

```text
┌──────────────────────────────────────────────────────────┐
│  Layer 1 — Delivery                                      │
│  HLS .m3u8 / DASH .mpd manifest, HTTP fetches, ABR logic │
└──────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────┐
│  Layer 2 — Segment                                       │
│  ~2–6 s file: .ts (legacy HLS) or .m4s / .mp4 (CMAF)     │
└──────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────┐
│  Layer 3 — Container                                     │
│  TS packets (188-byte) or ISOBMFF boxes (ftyp, moov, …)  │
│  Carries metadata, timing, and the encoded payload       │
└──────────────────────────────────────────────────────────┘
                          ↓
┌──────────────────────────────────────────────────────────┐
│  Layer 4 — Bitstream  ← THIS BOOK STARTS HERE            │
│  NAL units (H.264/HEVC) or OBUs (AV1); inside each, the  │
│  compressed slice data — your codec payload              │
└──────────────────────────────────────────────────────────┘
                          ↓
                  decoded YUV pixels
```

If you've done streaming work, you've probably treated layers 1–3 as
*diagnoseable* (you can crack open a manifest, you can read an
ISOBMFF box header, you know what `EXT-X-DISCONTINUITY` means) but
layer 4 as *opaque*. This book opens up layer 4.

The good news is that layer 4 has the same shape as layers 2 and 3.
It's just bytes laid out in a spec. The H.264 spec is ~800 pages, but
those pages describe field names, lengths, and decoding procedures —
nothing more exotic than what you'd find in any binary protocol
document. The skill of *reading those bits* is the only thing
separating you from being a codec implementer, and it's a skill, not
a talent.

## 0.2 Where in the segment is the bitstream?

For modern HLS (CMAF) and DASH, a video segment is an **fMP4** file
— a small subset of ISO base media file format (ISOBMFF). You've
probably seen these top-level boxes in MediaInfo or `MP4Box -info`
output:

```text
ftyp     File-type box. "This is fMP4 with these brand identifiers."
moov     Movie box (in the init segment).
         Container-wide metadata: codecs, tracks, sample tables,
         timing, color tags, etc.
moof     Movie fragment box (in each media segment).
         Per-fragment timing and offsets.
mdat     Media data box.
         The actual encoded media samples — codec payload lives here.
```

For an HLS deployment using CMAF, you'd have:

```text
init.mp4    ←  contains ftyp + moov.   Sent once at session start.
1.m4s       ←  contains moof + mdat.   One segment of video.
2.m4s       ←  contains moof + mdat.   Next segment. Etc.
```

The decoder needs `init.mp4` to know *which codec* is in use and how
to interpret the payload. Each `.m4s` then carries the next chunk of
encoded frames.

The `mdat` box is just a length-prefixed blob. Its contents are
**per-sample**, where a *sample* in ISOBMFF terminology is one
access unit — roughly one video frame or one audio frame.

The codec configuration lives in
`moov.trak.mdia.minf.stbl.stsd.<codec_box>`. For H.264 the codec box
is `avc1` and it contains an `AVCDecoderConfigurationRecord` — a small
blob holding the SPS and PPS (sequence and picture parameter sets,
explained in §0.4). For HEVC it's `hvc1`/`hev1`. For AV1 it's `av01`.

For **legacy MPEG-TS HLS** (the `.ts` segments), the structure is
different — 188-byte TS packets, PAT/PMT tables, PES packets carrying
access units — but the principle is the same: a transport layer
wrapping a payload, and the payload is the bitstream we care about.
Most new deployments use CMAF; legacy TS is in maintenance mode.

## 0.3 NAL units — the basic packet inside the bitstream

For H.264 and HEVC, the bitstream is divided into **NAL units**
(Network Abstraction Layer units). Each NAL unit is a typed packet:

```text
NAL unit structure:

  ┌───────────────────────────────────────────────────────┐
  │  start code (Annex B framing)  OR  length prefix      │
  │  ────                              ────────           │
  │  00 00 00 01                       4-byte length      │
  └───────────────────────────────────────────────────────┘
  ┌───────────────────────────────────────────────────────┐
  │  NAL header                                           │
  │  ──────────                                           │
  │  H.264: 1 byte    HEVC: 2 bytes                       │
  │  The header encodes the NAL unit's TYPE               │
  └───────────────────────────────────────────────────────┘
  ┌───────────────────────────────────────────────────────┐
  │  Payload                                              │
  │  ──────                                               │
  │  Variable length. Type-specific structure.            │
  │  This is where the actual codec data lives.           │
  └───────────────────────────────────────────────────────┘
```

For H.264, the most common NAL types you'll see:

| Type | Name             | What's in it                                                |
|------|------------------|-------------------------------------------------------------|
| 7    | SPS              | Sequence parameter set — picture size, profile, level       |
| 8    | PPS              | Picture parameter set — entropy mode, deblock parameters    |
| 5    | IDR slice        | Intra slice that resets decoder state — your keyframe       |
| 1    | Non-IDR slice    | A regular P-frame or B-frame slice                          |
| 6    | SEI              | Supplemental enhancement info — HDR metadata, captions etc. |
| 9    | Access unit delim| Marks frame boundaries (optional in some streams)           |

An access unit (one frame's worth of NAL units) typically looks like
this on the wire:

```text
Keyframe access unit:
  [AUD] [SEI] [SPS] [PPS] [IDR slice 0] [IDR slice 1] ...

Subsequent frames:
  [AUD] [non-IDR slice 0] [non-IDR slice 1] ...
```

(SPS/PPS only need to be sent once and whenever they change, but many
encoders send them at every IDR for robustness.)

For HEVC the NAL types are different numbers but the principle is
identical. For AV1 the analogous unit is the OBU (Open Bitstream
Unit) — same shape, different naming. For VP9 it's a "frame" with a
similar internal header structure.

## 0.4 Cracking open a real segment

Concrete walk-through. Take any HLS / DASH stream of H.264 content.
You'll need `ffmpeg`/`ffprobe` and ideally `MP4Box` (from GPAC)
installed.

### Step 1 — see the container

```sh
ffprobe -hide_banner init.mp4
# tells you codec=h264, profile, level, resolution, frame rate,
# pixel format (yuv420p), and the extradata blob
# (your AVCDecoderConfigurationRecord)

MP4Box -info segment1.m4s
# walks the box tree, showing moof, mdat positions, sample sizes
```

You'll see something like:

```text
Stream #0:0(und): Video: h264 (High), yuv420p(progressive),
                  1920x1080 [SAR 1:1 DAR 16:9],
                  5000 kb/s, 30 fps, 30 tbr, 30000 tbn
```

Read the fields out loud: codec H.264, High profile, 4:2:0 chroma
subsampling, 1920×1080, square pixel aspect ratio, ~5 Mbps, 30 fps.
All meaningful values you can now interpret in container terms.

### Step 2 — see the NAL units

The `trace_headers` bitstream filter is your most powerful learning
tool. It parses every field of every NAL header and prints them:

```sh
ffmpeg -hide_banner -i segment1.m4s -c:v copy \
  -bsf:v trace_headers -f null - 2>&1 | less
```

You'll see output like:

```text
[trace_headers] Sequence Parameter Set
[trace_headers] profile_idc                                100
[trace_headers] constraint_set0_flag                         0
[trace_headers] level_idc                                   40
[trace_headers] seq_parameter_set_id                         0
[trace_headers] chroma_format_idc                            1
...
[trace_headers] Slice Header
[trace_headers] first_mb_in_slice                            0
[trace_headers] slice_type                                   7
[trace_headers] pic_parameter_set_id                         0
[trace_headers] frame_num                                    0
...
```

Every field of every NAL unit, parsed. This is your first encounter
with the bitstream as *fields*, not bytes.

### Step 3 — read a NAL header by hand

For H.264 the byte after the start code (or length prefix) encodes:

```text
bit 7    : forbidden_zero_bit (always 0)
bits 6-5 : nal_ref_idc (priority/importance, 0-3)
bits 4-0 : nal_unit_type (the 5-bit type field, 0-31)
```

So:

| Byte (hex) | Binary    | nal_ref_idc | nal_unit_type | Meaning            |
|------------|-----------|-------------|---------------|--------------------|
| `0x67`     | 0110 0111 | 3           | 7             | SPS                |
| `0x68`     | 0110 1000 | 3           | 8             | PPS                |
| `0x65`     | 0110 0101 | 3           | 5             | IDR slice          |
| `0x41`     | 0100 0001 | 2           | 1             | Non-IDR slice (ref) |
| `0x01`     | 0000 0001 | 0           | 1             | Non-IDR slice (non-ref) |
| `0x06`     | 0000 0110 | 0           | 6             | SEI                |

If you can recognize SPS / PPS / IDR / non-IDR by their leading byte
in a hex dump, you have officially read your first bitstream.

Pull out the `mdat` payload (`MP4Box -raw 1 segment1.m4s` writes it
to a `.h264` file) and look at it in a hex viewer:

```text
00000000  00 00 00 02 09 10                                 |.....|
00000006  00 00 00 1d 67 64 00 28  ac d9 40 78 02 27 e5 84  |....gd.(..@x.'..|
00000016  00 00 03 00 04 00 00 03  00 ca 3c 60 c6 58        |..........<`.X|
                  ^^                            ↑
                  ↑                             also notice this:
              start code 00 00 00 01     "emulation prevention"
              followed by 0x67 (SPS)     escape byte 03
```

(The `03` bytes appearing inside payloads are **emulation prevention
bytes**, inserted by the encoder to prevent the byte pattern
`00 00 00`+ from ever appearing inside a NAL payload and being
mistaken for a start code. The decoder strips them out before parsing
the NAL contents. You learn this kind of detail only by reading the
bytes.)

## 0.5 What's inside a slice — the decoder's actual work

Everything up to §0.4 was container plumbing. The interesting part —
the part where actual compression unwinds — happens **inside** a
slice NAL unit.

A slice has, very roughly, this structure:

```text
slice_header
  - slice type (I, P, B)
  - frame number, picture order count
  - reference frame indices (which prior frame(s) to predict from)
  - QP (quantization parameter — how much was thrown away)
  - deblocking filter parameters
  - ~30 other small fields

slice_data
  - for each macroblock (16×16 pixel block, in H.264):
      - mb_type (intra mode? inter mode? which sub-partition?)
      - prediction parameters (intra direction or motion vector)
      - residual transform coefficients
      - all entropy-coded using CABAC or CAVLC
```

The decoder, on receiving a slice, has to do this for each macroblock:

```text
1. Decode mb_type + prediction parameters from the bitstream.
2. Form the prediction:
     intra: copy/extrapolate from already-decoded neighbouring pixels
     inter: fetch a block from a reference frame at the given motion vector
3. Decode the transform coefficients (entropy decoding).
4. Inverse quantize the coefficients (multiply by quantizer step).
5. Inverse transform (IDCT or similar) → residual pixels.
6. reconstructed pixels = prediction + residual.
7. Store in the reconstructed frame buffer.
```

After every macroblock in the slice is done:

```text
8. Apply the in-loop deblocking filter to smooth block boundaries.
```

After every slice of the access unit is done:

```text
9. Write the reconstructed frame into the DPB (decoded picture buffer).
10. Emit frames in display order based on PTS (B-frames cause decode
    and display order to diverge).
```

That sequence — *parse → predict → entropy-decode residual → inverse
quantize → inverse transform → add → deblock → store* — is the
**block-based hybrid decoder pipeline**. It's the same shape for
every modern codec. H.264, HEVC, AV1, VP9 all do exactly these steps,
in this order. They differ only in:

- **Block sizes.** 16×16 macroblocks in H.264; up to 128×128 super-
  blocks in AV1, recursively partitionable down to 4×4.
- **Prediction modes.** 9 intra directions in H.264; 56+ in AV1.
- **Transform sizes / types.** H.264 has integer 4×4 and 8×8 DCT-like
  transforms; AV1 has many sizes from 4×4 to 64×64 plus identity and
  asymmetric DST variants.
- **Entropy coder.** CAVLC or CABAC for H.264; range coder for VP9 and
  AV1.
- **In-loop filters.** Deblocking for H.264 / HEVC; deblocking + CDEF
  + loop restoration for AV1.

**Every chapter from here on unpacks one of these stages, in enough
detail that you can implement it.**

## 0.6 What the decoder hands you

After all that work, the decoder produces — for each frame — a YUV
buffer. For typical streaming content (H.264 at yuv420p):

```text
Y  plane:  width × height bytes              (luma — brightness)
Cb plane:  (width/2) × (height/2) bytes      (chroma blue-difference)
Cr plane:  (width/2) × (height/2) bytes      (chroma red-difference)
```

For 1920×1080: ~3 MB per frame. At 30 fps that's ~90 MB/s of decoded
video coming out of a ~5 Mbps stream — roughly **145× compression**.

If you set up FFmpeg correctly you can dump these raw planes:

```sh
ffmpeg -i segment1.m4s -c:v rawvideo -pix_fmt yuv420p out.yuv
```

That file is now your boundary between *codec* and *everything else*
(renderer, scaler, color management, display pipeline).

Open `out.yuv` in **YUView** (the canonical YUV viewer) to verify the
decode looks right. Or feed it back to FFmpeg to re-encode in a
different codec to compare. The raw YUV is the universal currency of
post-decode operations.

## 0.7 The map of what's left to learn

You're now at the right altitude to read the rest. The structure of
this book:

- **[Chapter 1 — Build a toy decoder](ch01-toy-decoder.md).** A
  ~200-line walkthrough of a complete intra-only decoder, in Rust.
  The skeleton you hang every subsequent chapter on.
- **[Chapter 2 — Bitstream I/O, exp-Golomb, and reading specs](ch02-bitstream-io.md).**
  The practical skill that gates every codec implementation. Not in
  any textbook. Possibly the most leveraged chapter for you.
- **[Chapter 3 — Color, pixels, and human vision](ch03-color-and-vision.md).**
  What "yuv420p" actually means. Saves you from the most common
  production bug.
- **[Chapter 4 — Transfer functions and HDR](ch04-transfer-and-hdr.md).**
  Why HDR is a different conversation, and what changes when you
  decode it.
- **Chapters 5–13** — one chapter per pipeline stage (intra,
  inter/motion, transforms, quantization, entropy coding, loop
  filters, DPB, rate control). Read in order on a first pass, or
  jump to the one matching your current task.
- **Chapters 14–17** — codec architectures, audio, containers,
  streaming protocols (overlap with your existing experience).
- **Chapters 18–21** — hardware acceleration, conformance, quality
  metrics, performance, production realities.
- **Part VI (optional appendix) — The theory.** Information theory
  and rate-distortion, demoted to optional reading. Read if you want
  the *why*; skip if you just want to ship a decoder. The math is
  here for completeness, not gatekeeping.

Throughout the book, every chapter opens with an **Engineering
takeaway** box — the 2–3-sentence summary of what an implementer
needs to know. If you read only the takeaway boxes you'll still know
~70% of what matters for implementation.

Sections marked `★` are optional theoretical asides. Skip them on a
first read; come back if curious.

## 0.8 Exercise — actually do this before reading on

Spend 30 minutes on the following. It will save you hours later.

1. Pick any HLS / DASH stream you've worked with. Download:
   - One init segment (e.g., `init.mp4`).
   - Two media segments — one near the start (likely contains an
     IDR), one mid-stream.

2. Run `MP4Box -info` and `ffprobe -show_streams -show_packets` on
   each. Read the output. You should now recognize:
   - The codec, profile, and level.
   - The resolution and pixel format.
   - The frame rate and bitrate.
   - The presence (or absence) of an IDR at the start of each segment.

3. Dump the `mdat` to a separate file. `MP4Box -raw 1 segment.m4s`
   writes the elementary stream to `segment_track1.h264`. Open in a
   hex viewer (`xxd`, `hexyl`, `010 Editor`, any will do). Find the
   start codes (`00 00 00 01`). For each, read the next byte and
   identify the NAL type using the table in §0.4.

4. Run the segment through `ffmpeg -bsf:v trace_headers`. Read the
   parsed fields for one slice header — *every field*. Don't worry if
   you don't understand them yet; the goal is to see that they all
   have names and values.

5. Decode to raw YUV:
   ```sh
   ffmpeg -i segment1.m4s -c:v rawvideo -pix_fmt yuv420p out.yuv
   ```
   Note the file size. Compute the ratio: YUV size ÷ segment size.
   That number — somewhere between 50× and 500× — is the compression
   ratio your codec achieved on this content.

By the end of this exercise you've handled every layer: manifest,
container, NAL unit, decoded YUV. The rest of the book is about what
happens between **NAL units** and **YUV** — which is exactly the gap
you came here to fill.

Onward to [Chapter 1 — Build a toy decoder](ch01-toy-decoder.md).
