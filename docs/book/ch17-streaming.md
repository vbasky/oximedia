# Chapter 17 — Streaming Protocols

> **Engineering takeaway:** This chapter is short because you already
> know this material from your day job. The codec engineer's
> additional view is what streaming protocols require *from the
> codec*: segment alignment with IDRs, codec configuration in
> manifests, codec/container compatibility constraints. We'll cover
> the codec-side touchpoints of HLS, DASH, CMAF, LL-HLS, and WebRTC —
> not the protocols themselves.

You've shipped HLS and DASH. This chapter is *not* a tutorial on
manifests or ABR; it's a look at how codec choices interact with
streaming protocol choices. If you want the streaming protocol details
themselves, RFC 8216 (HLS) and ISO/IEC 23009-1 (DASH) are the
authoritative documents.

## 17.1 What streaming wants from a codec

Streaming protocols impose several requirements on the codec/container
combination:

1. **Frequent random access.** ABR clients switch between renditions;
   that means each segment must start with an **IDR** (or equivalent
   keyframe). Encoders force keyframes at GOP boundaries aligned to
   segment durations.

2. **Aligned timestamps across renditions.** A 1080p and a 720p
   rendition must have matching PTS for each segment so the client
   can switch seamlessly.

3. **Self-contained segments.** Each segment must be decodable
   independently of others (given the init segment). No reference to
   prior segments' frames except through the codec's normal DPB
   management — and the DPB is implicitly reset at IDR.

4. **Predictable bitrate.** Within a rendition, bitrate stability
   helps client buffer estimation.

These shape the encoder's rate control and GOP structure.

## 17.2 HLS — HTTP Live Streaming

The Apple-originated protocol. Now standardized as RFC 8216 (HLS) and
RFC 8217 (low-latency HLS).

### Codec-side considerations

- **Containers**: legacy uses `.ts` (MPEG-TS) segments; modern uses
  CMAF (`.m4s`) with the `EXT-X-MAP` directive pointing at the init
  segment.
- **Codecs**: H.264 + AAC is the default everyone-supports baseline.
  HEVC is supported by Apple devices. AV1 support is spreading.
- **Codec advertisement**: `EXT-X-STREAM-INF` includes a `CODECS`
  attribute with the RFC 6381 codec string.
- **Segment duration**: typically 4–6 seconds for VOD, 2 seconds for
  live, 1 second or less for LL-HLS.
- **IDR alignment**: every segment's first sample must be an IDR or
  equivalent.

### Low-Latency HLS (LL-HLS)

The 2020 addition. Key changes:

- **Smaller segments** (~1 second) divided into **partial segments**
  (~100–300 ms) for sub-segment delivery.
- **Preload hints** in the manifest, so clients can prefetch.
- **Render queue management** to keep client latency to ~3–5 seconds
  (vs ~30+ for traditional HLS).

For the codec: more frequent IDRs (every 1 second instead of every 4),
which means slightly worse compression but better random access.

## 17.3 DASH — Dynamic Adaptive Streaming over HTTP

The MPEG-standardized equivalent of HLS. Widely deployed in
non-Apple ecosystems.

### Codec-side considerations

- **Containers**: CMAF (`.m4s`) is standard. Plain MP4 still works.
- **Codecs**: same set as HLS; mappings via MPD's `codecs` attribute.
- **Manifest format**: XML (the `MPD`).
- **Segment templating**: time-based or number-based URL templates;
  client constructs URLs from the template + media segment offsets.

### Low-Latency DASH (LL-DASH)

Uses **chunked encoding** of CMAF segments — server streams partial
segments as they encode. Reduces glass-to-glass latency to ~3 seconds.

The decoder side: partial segments need partial parsing — handle
incomplete `mdat` and `moof` buffers. This is implementation work,
but most fMP4 parsers support it natively.

## 17.4 CMAF — Common Media Application Format

CMAF is a *container* (Chapter 16) but plays a key role in streaming
*protocols*. It's the format that allows one segment to be served via
both HLS and DASH:

- HLS playlist references CMAF segments.
- DASH MPD references the same CMAF segments.
- One CDN delivery; both protocols work.

This was the big move of the late 2010s: media-format unification.
You can have one segment set serving every streaming protocol.

### Constraints

CMAF segments must:
- Use specific brand identifiers (`cmf2`, `cmfc`, etc.).
- Have IDR at segment start.
- Use predictable timestamp scaling.
- Use specific encryption schemes if encrypted (`cbcs` or `cenc`).

For the codec: encode for CMAF and you encode for HLS, DASH, and
anyone else.

## 17.5 WebRTC — real-time

For real-time (conferencing, broadcasting), WebRTC bypasses the
HLS/DASH model entirely and uses **RTP** (Real-time Transport
Protocol) over UDP.

### Codec considerations

- **Video codecs**: VP8, VP9, H.264, AV1. WebRTC mandates VP8 and
  H.264 as minimum support; modern endpoints add VP9 and AV1.
- **Audio**: Opus is mandatory; G.711 (PCM µ-law/A-law) is fallback.
- **No fixed GOP structure**: keyframes are sent on demand (when a
  receiver requests via PLI/FIR). Encoders typically maintain
  reference-frame structures suited for packet loss.
- **SVC** (Scalable Video Coding) is common. Different temporal
  layers (or spatial layers, in some codecs) at different priorities.
  The simulcast / SVC pattern lets a server selectively forward
  different layers to different receivers.

### What the codec must support

- Quick keyframe injection (encoder emits IDR on request).
- Low latency operation (no B-frames typically — they add delay).
- Reference frame management for packet-loss resilience.

These are encoder-side concerns. The decoder side is mostly the same
codec stack, just with a different transport.

## 17.6 Codec compatibility matrix

A rough cheat sheet for what works where:

| Codec  | HLS legacy | HLS CMAF | DASH | WebRTC | Smart TVs | Browsers |
|--------|-----------|----------|------|--------|-----------|----------|
| H.264  | yes       | yes      | yes  | yes    | yes       | yes      |
| HEVC   | partial   | yes      | yes  | rare   | yes       | partial  |
| VP9    | rare      | n/a      | yes  | yes    | rare      | yes (most) |
| AV1    | n/a       | yes      | yes  | yes (modern) | yes (modern) | yes (most modern) |
| AAC    | yes       | yes      | yes  | n/a    | yes       | yes      |
| Opus   | rare      | yes      | yes  | yes    | rare      | yes      |
| AC-3   | yes       | yes      | yes  | n/a    | yes       | partial  |

The codec engineer's contribution to a streaming team is: "We can ship
codec X to client population Y; that's the decoder cost; that's the
encoder cost." This matrix is the starting point.

## 17.7 ABR ladders and codec laddering

A typical 2026 ABR ladder serves multiple codecs and multiple
bitrates:

```text
H.264 ladder:    240p 400kbps → 360p 800k → 480p 1.5M → 720p 3M → 1080p 5M
HEVC ladder:                            → 480p 700k → 720p 1.5M → 1080p 3M → 4K HDR 10M
AV1 ladder:                             → 720p 1M  → 1080p 2M → 4K 6M
```

A modern player selects:
- First, the best codec the client supports (AV1 > HEVC > H.264).
- Then, the best bitrate within that codec given network conditions.

Each rendition is a separate encode; storage cost is the sum.
Netflix's per-title encoding optimizes each rendition individually
based on content complexity, not a fixed quality ladder.

## 17.8 Where this lives in the workspace

Streaming protocols are typically handled at a higher layer than
codec decoders. This workspace decodes the bitstream; the streaming
client is a consumer.

If you're building a player that includes oximedia decoders, your
streaming stack might look like:

```
HLS / DASH manifest parser
   ↓
ABR logic (rendition selection)
   ↓
HTTP fetch for init + media segments
   ↓
Container demuxer (CMAF / TS)
   ↓
Codec decoder (oximedia-codec)
   ↓
Render
```

The codec decoder is at the bottom; everything above it is "streaming
infrastructure."

## 17.9 Further reading

- **[RFC8216]** — HLS specification. Short and readable.
- **[DASHSpec]** — ISO/IEC 23009-1, DASH specification.
- **DASH-IF CMAF Ingest** specification — for live workflows.
- **WebRTC RFC family** — RFC 8825, 8826, 8827 for the protocol;
  RFC 7742 for video codec requirements.
- **Apple Tech Note QA1962** — Apple's authoritative HLS guidance.
- **Twitch Engineering blog** — for real-world LL-HLS deployment
  notes.

## 17.10 Exercises

1. **GOP alignment.** An HLS deployment uses 4-second segments. The
   encoder is configured with a 10-second GOP. What's the bug? What
   should happen instead?

2. **Codec ladder.** Design a 4-rendition HLS ladder for a service
   that wants H.264 universal compatibility and HEVC for premium
   devices. Specify resolutions, bitrates, codec strings.

3. **CMAF vs TS.** What's the bandwidth overhead of MPEG-TS vs CMAF
   for the same encoded content? (Hint: think about the 188-byte
   packet alignment.)

4. **LL-HLS partial segments.** A live stream uses 1-second segments
   each divided into 250 ms partial segments. The encoder still
   needs to insert keyframes — does it insert at every partial
   boundary or only at the start of full segments? Why?

5. *(Reading.)* Look at your HLS manifest tool of choice. Find the
   `EXT-X-VERSION` and `EXT-X-CODECS` directives. What versions does
   your stream require?

6. **Glass-to-glass latency.** For a live broadcast, list the sources
   of latency from camera to viewer. (Encode → segment → upload → CDN
   → fetch → decode → render.) Which can you reduce?

---

Next: [Chapter 18 — Hardware Acceleration](ch18-hardware.md).
