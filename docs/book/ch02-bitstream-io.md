# Chapter 2 — Bitstream I/O, Exp-Golomb, and Reading Specs

> **Engineering takeaway:** Codec bitstreams are not byte-aligned. You
> need a "bit reader" that lets you ask for *N bits at a time*, where N
> can be 1, 17, or any value the spec dictates. On top of that primitive,
> codecs build a small vocabulary of variable-length codes — most
> commonly **exp-Golomb**. This chapter gives you both, plus the
> discipline for translating an ITU spec section into working Rust.
> It's the most leveraged chapter in the book.

The single largest skill gap between "someone who knows codecs in
theory" and "someone who can ship codec code" is the ability to read
specs. Every H.264 / HEVC / AV1 decoder is, at its heart, a careful
implementation of a spec document where each field is named, each
field has a length (which may be variable), and the implementer's job
is to read them in order without dropping bits.

The textbooks don't teach this because there's no theory to it.
Reading specs is mostly *patience plus a good bit reader*. This
chapter gives you the bit reader and shows you the patience.

## 2.1 Why bit-level I/O

A naïve "header struct" approach works for fixed-width formats like
the toy codec from Chapter 1, or like Wave audio, or like BMP. You
read 9 bytes, cast them to a struct, done.

Codec bitstreams aren't like that. Look at an H.264 SPS:

```text
seq_parameter_set_rbsp() {
    profile_idc                          u(8)     ← 8 bits
    constraint_set0_flag                 u(1)     ← 1 bit
    constraint_set1_flag                 u(1)     ← 1 bit
    constraint_set2_flag                 u(1)     ← 1 bit
    constraint_set3_flag                 u(1)     ← 1 bit
    constraint_set4_flag                 u(1)     ← 1 bit
    constraint_set5_flag                 u(1)     ← 1 bit
    reserved_zero_2bits                  u(2)     ← 2 bits
    level_idc                            u(8)     ← 8 bits
    seq_parameter_set_id                 ue(v)    ← variable!
    chroma_format_idc                    ue(v)    ← variable
    ...
```

In ten lines of spec you have fields of widths 8, 1, 1, 1, 1, 1, 1, 2,
8, variable, variable. The 8+8 fields are byte-aligned (the boundary
between byte 1 and byte 2 lands cleanly), but as soon as you hit `ue(v)`
— a variable-width code — alignment goes out the window. The next field
might start mid-byte.

You need a tool that doesn't care about byte boundaries. That's a
**bit reader**.

## 2.2 A bit reader, end to end

The mental model: a bit reader holds a byte slice plus a *bit-level
cursor*. You ask for N bits; it shifts the cursor forward by N and
returns the bits as an unsigned integer. Byte boundaries are
internal — invisible to the caller.

Minimal Rust:

```rust
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,  // bit position within data (0 = MSB of byte 0)
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    /// Read up to 32 bits, MSB-first, returning as u32.
    pub fn read_bits(&mut self, n: usize) -> Result<u32, ReadError> {
        assert!(n <= 32);
        if self.bit_pos + n > self.data.len() * 8 {
            return Err(ReadError::Eof);
        }

        let mut value: u32 = 0;
        for _ in 0..n {
            let byte = self.data[self.bit_pos / 8];
            let bit  = (byte >> (7 - (self.bit_pos % 8))) & 1;
            value = (value << 1) | (bit as u32);
            self.bit_pos += 1;
        }
        Ok(value)
    }

    /// Read a single bit as a bool.
    pub fn read_bit(&mut self) -> Result<bool, ReadError> {
        Ok(self.read_bits(1)? != 0)
    }

    /// How many bits have we consumed?
    pub fn bits_consumed(&self) -> usize {
        self.bit_pos
    }

    /// Align to the next byte boundary (used in some codecs).
    pub fn byte_align(&mut self) {
        self.bit_pos = (self.bit_pos + 7) & !7;
    }
}
```

This is correct but slow (one-bit-at-a-time). Production bit readers
process whole bytes or even whole 32/64-bit words at a time, then
shift out the bits the caller asks for. The principle is the same;
the optimizations are mechanical. See the workspace's
[`crates/oximedia-bitstream/`](../../crates/oximedia-bitstream/)
for the production version.

### The MSB-first convention

Note that the loop above reads **bit 7 first** (the most significant
bit of each byte). Every video codec spec uses this convention. The
JPEG spec, the H.264 spec, the HEVC spec, the AV1 spec — all MSB-first.

Audio codecs sometimes go the other way (FLAC's verbatim sub-blocks
are LSB-first). When you implement, **always check the spec** for
which direction; don't assume.

### Worked example with the reader

Say you have bytes `[0xA9, 0x47]` = binary `1010 1001 0100 0111`.

```rust
let mut r = BitReader::new(&[0xA9, 0x47]);
r.read_bits(3)?;  // 101  = 5
r.read_bits(5)?;  // 01001 = 9
r.read_bits(4)?;  // 0100 = 4
r.read_bits(4)?;  // 0111 = 7
```

That's how you parse "two 3-bit fields then two 4-bit fields" from a
16-bit blob — without ever invoking byte boundaries.

## 2.3 Exp-Golomb codes (the H.264 / HEVC star)

Most fields in an H.264 SPS are not fixed-width. The spec writes
`ue(v)` for **unsigned exp-Golomb of variable length**, and `se(v)`
for the signed variant. Here's the trick.

### Unsigned exp-Golomb (ue)

The codeword for unsigned integer N is:

```text
(N leading zero bits) followed by 1 followed by (N significant bits)
```

Where N = floor(log2(value + 1)). Decoded values for small inputs:

| Value | Codeword     | Bits |
|------:|--------------|-----:|
|   0   | `1`          |    1 |
|   1   | `010`        |    3 |
|   2   | `011`        |    3 |
|   3   | `00100`      |    5 |
|   4   | `00101`      |    5 |
|   5   | `00110`      |    5 |
|   6   | `00111`      |    5 |
|   7   | `0001000`    |    7 |
|   8   | `0001001`    |    7 |
|  …    | …            |   …  |

The codeword is its own length declaration: you count leading zeros,
read the implied `1`, then read that many more bits.

The decoder:

```rust
pub fn read_ue(r: &mut BitReader) -> Result<u32, ReadError> {
    // Count leading zeros.
    let mut leading_zeros = 0;
    while r.read_bit()? == false {
        leading_zeros += 1;
        if leading_zeros > 31 {
            return Err(ReadError::InvalidExpGolomb);
        }
    }
    // After leading zeros and the '1', read `leading_zeros` more bits.
    let suffix = if leading_zeros > 0 {
        r.read_bits(leading_zeros)?
    } else {
        0
    };
    Ok((1u32 << leading_zeros) - 1 + suffix)
}
```

Trace it on the bits `010` (codeword for 1):

1. `read_bit()` → false (leading zero count = 1)
2. `read_bit()` → true (end of prefix)
3. `read_bits(1)` → 0 (suffix)
4. Return `(1 << 1) - 1 + 0 = 1` ✓

Trace it on `00100` (codeword for 3):

1. read_bit → 0, read_bit → 0 (leading zeros = 2)
2. read_bit → 1 (end of prefix)
3. read_bits(2) → 00 = 0 (suffix)
4. Return `(1 << 2) - 1 + 0 = 3` ✓

### Signed exp-Golomb (se)

For signed values, H.264 encodes the unsigned value `k` and decodes
it as:

```text
signed = if k % 2 == 0 then  −k/2   else  (k+1)/2
```

So unsigned 0 → signed 0, unsigned 1 → signed 1, unsigned 2 → signed -1,
unsigned 3 → signed 2, unsigned 4 → signed -2, etc. Positive integers
get the odd unsigned codewords; negative integers get the even ones.

```rust
pub fn read_se(r: &mut BitReader) -> Result<i32, ReadError> {
    let k = read_ue(r)? as i32;
    Ok(if k % 2 == 0 { -(k / 2) } else { (k + 1) / 2 })
}
```

### Truncated exp-Golomb (te)

Used in a few places. Read it like ue, but with a maximum value the
encoder is allowed to use; if `max == 1`, just read one bit (its
inverse).

### Mapped exp-Golomb (me)

Used for coded block pattern in some H.264 contexts. Read it like ue,
then map through a lookup table specified in the standard.

### Why exp-Golomb?

Exp-Golomb has two virtues for codec headers:

1. **Self-delimiting.** No length field needed; the codeword carries
   its own length.
2. **Good for skewed distributions.** Most header fields are small
   most of the time (the `seq_parameter_set_id` is 0 in 99% of
   streams). Exp-Golomb gives small integers short codes.

It's not always optimal — for *very* skewed distributions you'd want
arithmetic coding — but it's free of state, and zero-allocation, and
that's plenty for sequence/picture/slice headers.

## 2.4 Emulation prevention — the gotcha

There's one more thing you need before parsing real H.264 / HEVC.

Recall from Chapter 0 that NAL units in Annex B framing are separated
by start codes `00 00 00 01`. If the payload of a NAL unit ever
contained the sequence `00 00 00`+, you'd have an ambiguity — is this
a start code or just a coincidence?

The encoder prevents this by inserting a "**emulation prevention
byte**" `03` whenever the pattern `00 00 0X` would appear in the
payload (for X ∈ {0, 1, 2, 3}). The decoder strips these out *before*
parsing the NAL payload.

```text
NAL payload as written by encoder (with emulation prevention):
  ... AB CD 00 00 03 00 EF 12 ...
                    ^^
                    inserted by encoder

After emulation prevention removal:
  ... AB CD 00 00 00 EF 12 ...
```

In code:

```rust
pub fn strip_emulation_prevention(nal_payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal_payload.len());
    let mut zeros = 0;
    for &b in nal_payload {
        if zeros >= 2 && b == 0x03 {
            zeros = 0;
            continue;          // skip the emulation byte
        }
        out.push(b);
        if b == 0x00 { zeros += 1; } else { zeros = 0; }
    }
    out
}
```

This is the source of one of the most common "decoder doesn't parse
the SPS right" bugs: someone forgot to strip emulation prevention.

ISOBMFF / fMP4 framing uses **length-prefixed NAL units instead of
start codes**, so emulation prevention bytes are still present in the
bitstream (the encoder doesn't know which framing the player will
use), but you still need to strip them on the decoder side. Don't
skip this step.

## 2.5 Putting it together — parsing an H.264 SPS

Here's roughly what parsing the first 20 fields of an SPS looks like
with the tools above:

```rust
pub fn parse_sps(rbsp: &[u8]) -> Result<Sps, ParseError> {
    let bytes = strip_emulation_prevention(rbsp);
    let mut r = BitReader::new(&bytes);

    let profile_idc          = r.read_bits(8)? as u8;
    let constraint_set0_flag = r.read_bit()?;
    let constraint_set1_flag = r.read_bit()?;
    let constraint_set2_flag = r.read_bit()?;
    let constraint_set3_flag = r.read_bit()?;
    let constraint_set4_flag = r.read_bit()?;
    let constraint_set5_flag = r.read_bit()?;
    let _reserved            = r.read_bits(2)?;
    let level_idc            = r.read_bits(8)? as u8;
    let seq_parameter_set_id = read_ue(&mut r)?;

    let chroma_format_idc = if profile_idc == 100 || profile_idc == 110
                            || profile_idc == 122 || profile_idc == 244
                            || profile_idc == 44  || profile_idc == 83
                            || profile_idc == 86 {
        read_ue(&mut r)?
    } else {
        1   // 4:2:0 default
    };

    // ... many more fields ...

    Ok(Sps {
        profile_idc, level_idc, seq_parameter_set_id,
        chroma_format_idc, /* ... */
    })
}
```

Notice the pattern: the spec is your **executable to-do list**. Every
field becomes one line of code. Conditional fields (`if profile_idc
∈ {...}`) become `if` statements. Loops (`for i = 0..N`) become Rust
`for` loops. There are no algorithmic decisions to make — *the spec
already made them*. You're transcribing.

That's the entire mental shift for spec-following: **stop thinking,
start transcribing**. Resist the urge to abstract or improve. Get the
straightforward implementation working, get it conformance-tested,
*then* optimize.

## 2.6 How to read an ITU spec

Pretty soon you'll be opening H.264 (ITU-T H.264 / ISO 14496-10) on
your own. Some conventions to internalize:

### Pseudo-code conventions

The spec uses C-like pseudo-code throughout:

```text
nal_unit_type = nal_unit_header( ) & 0x1F
if( nal_unit_type == 7 )
    seq_parameter_set_data( )
else if( nal_unit_type == 8 )
    pic_parameter_set_data( )
```

Translate directly to Rust. Don't get fancy. The spec's `for( i = 0;
i < N; i++ )` is your `for i in 0..N`. The spec's
`array[i]` is your `array[i]`. The spec's `function( a, b )` is your
`function(a, b)`.

### Field length notation

`u(n)` — unsigned, n bits.
`s(n)` — signed, n bits (two's complement).
`ue(v)` — unsigned exp-Golomb, variable length.
`se(v)` — signed exp-Golomb, variable length.
`b(8)` — byte.
`f(n)` — fixed pattern of n bits (must equal a specific value).
`u(v)` — unsigned, variable length (bit count depends on prior fields).

When you see `u(v)`, the bit count is *somewhere in the surrounding
spec text* — find it and code accordingly.

### Order matters

The fields appear *in the order you read them from the bitstream*.
You cannot skip ahead. You cannot rearrange. The pseudo-code is
imperative.

### "Reserved" / "shall be"

Fields marked "shall be 0" or "reserved" are still in the bitstream —
you must read them, even though you (in a strict decoder) should also
check that they have the expected value. Permissive decoders ignore
them; strict decoders error.

### Conformance: bit-exact, not "looks right"

The spec specifies *exactly* what bits the decoder must produce.
Every intermediate value is integer-precise. A correctly implemented
decoder produces output identical to every other correctly
implemented decoder, bit for bit. See Chapter 19 for how to test that.

## 2.7 Where the workspace's bit reader lives

This workspace has a shared bit-reader crate:
[`crates/oximedia-bitstream/`](../../crates/oximedia-bitstream/). It's
the production version of the toy in §2.2, with:

- 32-bit word reads under the hood (fast path).
- Bounds checking with proper error types.
- `read_ue`, `read_se`, `read_te` helpers.
- `peek_bits` (look at next N bits without consuming).
- Emulation-prevention-aware variants for NAL payloads.

Read `oximedia-bitstream/src/lib.rs` and you'll see the same shape as
the §2.2 minimal version, with the optimizations layered on top.

When you implement a new codec parser, **use this crate**. Don't
roll your own.

## 2.8 A guided spec exercise

Open the H.264 spec (ITU-T Recommendation H.264). Find Table 7-1
("NAL unit type codes") and Section 7.3.1 ("NAL unit syntax"). Read
them. You should be able to map each row to:

- Which NAL type number it is.
- What's inside (which higher-level structure follows).
- Whether it carries an RBSP or a complete data structure.

Then find Section 7.3.2.1 ("Sequence parameter set RBSP syntax"). Open
your text editor next to the spec. Translate the first 30 fields into
Rust. Use the bit reader above as your toolkit.

You'll discover:

- ~25 minutes of patient transcription.
- 3–5 bugs you make by misreading the spec.
- A working SPS parser by the end.

This exercise — translate a spec section into Rust — is what you'll
do dozens of times if you work on codec implementations. Building it
into reflex is what this chapter is for.

## 2.9 Where this lives in the workspace

The H.264 parser portion of this workspace lives in
[`crates/oximedia-codec/src/h264/parser/`](../../crates/oximedia-codec/src/h264/parser/)
*(when present; otherwise see codec_status.md for current state)*.
Read the SPS parser file alongside Section 7.3.2.1 of the spec; you
should see the spec line on the left and the Rust line on the right
match one-for-one.

This workspace's [ProRes parser](../../crates/oximedia-codec/src/prores/parser.rs)
follows the same discipline: parse the frame header, parse each
slice's metadata, then pass to the per-slice entropy decoder. SMPTE
RDD 36 is the spec; the parser is the transcription.

## 2.10 Exercises

1. **Implement read_ue from scratch** without referring to §2.3.
   Test on the table values. (This is a 15-minute exercise that locks
   in the prefix/suffix split.)

2. **Implement emulation prevention stripping** — both directions
   (encode side: insert; decode side: remove). Verify they're inverses
   on a random byte string.

3. **Translate the H.264 SPS** Section 7.3.2.1, fields 1 through 20,
   into Rust. (No grading; this is a self-test. If you can do this,
   you can implement any header parser.)

4. *In the codebase.* Open
   [`crates/oximedia-bitstream/src/lib.rs`](../../crates/oximedia-bitstream/).
   Find `read_bits`. Note the optimization (word-at-a-time reads). Map
   each line to the equivalent line in the §2.2 minimal reader. They
   should be the same algorithm, faster.

5. **Read a real SPS.** From a real H.264 stream, extract one SPS
   NAL unit (use `ffmpeg -bsf:v trace_headers` from Ch 0, then read
   the printed SPS values). Now parse the same NAL with your own
   parser. Outputs should match exactly.

6. *(Reading.)* Skim sections 7.3.2.1 and 7.3.2.2 of the H.264 spec.
   How many bit-level fields are there per SPS? Per PPS? (You'll get a
   feel for "small but tedious" being the right vibe for spec
   transcription.)

## 2.11 What's next

You now have the two foundational skills:

- A decoder skeleton ([Chapter 1](ch01-toy-decoder.md)) — you know
  what stages a decoder has.
- Bitstream I/O (this chapter) — you can read the bits each stage
  needs.

The next two chapters establish what the pixels you decode *mean*:

- **[Chapter 3 — Color](ch03-color-and-vision.md)**. Inputs and
  outputs of the decoder, and the four header fields that fix 90% of
  production color bugs.
- **[Chapter 4 — HDR](ch04-transfer-and-hdr.md)**. What changes with
  PQ and HLG.

After that, Part II opens up each stage of the pipeline you saw in
Chapter 1, one chapter at a time.
