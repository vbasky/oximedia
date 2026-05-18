# Chapter 18 — Hardware Acceleration

> **Engineering takeaway:** Most consumer devices have a hardware
> video decoder block that's 10–100× more power-efficient than
> software decoding. The OS exposes it through a platform-specific
> API (VideoToolbox on Apple, VAAPI on Linux, NVDEC on NVIDIA, Direct
> X on Windows). The hardware works at the bitstream level — you hand
> it encoded bytes plus codec configuration and it produces decoded
> frames in GPU memory. Your code becomes a coordinator: parse
> containers, hand bitstreams to hardware, receive YUV frames. For
> production media apps, hardware decode is mandatory; for codec
> *research* and *correctness* work, software decode is what you need.

A modern phone, tablet, laptop, or TV has dedicated hardware for
decoding popular video codecs (H.264, HEVC, sometimes AV1). The
hardware is *vastly* more efficient than CPU decode — a typical phone
SoC decodes 4K HEVC at <1W; the same on the CPU would consume 5–10W
and drain the battery in minutes.

For shipping a media app, hardware decode is non-negotiable. For
*understanding* codecs, software decode is what teaches you the
bitstream.

This chapter covers what hardware decoders do, the major platform
APIs, and when to use hardware vs software.

## 18.1 What the hardware does

A hardware video decoder is a specialized ASIC implementing the codec
bitstream-to-pixels pipeline. The API surface is similar everywhere:

1. Application provides **codec configuration** (SPS/PPS for H.264;
   sequence header for AV1).
2. Application provides **encoded bytes** for each access unit
   (frame).
3. Hardware decodes and writes the result to **GPU memory** —
   typically an NV12 or P010 surface.
4. Application receives a handle to the GPU surface (or copies it
   back to CPU memory if needed).

The hardware is **stateful** — it maintains its own DPB, applies loop
filters, manages reference frames. The application only sees the
output.

### What hardware can and can't do

- **Can**: decode standard codec profiles up to specified resolutions
  and frame rates. Apply post-decode operations (scaling, chroma
  conversion, deinterlace).
- **Can't**: decode non-standard profiles, modified bitstreams, or
  codecs the hardware doesn't support. Recover gracefully from
  corrupted bitstreams (often returns an error or freezes).
- **Limited**: handle multiple concurrent streams (depends on
  hardware capacity); decode at frame rates above the hardware's
  rated performance.

## 18.2 Apple: VideoToolbox

The macOS / iOS / iPadOS / tvOS API.

### Decoder model

1. Create a `VTDecompressionSession` with codec specification.
2. Call `VTDecompressionSessionDecodeFrame` with each compressed
   sample.
3. Callback fires with the decoded `CVImageBuffer` (a GPU surface).
4. Render the buffer via Metal, Core Video, or AVFoundation.

```swift
let session = VTDecompressionSessionCreate(...)
VTDecompressionSessionDecodeFrame(
    session,
    sample_buffer,        // CMSampleBuffer with encoded data
    frame_flags,
    nil,                  // pixel buffer (or destination)
    callback              // called when decode finishes
)
```

### Codec support

- **H.264**: Baseline, Main, High; up to 4K at 60fps depending on
  device.
- **HEVC**: Main, Main 10; up to 8K at 60fps on M2+ chips.
- **AV1**: hardware decode on A17 Pro / M3 and later; software
  fallback otherwise.
- **ProRes** and **ProRes RAW**: hardware decode on M1+ (great for
  pro workflows).

### Common patterns

- For HLS / DASH playback: AVFoundation handles the whole pipeline.
  You never touch VideoToolbox directly.
- For custom playback: use AVSampleBufferDisplayLayer + manual
  VideoToolbox decode.
- For frame extraction or transcoding: VideoToolbox directly + Core
  Image / Metal for post-processing.

### Where the workspace meets this

This workspace's [`wire_formats.md`](../wire_formats.md) covers
VideoToolbox specifics:
- `CMSampleBuffer` structure.
- `CVImageBuffer` / `CVPixelBuffer` interop.
- NV12 vs other pixel formats.
- The decoder callback model.

## 18.3 Linux: VAAPI and V4L2

### VAAPI (Video Acceleration API)

Intel's API, now broadly supported on AMD and (limited) NVIDIA on
Linux. Used by FFmpeg (`-hwaccel vaapi`), VLC, mpv.

```c
VADisplay display = vaGetDisplayDRM(fd);
VAContextID context = ...;
vaBeginPicture(display, context, surface);
vaRenderPicture(display, context, buffer_id, num_buffers);
vaEndPicture(display, context);
```

The application provides codec-specific *buffer types*:
- Picture parameter buffer (decoded SPS/PPS).
- Slice parameter buffer (one per slice).
- Slice data buffer (encoded bytes).

The hardware processes them, writes to a VA-API surface, application
maps the surface to read pixels or render directly via OpenGL / Vulkan.

### V4L2 (Linux kernel video API)

The lower-level kernel interface. Embedded systems and Raspberry Pi
use this directly. More direct, less abstraction.

### Where it lives

- Linux desktops (mpv, VLC).
- Servers doing video processing at scale (FFmpeg pipelines).
- Embedded video appliances (security cameras, IPTV boxes).

## 18.4 NVIDIA: NVDEC and NVENC

NVIDIA's hardware. Different from the rest because NVIDIA cards have
dedicated decode (NVDEC) and encode (NVENC) hardware separate from
the CUDA cores.

### Decoder API

CUDA-based; usually accessed through NVIDIA's Video Codec SDK or
indirectly through FFmpeg (`-hwaccel cuda` or `-hwaccel nvdec`).

```c
CUVIDDECODECREATEINFO info = { /* codec, target_format, etc. */ };
cuvidCreateDecoder(&handle, &info);
cuvidDecodePicture(handle, &pic_params);
cuvidMapVideoFrame(handle, picture_index, &dev_ptr, &pitch);
```

The hardware writes to CUDA device memory. You can then run CUDA
kernels on the output (scaling, color conversion, ML inference) or
copy back to host.

### Codec support (as of 2025)

- H.264: comprehensive.
- HEVC: comprehensive.
- AV1: Ada/Hopper architecture (RTX 40-series) and newer.
- VP9: comprehensive.

### Where it lives

- Server-side transcoding (cloud video pipelines).
- Streaming overlay tools (OBS).
- ML video processing pipelines.

## 18.5 Windows: DirectX Video Acceleration

Microsoft's API. Now D3D11 Video API (older D3D9 still supported).

```
Direct3D 11 Video Device
  → Video Processor for color/format conversion
  → Video Decoder for bitstream → YUV
```

Game engines, media foundation, browsers all sit on this. For Windows
codec work, this is the API.

### Where it lives

- Microsoft Edge (Chromium with Windows-specific paths).
- Windows Media Foundation.
- Game streaming overlays.
- Most native Windows media apps.

## 18.6 Bitstream-level vs slice-level vs frame-level

Hardware decoders differ in how much "intelligence" is in the
hardware vs the driver:

- **Bitstream-level**: hardware does everything from bytes to pixels.
  Application hands it raw bytes. Simplest API; most modern hardware.
- **Slice-level**: hardware decodes individual slices; driver / app
  does NAL parsing and slice extraction. Older designs.
- **Frame-level**: hardware does per-block work but driver does
  CABAC. Rare modern; some embedded GPUs.

The trend has been toward bitstream-level decoders — the application
gets a simpler API and the hardware handles more. VAAPI on Intel
chips is slice-level; VideoToolbox is frame-level (you provide
SPS/PPS separately); NVDEC is bitstream-level.

## 18.7 When to use hardware vs software

**Hardware**: ship in production. Battery, scale, multi-stream,
4K/HDR/8K.

**Software**:
- Development and testing — you can step through it.
- Codec engineering — you need bit-exact control.
- Codecs the hardware doesn't support (new profiles, niche codecs).
- Server-side transcoding where flexibility matters more than energy.
- Cross-platform code where one binary needs to run everywhere.

A typical app uses both:
- Hardware for playback when supported.
- Software fallback for edge cases.

## 18.8 Common pitfalls

**Format mismatches.** Hardware decoders output specific formats
(NV12, P010 most commonly). If your renderer expects YUV420p planar,
you need conversion — fast on GPU, but a step you have to handle.

**Out-of-order output.** Some hardware decoders produce frames in
decode order rather than display order. You may need to maintain your
own PTS-based reorder buffer.

**Capability detection.** "Does this device support 10-bit HEVC at
4K 60fps?" — answered by querying API capabilities at runtime. Always
do this; don't assume.

**Error recovery.** Corrupted bitstreams crash some hardware
decoders. Production code should re-init on error rather than die.

**Concurrent stream limits.** Most consumer hardware can decode 1–4
streams simultaneously. Exceeding the limit silently switches to
software fallback (slow) or returns error.

## 18.9 Where this lives in the workspace

This workspace is software-focused for codec engineering, but
real-world integration would interact with hardware decoders. The
workspace's [`wire_formats.md`](../wire_formats.md) covers the Apple
VideoToolbox integration patterns. Future modules might include:

```text
crates/oximedia-hw/src/
  videotoolbox.rs   ← Apple integration
  vaapi.rs          ← Linux integration
  nvdec.rs          ← NVIDIA integration
  dx11.rs           ← Windows integration
```

Each is a thin adapter that converts oximedia's frame stream model to
the platform API.

## 18.10 Further reading

- **Apple Developer Documentation** — VideoToolbox programming guide.
- **Intel VAAPI Programming Guide** — `01.org` archives.
- **NVIDIA Video Codec SDK Documentation** — included with the SDK.
- **Microsoft DirectX Video Acceleration** documentation.
- **FFmpeg's hwaccel documentation** — how to use each platform's
  hardware decoder from FFmpeg. Practical reference.

For Linux-side hardware decode, the LWN.net articles on V4L2 stateful
encoder/decoder support are the best long-form coverage.

## 18.11 Exercises

1. **Identify the API.** You're building an iOS media player. Which
   API do you use for video decode? An Android player? A Linux desktop
   player? A Windows kiosk?

2. **Capability check.** Sketch the runtime check for "does this
   device support HEVC 10-bit 4K decode?" for VideoToolbox.

3. **Format conversion.** Your decoder output is NV12; your renderer
   expects YUV420p planar (separate U and V planes). What's the
   conversion? GPU or CPU?

4. **Software fallback.** When should a media app fall back to
   software decode? Sketch the decision tree.

5. *(Reading.)* Look at FFmpeg's `libavcodec/videotoolbox.c` (or
   `vaapi_h264.c`, or `cuviddec.c`). Find the entry point — note
   that it's a thin wrapper that hands bitstream data to platform API.

6. **Multi-stream.** A wall of 16 surveillance cameras at 1080p
   30fps each = 16× simultaneous decode. Will consumer hardware
   handle this? When would server-class hardware (NVIDIA Quadro,
   data center) be needed?

---

End of Part IV. Next: [Chapter 19 — Bit-Exact Conformance](ch19-conformance.md).
