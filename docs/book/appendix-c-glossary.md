# Appendix C — Glossary

Terms used throughout the book, alphabetical. Each entry includes a
brief definition and (in parentheses) the chapter(s) where it's
introduced or used most.

---

**Access unit.** All the NAL units that make up one frame's worth of
coded data. Roughly synonymous with "encoded frame." (Ch 0, 5)

**ABR (Adaptive Bitrate).** Streaming strategy where the client
switches between multiple bitrate renditions based on network
conditions. The encoder produces a ladder of renditions; the
manifest describes them; the player picks. (Ch 17)

**ABR (Average Bit Rate).** Encoder rate control mode that targets an
average bitrate over the file, with looser short-term constraints than
CBR. (Ch 13)

**Access unit delimiter (AUD).** A NAL unit type (H.264 type 9) that
marks the start of a new access unit. Optional in some streams; useful
for finding frame boundaries. (Ch 0)

**ACES.** Academy Color Encoding System. The industry color
pipeline for preserving creative intent end-to-end through production.
The codec rarely speaks ACES directly, but the final output reaches
the codec after going through ACES-shaped transforms. (Ch 3, 4)

**ACR (Absolute Category Rating).** Subjective testing method where
viewers rate encoded video on a discrete scale (e.g., 1–5) without
seeing the original. Standardized in ITU-R BT.500. (Ch 20)

**ADTS (Audio Data Transport Stream).** AAC streaming framing format
with small per-packet headers. (Ch 15)

**Anchor frame / IDR (Instantaneous Decoder Refresh).** A keyframe
that resets the decoder's state — no reference to any prior frame.
Required at every random access point (segment boundary in HLS/DASH).
(Ch 7, 11, 17)

**Annex B framing.** The framing where NAL units are separated by
start codes (`00 00 00 01`). Used in live streams and `.h264` /
`.h265` files; in fMP4 NAL units are length-prefixed instead. (Ch 0)

**AOM (Alliance for Open Media).** The industry consortium behind AV1.
(Ch 14)

**AQ (Adaptive Quantization).** An encoder-side technique that
adjusts QP per block based on perceptual factors (brightness,
variance, edges). Decoder reads the per-block QP delta and is
indifferent to how it was chosen. (Ch 9, 13)

**Arithmetic coding.** An entropy coding technique that represents a
sequence of symbols as a single number in a sub-interval of [0, 1).
Achieves the Shannon entropy bound. CABAC is its H.264/HEVC variant.
(Ch 10, 23)

**AV1.** Modern royalty-free video codec from AOM, released 2018.
Roughly 50% better than H.264 at the same quality. (Ch 14)

**AVC.** Advanced Video Coding. The MPEG name for H.264. (Ch 14)

**Bjøntegaard delta rate (BD-rate).** The standard codec comparison
metric: average percentage bitrate savings of one codec over another
at the same quality. (Ch 20, 23)

**Bin (CABAC).** A single binary symbol decoded by CABAC's arithmetic
coder. Multi-valued fields are binarized into sequences of bins. (Ch
10, Appendix A)

**Bit reader.** A small abstraction over a byte buffer that lets you
read N bits at a time, regardless of byte boundaries. The fundamental
primitive of bitstream parsing. (Ch 2)

**Block-based hybrid pipeline.** The fundamental architecture of
every modern video codec: divide image into blocks, apply prediction,
transform, quantization, entropy coding per block. (Ch 5, 24)

**B-frame (bidirectionally predicted).** A frame that can use both
past and future frames as references. Better compression than
P-frames, but cause decode order to diverge from display order. (Ch
7, 12)

**Boolean coder.** VP9's name for its arithmetic coder; a simpler
ancestor of AV1's range coder. (Ch 14)

**bS (boundary strength).** H.264 deblocking filter parameter
determining how strongly to filter a block boundary. Computed per
edge based on neighbour block context. (Ch 11)

**BT.601 / BT.709 / BT.2020.** ITU recommendations specifying color
primaries, transfer functions, and matrix coefficients for SD, HD, and
UHD video respectively. (Ch 3)

**BT.2100.** ITU recommendation defining HDR formats: HDR10 (PQ) and
HLG. (Ch 4)

**CABAC (Context-based Adaptive Binary Arithmetic Coding).** H.264 /
HEVC's adaptive arithmetic coder. Uses per-context probability
estimates that update as bins are decoded. (Ch 10, Appendix A)

**CAVLC (Context-Adaptive Variable Length Coding).** H.264's
alternative to CABAC. Variable-length codes with context-adaptive
codebook selection. Simpler than CABAC; ~5–10% worse compression. (Ch
10)

**CBR (Constant Bit Rate).** Rate control mode targeting a fixed
bitrate over a buffer window. Quality fluctuates. (Ch 13)

**CDEF (Constrained Directional Enhancement Filter).** AV1's
in-loop filter targeting ringing artifacts. (Ch 11)

**Chroma.** The color (non-brightness) components of a YCbCr pixel.
Cb and Cr. (Ch 3)

**Chroma subsampling (4:2:0 / 4:2:2 / 4:4:4).** Discarding color
resolution while keeping luma resolution. 4:2:0 is universal in
streaming. (Ch 3)

**CMAF (Common Media Application Format).** A strict fMP4 profile
designed for streaming. Lets one segment set serve both HLS and DASH.
(Ch 16)

**Codec configuration record.** A small data structure (e.g.,
AVCConfigurationRecord for H.264) that holds the decoder-init
information — SPS, PPS, etc. — in containers. (Ch 0, 16)

**Coefficient.** An entry in the transform-domain matrix. Most are
quantized to zero; surviving non-zero ones carry the signal. (Ch 8, 9)

**Conformance.** The property of producing bit-identical output to
every other compliant decoder, given the same bitstream. Required by
codec standards. (Ch 19)

**Context (CABAC).** A probability model used by CABAC for decoding
one bin. H.264 has ~460 contexts; the decoder selects which based on
the bin's role and neighbour state. (Ch 10, Appendix A)

**CPB (Coded Picture Buffer).** The HRD model's input buffer. The
bitstream must satisfy buffer constraints (no underflow, no overflow).
(Ch 12, 13)

**CRF (Constant Rate Factor).** Rate control mode targeting constant
perceived quality with frame-type-specific QP offsets. (Ch 13)

**CTU (Coding Tree Unit).** HEVC's largest block unit. CTUs are
recursively partitioned via quad-trees. (Ch 14)

**DCT (Discrete Cosine Transform).** The frequency-domain transform
used in most codecs. Energy-compacting for natural image content.
(Ch 8)

**Deblocking filter.** In-loop filter that smooths block boundaries
to reduce blocking artifacts. (Ch 11)

**Dequantization.** Multiplying received quantized coefficients by
their step size to approximately reconstruct the original
coefficients. (Ch 9)

**Display P3.** Color space variant using DCI-P3 primaries with D65
white point. Used by Apple displays. (Ch 3)

**Distortion (D).** The error between original and reconstructed
content. Measured as MSE, PSNR, SSIM, VMAF, etc. (Ch 20, 23)

**DPB (Decoded Picture Buffer).** The decoder's buffer of recently
decoded frames, used as references for future frames and for display
reordering. (Ch 12)

**Drift.** Cumulative error between encoder's and decoder's
reconstructions. Caused by non-bit-exact operations. Visible after
many frames as progressive image degradation. (Ch 19)

**DTS (Decode Timestamp).** When a frame should be decoded.
(Ch 7, 12)

**ECM.** Enhanced Compression Model — emerging post-VVC research
codec. (Ch 14)

**Emulation prevention byte.** A `0x03` byte inserted into NAL
payloads to prevent the start-code pattern (`00 00 00 01`) from
appearing inside payloads. The decoder strips them out. (Ch 2)

**Entropy (Shannon).** The minimum bits per symbol required to
represent a source with given probabilities. The fundamental limit of
lossless compression. (Ch 23)

**Entropy coding.** Converting symbols to bits at near-entropy rate.
The final compression stage. Examples: Huffman, CAVLC, CABAC, range
coder. (Ch 10)

**Exp-Golomb (ue / se).** Variable-length code used in H.264 / HEVC
headers. Self-delimiting: leading zeros indicate codeword length. (Ch
2)

**FFV1.** A lossless video codec. Compresses to ~50% of raw size at
best. (Ch 14)

**fMP4 (fragmented MP4).** MP4 variant with `moof` boxes per fragment
instead of a single file-scope `moov`. Used in streaming. (Ch 16)

**FourCC.** Four-character codes identifying box types in ISOBMFF
(`ftyp`, `moov`, `mdat`, etc.) and codec types (`avc1`, `hvc1`,
`av01`). (Ch 16)

**GOP (Group of Pictures).** A run of frames between IDR keyframes.
Bounds the maximum prediction reference depth. (Ch 17)

**H.264 / AVC.** ITU/ISO video codec released 2003. Most common
streaming codec in 2026. (Ch 14)

**H.265 / HEVC.** ITU/ISO video codec released 2013. ~50% better than
H.264. (Ch 14)

**H.266 / VVC.** ITU/ISO video codec released 2020. Latest in the
MPEG family. (Ch 14)

**HDR (High Dynamic Range).** Video format supporting much wider
luminance range than SDR. Achieved through BT.2100 transfer functions
(PQ or HLG). (Ch 4)

**HDR10.** Static HDR format using PQ + BT.2020 primaries. Per-stream
metadata: MaxCLL, MaxFALL, MDCV. (Ch 4)

**HEVC.** See H.265.

**HLG (Hybrid Log-Gamma).** HDR transfer function used in
broadcast. Display-adaptive (relative). (Ch 4)

**HLS (HTTP Live Streaming).** Apple's streaming protocol. Uses M3U8
manifests pointing at media segments. (Ch 17)

**HRD (Hypothetical Reference Decoder).** Abstract decoder model the
bitstream must satisfy. Constrains the rate controller's QP choices.
(Ch 12, 13)

**Huffman coding.** Variable-length entropy coding using prefix-free
codewords. Loses 5–20% efficiency to arithmetic coding on skewed
distributions. (Ch 10)

**Inter prediction.** Predicting a block from a previously decoded
frame. (Ch 7)

**Intra prediction.** Predicting a block from already-decoded
neighbours within the same frame. (Ch 6)

**ISOBMFF.** ISO Base Media File Format. Parent of MP4, MOV, fMP4,
CMAF. (Ch 16)

**Lagrangian (J = D + λR).** The expression every encoder minimizes
per coding decision. λ is the price of a bit in distortion units. (Ch
23, 25)

**λ (lambda).** Lagrangian multiplier. Higher λ → save bits; lower λ
→ spend freely. Function of QP. (Ch 23, 25)

**Loop filter.** In-loop filters (deblocking, SAO, CDEF, LR) applied
to reconstructed frames before they enter the DPB. (Ch 11)

**LR (Loop Restoration).** AV1's in-loop filter for detail recovery,
typically a Wiener filter. (Ch 11)

**Luma.** The brightness component of a YCbCr pixel. Y. (Ch 3)

**MaxCLL / MaxFALL.** HDR static metadata signaling the brightest
pixel (MaxCLL) and brightest frame average (MaxFALL) in a stream.
(Ch 4)

**MDCT (Modified Discrete Cosine Transform).** The audio-codec
transform. Overlapping windows prevent boundary clicks. (Ch 15)

**MDCV (Mastering Display Color Volume).** HDR metadata describing
the mastering display's primaries and luminance range. (Ch 4)

**Merge mode.** Inter prediction mode where the block uses a
neighbour's MV with no delta. HEVC and AV1 feature. ~5% BD-rate gain
over H.264. (Ch 7)

**Motion compensation (MC).** The decoder's job: receive (ref_idx,
MV) and fetch the predicted block from the reference. (Ch 7)

**Motion estimation (ME).** The encoder's job: find the best-matching
MV. Expensive; dominates encode time. (Ch 7)

**MPEG-TS.** MPEG-2 Transport Stream. 188-byte packet format used in
broadcast and legacy HLS. (Ch 16)

**MS-SSIM (Multi-Scale SSIM).** Perceptual quality metric computing
SSIM at multiple scales. (Ch 20)

**MTS (Multiple Transform Selection).** VVC feature allowing different
transform types per block. (Ch 14)

**MV (Motion Vector).** The (x, y) displacement from a block in the
current frame to its predicting block in the reference frame. (Ch 7)

**MVD (Motion Vector Delta).** The transmitted difference between the
actual MV and the MV predictor (MVP). (Ch 7)

**MVP (Motion Vector Predictor).** A prediction of the current
block's MV from neighbour MVs. Median (H.264) or list-based (HEVC).
(Ch 7)

**MXF (Material Exchange Format).** Broadcast professional container
specified in SMPTE ST 377. (Ch 16)

**NAL unit.** Network Abstraction Layer unit. The basic packet of
H.264 / HEVC bitstreams. Each carries a typed payload. (Ch 0)

**OBU (Open Bitstream Unit).** AV1's analog to NAL unit. (Ch 0)

**Opus.** Open audio codec supporting both speech and music. WebRTC
mandatory. (Ch 15)

**P-frame (predicted).** A frame predicted from one reference frame.
(Ch 7)

**Plane.** One color component of an image (Y, Cb, or Cr) stored as a
2D array. (Ch 3)

**POC (Picture Order Count).** Display-order index for H.264 / HEVC.
Distinct from decode order. (Ch 12)

**PPS (Picture Parameter Set).** H.264 / HEVC header carrying picture-
level configuration (entropy mode, deblock filter parameters). (Ch 0,
5)

**PQ (Perceptual Quantizer, BT.2100/ST 2084).** HDR transfer function
absolute in cd/m². (Ch 4)

**Profile.** A specific subset of codec features. H.264 has Baseline,
Main, High. HEVC has Main, Main 10, etc. (Ch 14)

**PSNR (Peak Signal-to-Noise Ratio).** Standard objective quality
metric. dB scale; higher = better. Limited as a perceptual proxy.
(Ch 20)

**PTS (Presentation Timestamp).** When a frame should be displayed.
(Ch 7, 12)

**QP (Quantization Parameter).** The user-facing quality dial. Higher
QP = more compression = more loss. (Ch 9)

**Range coder.** AV1 / VP9's arithmetic coder variant. Mathematically
equivalent to CABAC's arithmetic but with different bookkeeping. (Ch
10)

**Rate (R).** Bits used to encode a sample or block. (Ch 23)

**Rate control.** Encoder logic that picks QP per frame / per block
to hit a target bitrate. (Ch 13, 25)

**RDO (Rate-Distortion Optimization).** The encoder's inner-loop
optimization, evaluating candidate choices by J = D + λR. (Ch 23, 25)

**Reference picture.** A frame in the DPB used as a prediction source
for current decoding. (Ch 7, 12)

**Renormalization (CABAC).** Periodic shift of the arithmetic coder's
state to keep within bit precision. Reads new bits from the bitstream.
(Ch 10, Appendix A)

**Residual.** The prediction error: original − prediction. The actual
content the encoder/decoder transforms and quantizes. (Ch 5)

**RPS (Reference Picture Set).** HEVC's per-slice list of which
pictures are currently references. (Ch 12)

**RTP (Real-time Transport Protocol).** UDP-based protocol for
real-time video / audio delivery. Used by WebRTC. (Ch 17)

**SAO (Sample Adaptive Offset).** HEVC's in-loop filter that adjusts
pixel values based on classification. (Ch 11)

**Scan order.** The pattern in which 2D coefficients are written to a
1D sequence. Zigzag is the canonical pattern. (Ch 8)

**SEI (Supplemental Enhancement Information).** H.264 / HEVC NAL unit
type carrying optional metadata (HDR signaling, closed captions). (Ch
0)

**Slice.** A unit of a frame that can be decoded independently of
other slices in the same frame. Used for error resilience and
parallelism. (Ch 0, 5)

**SPS (Sequence Parameter Set).** H.264 / HEVC header carrying
sequence-level configuration (resolution, profile, level, bit depth).
(Ch 0, 5)

**SSIM (Structural Similarity Index).** Perceptual quality metric
considering local luminance, contrast, structure. (Ch 20)

**Start code.** The byte pattern `00 00 00 01` marking the start of a
NAL unit in Annex B framing. (Ch 0)

**Sub-pel motion.** Motion at fractional-pixel precision (¼-pel,
⅛-pel). Achieved with interpolation filters. (Ch 7)

**Superblock.** AV1's largest block unit (up to 128×128). (Ch 14)

**Tile.** A spatial division within a frame that's independently
decodable. Used for parallel decode in HEVC, AV1. (Ch 21)

**TF (Transfer Function).** The curve mapping stored code values to
real-world luminance. SDR: power-law. HDR: PQ or HLG. (Ch 4)

**Transform.** Frequency-domain basis change applied to spatial
residuals. DCT and variants. (Ch 8)

**Two-pass.** Rate control mode that encodes once for statistics, then
again for the actual output. Better quality per bit than single-pass.
(Ch 13, 25)

**VBR (Variable Bit Rate).** Rate control mode targeting average
bitrate over the file. (Ch 13)

**VBV (Video Buffer Verifier).** Buffer model used by H.264 / HEVC
encoder for HRD compliance. (Ch 12, 13)

**VLC (Variable-Length Code).** Huffman-style codes with variable
bitstring lengths. CAVLC's underlying mechanism. (Ch 10)

**VMAF (Video Multi-method Assessment Fusion).** Netflix's learned
quality metric. Correlates well with subjective scores. (Ch 20)

**VP9.** Google's video codec, predecessor to AV1. (Ch 14)

**VVC.** See H.266.

**WebM.** Restricted Matroska profile for web delivery. Supports
VP8/VP9/AV1 + Vorbis/Opus. (Ch 16)

**WebRTC.** Real-time communication protocol stack. Uses RTP +
specific signaling. (Ch 17)

**Y / Cb / Cr.** Luma + two chroma differences. The color space most
codecs work in. (Ch 3)

**YCbCr.** Color space derived from RGB by linear matrix. The "YUV"
of video. (Ch 3)

**Zigzag scan.** The standard pattern for converting 2D DCT
coefficients to 1D scan order: top-left → bottom-right, sweeping
diagonally. (Ch 8)

---

For deeper coverage of any term, the chapter references are above.
For unfamiliar terms not in this glossary, check the
[Bibliography](bibliography.md) for codec-specific references.
