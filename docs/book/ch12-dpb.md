# Chapter 12 — Reference Picture Management (DPB)

> **Engineering takeaway:** The Decoded Picture Buffer (DPB) is the
> decoder's set of frames that future frames might reference *or* that
> have been decoded but not yet displayed (because B-frames cause
> decode order to diverge from display order). The DPB is a small
> capacity (typically 4–16 frames) managed by the bitstream's
> reference marking commands. Two operations matter: **insert** a newly
> decoded frame, and **emit** the next frame to display (which may be
> several frames behind decode order). Mismanaging the DPB causes
> green/garbled frames, decoder crashes, or missing references — the
> nastiest bugs in a decoder.

The DPB is the decoder's "memory" of recently decoded frames. It
serves two simultaneous purposes:

1. **Source of references** for inter prediction (Chapter 7).
2. **Output reordering buffer** so B-frames (decoded out of display
   order) can be emitted in correct PTS order.

These two purposes have different lifetimes — a frame might be done
serving as a reference but not yet ready to be emitted (because a
later-PTS frame is sitting ahead of it in display order). The DPB
machinery tracks both.

## 12.1 The two roles of a DPB frame

Each frame in the DPB has two independent state flags:

- **Used as reference?** (Short-term, long-term, or unused.) Frames
  used as references must stay in the DPB until released. Released
  frames can be evicted to make room.
- **Output pending?** (Not yet emitted because its PTS hasn't come
  up.) Output-pending frames must stay until emitted.

A frame can transition through several states:

```text
decoded (just reconstructed)
  ↓
inserted into DPB, marked as short-term reference + output pending
  ↓
emitted (when its PTS comes up — but still serving as reference)
  ↓
marked as unused (when explicitly released by bitstream)
  ↓
evicted from DPB
```

A frame is only safe to free when *both* flags say it's no longer
needed.

## 12.2 Picture Order Count (POC) and reordering

To emit frames in display order, the decoder needs to know each
frame's display position. This is **POC** (Picture Order Count) in
H.264/HEVC.

POC is *not* the same as a sequential index. It's a virtual timestamp
in display order:

```text
Display order:    I0  B1  B2  P3  B4  B5  P6
Decode order:     I0  P3  B1  B2  P6  B4  B5
POC:              0   1   2   3   4   5   6
```

POC values increase monotonically with display position. The bitstream
encodes POC indirectly (via `pic_order_cnt_lsb` and PPS-level
configuration); the decoder computes the full POC from the bits.

To emit a frame:
- Find the frame in the DPB with the smallest POC that hasn't been
  emitted yet, **and** all earlier-POC frames have already been
  emitted.
- Emit it. Mark as "output complete."

This is essentially a min-heap operation, but with the constraint that
you only emit when the gap from the just-emitted frame is small (no
gaps in display order).

## 12.3 H.264 reference picture marking

H.264's DPB management is signaled per-slice via the **reference
picture marking** syntax element. There are two flavors:

### Sliding window (default)

The decoder maintains a FIFO of short-term references. When a new
frame is decoded, the oldest reference falls off. Simple, used by
most streaming encoders.

### Adaptive (explicit commands)

The slice header can contain explicit commands:
- `MMCO 1`: mark short-term picture N as no longer used.
- `MMCO 2`: mark long-term picture N as no longer used.
- `MMCO 3`: convert short-term picture N to long-term.
- `MMCO 4`: change the maximum long-term picture count.
- `MMCO 5`: clear the entire DPB (typically at IDR boundaries).
- `MMCO 6`: mark current picture as long-term.

The decoder reads these commands and updates the DPB state.

## 12.4 HEVC reference picture sets (RPS)

HEVC simplifies and generalizes H.264's mechanism. Instead of issuing
explicit MMCO commands, each slice carries a complete **Reference
Picture Set** — the full list of pictures that are *currently
references*:

- Short-term references after this slice.
- Long-term references after this slice.

The decoder updates the DPB to match: pictures in the RPS stay,
pictures not in the RPS are released. The next slice / picture starts
with a freshly-specified RPS.

This is more verbose in the bitstream (an RPS is encoded in each
slice) but eliminates a class of error — the encoder can't accidentally
release a picture it later wants to reference.

## 12.5 AV1 reference frame slots

AV1 takes yet another approach: **8 reference frame slots**. A
sequence-level concept says "this codec has 8 slots indexed 0–7." Each
frame:

- Picks up to 7 reference frames from these slots.
- Optionally writes its reconstructed result into a slot (overwriting
  what was there).

The encoder's job is to manage which frames live in which slots.
Modern AV1 encoders use sophisticated reference selection (which
reference picture for which block, picking from the 7 available).

The decoder side: read the slot indices for each block's references,
look up the picture, decode. After the frame's reconstructed, possibly
write it to a slot per the frame header's instructions.

## 12.6 DPB size and conformance

The DPB has a *fixed maximum capacity* specified by the profile and
level. For example:

- H.264 Baseline: typically 4 frames at most.
- H.264 Main: typically 8 frames at most (some levels go to 16).
- HEVC Main: up to 6 frames at 1080p, more at lower resolutions.
- AV1: fixed 8 slots in the spec.

A conformant bitstream cannot require more frames in the DPB than the
level allows. Decoders allocate based on the level field.

## 12.7 Inserting a newly decoded frame

When a frame is fully reconstructed and post-filtered, the decoder:

1. **Find an empty slot** in the DPB. If none, evict the
   most-stale-and-unused frame.
2. **Insert** the frame.
3. **Mark** it according to the slice header's reference marking
   commands.
4. **Update POC** for the new frame.
5. **Maybe emit** other frames from the DPB whose POC has caught up.

The "maybe emit" step needs care. After inserting frame N, you can
emit frames with POC < N's POC if (a) they're not still needed as
references, (b) they haven't been emitted yet.

## 12.8 The full DPB algorithm

```rust
fn process_frame(dpb: &mut DPB, frame: Frame, refs: ReferenceMarking) {
    // 1. Make room.
    while dpb.is_full() {
        if let Some(slot) = dpb.find_evictable() {
            dpb.remove(slot);
        } else {
            panic!("DPB full and no evictable frames - bitstream is broken");
        }
    }

    // 2. Insert the new frame.
    dpb.insert(frame, refs);

    // 3. Update reference marking.
    refs.apply_to(dpb);

    // 4. Emit any frames that are now safe to emit.
    while let Some(next) = dpb.find_emittable_in_order() {
        emit_to_display(next);
        next.output_done = true;
    }
}

fn DPB::find_evictable(&self) -> Option<usize> {
    // A frame is evictable if it's not a reference AND has been
    // emitted (or is unused at all).
    for (i, frame) in self.frames.iter().enumerate() {
        if !frame.is_reference && frame.output_done {
            return Some(i);
        }
    }
    None
}
```

The actual implementation has more edge cases (IDR refresh, gaps in
frame numbers, error resilience), but this captures the shape.

## 12.9 PTS / DTS and the decoder timing model

PTS / DTS interact with the DPB:

- **DTS** (Decode Timestamp): when the decoder consumes this access
  unit.
- **PTS** (Presentation Timestamp): when this frame should be
  displayed.

For a stream with no B-frames, PTS = DTS. With B-frames:

```text
Display:  I0(0)  B1(1)  B2(2)  P3(3)
Decode:   I0     P3      B1     B2
DTS:      0      1      2      3
PTS:      0      3      1      2
```

The decoder uses DTS for input pacing (when can I read the next
chunk?) and PTS for output pacing (when should I show this frame?).

The container framework typically carries both; the decoder uses PTS
in `find_emittable_in_order` to determine display order.

## 12.10 The HRD — bitstream-level buffer constraints

A separate but related concept: the **Hypothetical Reference Decoder
(HRD)** specifies an abstract buffer model. The bitstream must satisfy
constraints on:

- **CPB** (Coded Picture Buffer): bits arriving at a specified rate
  must not overflow or underflow a buffer of specified size.
- **DPB** (decoded picture buffer): never exceed the level's capacity.

These constraints exist so that a real-time decoder (e.g., a
hardware decoder in a TV) can operate within fixed memory and
bitrate. The encoder's rate controller (Chapter 13) keeps within
the HRD.

For a decoder implementer, the HRD is informational — the constraints
mean *any* compliant decoder can decode the bitstream in real time at
the specified rate. You don't actively check HRD compliance in the
decoder; you trust the encoder did.

## 12.11 Error resilience

What happens if a frame is corrupted or missing?

- **For intra-only codecs**, just emit a placeholder for the missing
  frame; the rest of the stream is independent.
- **For codecs with references**, a missing frame breaks all
  subsequent inter-predicted frames that reference it. Frames depending
  on the missing reference will produce visibly wrong output until
  the next IDR.

The decoder's options:
- Drop the corrupted frame and any frames that reference it.
- Substitute a previous good frame as the reference (and accept
  drift).
- Resync at the next IDR.

Production decoders implement some combination of these. The encoder's
role in error resilience is to insert IDRs at regular intervals
(GOP size) so the longest possible drift period is bounded.

## 12.12 Where this lives in the workspace

ProRes has **no DPB** — intra-only, no references. Each frame is
emitted immediately.

For inter-coded codecs (when added):

```text
crates/oximedia-codec/src/h264/dpb.rs       ← ~400 lines
crates/oximedia-codec/src/hevc/rps.rs       ← ~500 lines
crates/oximedia-codec/src/av1/ref_frames.rs ← ~300 lines (slot-based)
```

The DPB is one of the more bug-prone parts of a decoder. Treat
correctness as critical and test extensively with conformance suites.

## 12.13 Further reading

- **[Wiegand2003]** §III.A — H.264 reference picture management.
- **[Sullivan2012]** §IX — HEVC RPS structure.
- **[Chen2020]** §2.6 — AV1 reference frame management.
- **JM reference** (H.264) — the canonical reference DPB
  implementation, slow but correct.

## 12.14 Exercises

1. **Decode order to display order.** For a stream with display
   order `I B B P B B P` and a GOP-of-7 structure, list the decode
   order. List the POC values of each frame in decode order.

2. **DPB size.** A stream uses 4 short-term references and 2 long-
   term references. What's the minimum DPB capacity needed?
   What's the minimum level that supports it?

3. **MMCO trace.** A slice header has `MMCO 1: ref_pic_num=5` and
   `MMCO 3: convert pic 7 to long-term, idx=0`. What does the
   decoder do to the DPB?

4. *(Reading.)* In FFmpeg's `libavcodec/h264_refs.c`, find the
   reference list construction function. Don't try to read every
   line; observe the structure (decode marking commands, update
   short-term list, update long-term list).

5. **Error scenario.** A frame in the middle of a GOP arrives with a
   missing reference. What should the decoder do? Sketch the
   options and their tradeoffs.

6. **AV1 slots.** Frame N writes to slot 3. Frame N+1 references slot
   3. Frame N+2 *also* writes to slot 3, then later frames reference
   slot 3. Which frame's reconstruction does the later reference
   get? (Hint: slots are mutable; you read the current contents.)

---

Next: [Chapter 13 — Rate Control (Encoder-Side Reference)](ch13-rate-control.md).
