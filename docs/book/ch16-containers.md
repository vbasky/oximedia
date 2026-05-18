# Chapter 16 — Container Formats

> **Engineering takeaway:** A container is the file wrapping the codec
> bitstream. It carries timing (PTS/DTS), codec configuration (the
> bits the decoder needs *before* it can decode samples), track
> metadata (codec, resolution, language, captions), and the encoded
> samples themselves. The codec doesn't know what container it lives
> in; the container doesn't know what codec it's carrying. This
> separation is the most useful abstraction in media engineering.

You touched on containers in Chapter 0 (fMP4 / CMAF segments). This
chapter goes deeper: the four containers you'll meet most often, what
they're good at, and what the decoder takes from each.

## 16.1 What a container does

A container's job is to wrap codec-encoded payload with everything
needed to play it back:

- **Sample framing.** Where does each frame start and end in the
  byte stream?
- **Timing.** PTS and DTS for each sample.
- **Codec configuration.** SPS / PPS for H.264; sequence header for
  AV1. The decoder needs this before it can decode samples.
- **Track metadata.** What codec, what resolution, what language,
  what color tags.
- **Index / random access.** Where is the keyframe nearest time T?
- **Encryption metadata.** If the content is DRM-protected.
- **Captions / subtitles.** Often a separate track with its own
  codec (WebVTT, TTML, CEA-708).

Different containers prioritize different goals — broadcast favors
robustness against packet loss; streaming favors random access and
codec flexibility; production favors metadata richness.

## 16.2 ISOBMFF — the modern workhorse

**ISO Base Media File Format** (ISO/IEC 14496-12) is the parent
specification for MP4, MOV, fMP4, and CMAF. Every modern streaming
deployment uses it. Master this one.

### The box tree

ISOBMFF organizes a file as a tree of *boxes* (also called *atoms*).
Each box has a 4-byte size, a 4-byte type (FourCC), and a payload.
Box trees can nest.

```text
ftyp     file type box ("this is mp42 or iso5 or whatever")
moov     movie box (container-wide metadata)
  mvhd     movie header (duration, timescale, etc.)
  trak     track box (one per audio/video/subtitle track)
    tkhd     track header (track ID, duration, dimensions)
    mdia     media box
      mdhd     media header
      hdlr     handler (track type: vide, soun, subt)
      minf     media information
        stbl     sample table — the index
          stsd     sample descriptions (codec info)
          stts     decode timestamps
          ctts     composition timestamp offsets (for B-frames)
          stsc     sample-to-chunk mapping
          stsz     sample sizes
          stco     chunk offsets in the file
mdat     media data (the actual encoded samples)
```

That's a typical layout. Reading the box tree teaches you everything
the decoder needs.

### fMP4 / CMAF (the modern streaming variant)

Plain MP4 has `mdat` and `moov` at file scope — to access any sample,
you read the whole `moov` first. For live streaming, you can't
pre-compute `moov`. So fMP4 splits the file into **fragments**:

```text
ftyp     ...
moov     (in init segment only)
moof     movie fragment box (per segment)
  mfhd     fragment header
  traf     track fragment
    tfhd     track fragment header
    trun     track run (sample timing/sizes for this fragment)
mdat     this fragment's media data
```

Each segment is a self-contained `moof + mdat` pair. The decoder reads
`moof` to know where samples are, then reads them from `mdat`.

**CMAF** (Common Media Application Format) is a stricter profile of
fMP4 designed for streaming: specific brand identifiers, specific
constraints on track structure, designed to be byte-compatible with
both HLS and DASH delivery. As of 2026, almost all new streaming uses
CMAF.

### Codec configuration in ISOBMFF

The `moov.trak.mdia.minf.stbl.stsd` box holds a per-codec entry that
includes the codec-specific configuration:

- For H.264: `avc1` entry → `AVCConfigurationRecord` → SPS + PPS.
- For HEVC: `hvc1` or `hev1` entry → `HEVCConfigurationRecord`.
- For AV1: `av01` entry → `AV1CodecConfigurationRecord`.

The decoder reads this *first* (from the init segment), uses the
SPS/PPS to set up its internal state, then decodes media segments
without seeing those headers again (or with them re-sent
per-IDR-segment, redundantly).

This is the **separation of init from media** that makes CMAF work
for streaming: send the init segment once, send media segments
repeatedly.

### Where it lives

- **MP4** — generic file extension; can be plain or fragmented.
- **fMP4** — fragmented MP4; same internal structure with `moof`s.
- **MOV** — Apple's slightly older variant, similar but with QuickTime-
  specific brands. Usually plain (non-fragmented).
- **CMAF** — strict fMP4 for streaming.
- **.m4s** — segment file extension for fMP4 / CMAF segments.

## 16.3 Matroska / WebM

**Matroska** (MKV) is an open alternative to ISOBMFF. **WebM** is a
restricted profile of Matroska intended for web delivery.

### Structure

Matroska uses EBML (Extensible Binary Markup Language) — a binary
TLV format with named elements at every level. Conceptually:

```text
EBML       (file format identification)
Segment    (top-level container)
  SeekHead   (offsets to important elements)
  Info       (file duration, title)
  Tracks     (track definitions, codec info)
  Cluster    (encoded sample data, organized as Blocks)
    Timestamp  (cluster's base timestamp)
    SimpleBlock or BlockGroup (one sample's data)
  Cues       (random-access index)
  Tags       (metadata)
```

### vs ISOBMFF

- **Pros**: Open, flexible, supports almost any codec. Strong
  community tooling (`mkvtoolnix`).
- **Cons**: Less standardized for streaming. Browser support is
  limited to WebM (which restricts to VP8/VP9/AV1 + Vorbis/Opus).
  Not used much for HLS / DASH; CMAF wins there.

### Where it lives

- Open-source desktop video tools (VLC, MPV).
- Anime fan-sub community traditional.
- YouTube uses WebM for AV1 streaming alongside MP4.
- Linux desktop streaming.

For the workspace's purposes, Matroska is something you may need to
read for testing files but rarely produce.

## 16.4 MPEG-TS — the broadcast workhorse

**MPEG Transport Stream** (MPEG-2 Part 1) is the broadcast and legacy
HLS container. Designed in 1995 for error-tolerant delivery over
unreliable channels (satellite, cable, ATSC over-the-air).

### Structure: 188-byte packets

Everything is in 188-byte packets:

```text
TS packet (188 bytes):
  sync byte (0x47)         1 byte
  flags                    1 byte
  PID (packet ID)          13 bits
  scrambling control       2 bits
  adaptation control       2 bits
  continuity counter       4 bits
  optional adaptation field
  payload
```

The PID identifies which stream the packet belongs to. PIDs are
assigned at the broadcaster's discretion; the **Program Map Table
(PMT)** lists which PIDs are video, audio, etc.

A complete TS file is a sequence of these packets, often hundreds of
thousands long.

### PES packets and timing

Sample data is wrapped in **Packetized Elementary Stream (PES)**
packets, which then span multiple TS packets. PES carries:

- PTS / DTS (encoded as 33-bit values, 90 kHz scale).
- PES header indicating sample boundaries.

### Where it lives

- Legacy HLS (`.ts` segments).
- DVB (digital broadcast in Europe).
- ATSC (digital broadcast in North America).
- ARIB (Japan).
- Some IPTV deployments.

For modern HLS, CMAF (`.m4s`) is preferred. TS is dying but not dead.

### Pain points

- Constant 188-byte packets waste space when sample sizes don't align.
- PCR (Program Clock Reference) timing is sometimes lost in
  encapsulation, causing playback issues.
- No native HDR metadata support (extensions exist but are vendor-
  specific).

## 16.5 MXF — broadcast professional

**Material Exchange Format** (SMPTE ST 377) is the broadcast and
production container. You'll meet it in:

- Broadcast servers and playout systems.
- ProRes and DNxHR file delivery.
- IMF (Interoperable Master Format) — feature film mastering.

Conceptually similar to ISOBMFF (a typed-box file format) but with
broadcast-specific metadata (timecode, captions, color tags
extensively used) and stricter operational constraints (must support
random access at any frame, must support partial reads of large
files).

MXF is the production end of the pipeline. For streaming engineering,
you'll occasionally consume MXF (transcoding a master to streaming
formats); you won't produce it.

[Hartmann2010] is the canonical book.

## 16.6 What the decoder takes from the container

For each access unit (one frame), the decoder needs:

- **The bytes.** A pointer to the encoded payload.
- **The size.** How many bytes.
- **The PTS** (and DTS, if B-frames).
- **The keyframe flag.** Is this an IDR? Decoder may need to flush
  state.

The container hands these over as a stream of "samples." How they're
laid out in the file is the container's problem; the decoder doesn't
care.

### The init/media separation

For streaming containers (fMP4, CMAF):

- **Init segment**: codec config (SPS/PPS), codec brand, color tags.
  Read once at session start.
- **Media segment**: samples with timing. Read repeatedly.

The decoder is initialized from the init segment, then processes
media segments in a loop:

```rust
fn decode_session(init: &[u8], segments: impl Iterator<Item = &[u8]>) {
    let mut decoder = setup_from_init(init);
    for segment in segments {
        for (sample, pts, dts) in parse_segment(segment) {
            decoder.decode(sample, pts, dts);
        }
    }
}
```

## 16.7 Codec strings in manifests

In HLS and DASH manifests, codecs are identified by **codec strings**
that summarize the codec, profile, and level:

```text
H.264 High Profile, Level 4.0:    avc1.640028
H.264 Main Profile, Level 3.1:    avc1.4D401F
HEVC Main 10:                     hvc1.2.4.L150.b0
AV1 Main Profile, Level 4.0:     av01.0.04M.08
```

The decoder needs to recognize these and (a) verify it can decode
them, (b) configure itself accordingly. Mismatches between codec
string and actual stream content are a real-world cause of "decoder
doesn't work" bugs.

These strings are defined in:

- **RFC 6381** — the format.
- **ISO/IEC 14496-15** — for H.264 / HEVC mappings.
- **AOM AV1 spec** — for AV1.

## 16.8 Where this lives in the workspace

The workspace's container support is currently limited. ProRes
typically ships in MOV; H.264 / HEVC / AV1 ride in fMP4 / CMAF.

If/when a CMAF parser is added:

```text
crates/oximedia-container/src/
  isobmff/
    box_parser.rs    ← walk the box tree
    moov.rs          ← extract codec config from moov
    moof.rs          ← extract sample info from each fragment
  ts/
    packet.rs        ← 188-byte packet parsing
    pes.rs           ← PES extraction
```

The container parser is small but bug-prone. The boxes can be deeply
nested, and edge cases (corrupt boxes, unexpected types) need careful
handling. Use a battle-tested library where you can; the IETF's
`isoboxes` and GPAC's MP4Box are good references.

## 16.9 Further reading

- **ISO/IEC 14496-12** — ISOBMFF specification. Free to download.
- **ISO/IEC 14496-15** — H.264 / HEVC in ISOBMFF.
- **ISO/IEC 23001-7** — Common Encryption (CENC) for DRM in ISOBMFF.
- **DASH-IF CMAF spec** — strict CMAF profile for streaming.
- **RFC 6381** — codec strings.
- **Hartmann2010** — *MXF: The Material Exchange Format* (book).
- **Matroska wiki** — for Matroska / WebM specifics.

## 16.10 Exercises

1. **Walk a box tree.** Pick any `.mp4` file you have. Run `MP4Box
   -info` and `MP4Box -diso` (which dumps the box tree to XML).
   Find: the codec, the resolution, the color tags, the duration.

2. **Init vs media.** Take an HLS CMAF stream. Identify the init
   segment URL (`#EXT-X-MAP` in the playlist). Download both init
   and the first media segment. Run `MP4Box -info` on each. What
   boxes does init have that media doesn't? Why?

3. **Codec string.** A manifest advertises `avc1.4D401F`. What
   does that mean? (Hint: split into "codec", "profile", "level".)
   What if the actual stream is High Profile (100, not 77)?

4. *(Reading.)* Look at the FFmpeg source for the MP4 demuxer:
   `libavformat/mov.c`. Find the `mov_read_default` or equivalent
   entry point. Note the box-type dispatch table.

5. **Container choice.** You're shipping a new streaming service.
   Why would you choose CMAF over plain MP4? Over Matroska? Over
   MPEG-TS?

6. **MPEG-TS PID.** A TS file has video on PID 0x100 and audio on
   PID 0x101. To extract just the video stream from a 1 GB TS file,
   what do you do? (Hint: filter packets by PID.)

---

Next: [Chapter 17 — Streaming Protocols](ch17-streaming.md).
