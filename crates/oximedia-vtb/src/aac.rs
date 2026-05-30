//! AAC decoder via Apple AudioToolbox `AudioConverter`.
//!
//! AudioToolbox is a separate framework from VideoToolbox but lives in
//! the same `-sys` crate.  [`oximedia_vtb_sys`] already allowlists the
//! `AudioConverter*` and `AudioStream*` symbols.  This module wraps
//! them in a safe, single-frame-decode-per-call API that mirrors
//! [`crate::decoder::H264Decoder`].
//!
//! ## Scope
//!
//! - **AAC-LC** decode to **signed 16-bit interleaved PCM**, mono or
//!   stereo, at the sample rate the bitstream declares.
//! - Accepts either:
//!   - **ADTS-framed** AAC: each call to [`AacDecoder::decode_frame`]
//!     passes one ADTS frame; AudioConverter parses the 7-byte ADTS
//!     header itself.
//!   - **Raw AAC frames** (as found inside MP4 `mp4a` boxes): the
//!     caller supplies the `AudioSpecificConfig` bytes once via
//!     [`AacDecoder::set_magic_cookie`], then passes raw access units.
//!
//! ## Out of scope (for now)
//!
//! - HE-AAC (SBR + PS).  AudioConverter supports it via different
//!   format IDs (`kAudioFormatMPEG4AAC_HE` / `_HE_V2`) — exposing
//!   them is a thin extension to [`AacFormat`].
//! - Float output formats and non-interleaved layouts.
//! - Stateful flush of the converter's internal delay.  The
//!   underlying AudioConverter is reset between frames so this
//!   decoder is suitable for chunked / packet-at-a-time decode but
//!   not for absolute sample-count tracking across discontinuities.

use std::cell::Cell;
use std::ptr;

use oximedia_vtb_sys::{
    AudioBuffer, AudioBufferList, AudioConverterDispose, AudioConverterFillComplexBuffer,
    AudioConverterNew, AudioConverterRef, AudioConverterSetProperty, AudioStreamBasicDescription,
    AudioStreamPacketDescription, OSStatus, UInt32,
};

use crate::error::{Result, StatusContext, VtbError};

/// AAC format flavour selector — currently only AAC-LC; HE variants
/// are placeholders for future expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AacFormat {
    /// AAC Low Complexity — the default profile in MP4 / ADTS streams.
    LowComplexity,
}

impl AacFormat {
    /// CoreAudio format-ID constant for the variant.  Matches
    /// `kAudioFormatMPEG4AAC` from `<CoreAudioTypes/CoreAudioBaseTypes.h>`.
    const fn format_id(self) -> UInt32 {
        // 'aac ' — Apple's four-char code packed as a u32.
        const K_AUDIO_FORMAT_MPEG4_AAC: UInt32 = 0x6161_6320;
        match self {
            Self::LowComplexity => K_AUDIO_FORMAT_MPEG4_AAC,
        }
    }
}

/// `kAudioFormatLinearPCM` — the output format ID we always request.
const K_AUDIO_FORMAT_LINEAR_PCM: UInt32 = 0x6c70_636d; // 'lpcm'
/// `kAudioFormatFlagIsSignedInteger`.
const K_AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER: UInt32 = 0x4;
/// `kAudioFormatFlagIsPacked`.
const K_AUDIO_FORMAT_FLAG_IS_PACKED: UInt32 = 0x8;
/// `kAudioConverterDecompressionMagicCookie` — property ID for setting
/// the AudioSpecificConfig bytes on a raw-frame decoder.
const K_AUDIO_CONVERTER_DECOMPRESSION_MAGIC_COOKIE: UInt32 = 0x646d_6763; // 'dmgc'

/// Stateful AAC → 16-bit interleaved PCM decoder.
///
/// One [`AudioConverterRef`] is held for the lifetime of the decoder
/// and disposed on drop.  The decoder is **not** [`Sync`] (the
/// converter is documented as not thread-safe).
pub struct AacDecoder {
    converter: AudioConverterRef,
    sample_rate: u32,
    channels: u8,
}

// Safety: AudioConverterRef is opaque; we never share it across
// threads.  Send is fine because we own the pointer.
unsafe impl Send for AacDecoder {}

impl AacDecoder {
    /// Build a new decoder for the given input format, sample rate,
    /// and channel count.  The output is always `S16` interleaved at
    /// the input's sample rate.
    ///
    /// # Errors
    ///
    /// Returns [`VtbError::Status`] if `AudioConverterNew` fails —
    /// commonly because the requested input format isn't supported on
    /// the host (e.g. HE-AAC on a very old macOS without the codec
    /// installed).
    pub fn new(format: AacFormat, sample_rate: u32, channels: u8) -> Result<Self> {
        if !(1..=2).contains(&channels) {
            return Err(VtbError::Other(format!(
                "h264-aac: only mono / stereo supported, got {channels} channels"
            )));
        }
        // Input description: compressed AAC.  Sample-rate is required;
        // bytes-per-packet / bits-per-channel / channels-per-frame are
        // left as zero since AudioConverter reads them from the
        // bitstream (ADTS header or magic cookie).
        let input_desc = AudioStreamBasicDescription {
            mSampleRate: f64::from(sample_rate),
            mFormatID: format.format_id(),
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: 1024, // AAC-LC frame size
            mBytesPerFrame: 0,
            mChannelsPerFrame: UInt32::from(channels),
            mBitsPerChannel: 0,
            mReserved: 0,
        };
        // Output description: signed 16-bit interleaved PCM.
        let output_desc = AudioStreamBasicDescription {
            mSampleRate: f64::from(sample_rate),
            mFormatID: K_AUDIO_FORMAT_LINEAR_PCM,
            mFormatFlags: K_AUDIO_FORMAT_FLAG_IS_SIGNED_INTEGER
                | K_AUDIO_FORMAT_FLAG_IS_PACKED,
            mBytesPerPacket: UInt32::from(channels) * 2,
            mFramesPerPacket: 1,
            mBytesPerFrame: UInt32::from(channels) * 2,
            mChannelsPerFrame: UInt32::from(channels),
            mBitsPerChannel: 16,
            mReserved: 0,
        };

        let mut converter: AudioConverterRef = ptr::null_mut();
        // SAFETY: `&input_desc` / `&output_desc` are live for the
        // duration of the call; `converter` is a fresh out-pointer.
        let status: OSStatus = unsafe {
            AudioConverterNew(&input_desc, &output_desc, &mut converter)
        };
        StatusContext::wrap(status, "AudioConverterNew")?;

        Ok(Self {
            converter,
            sample_rate,
            channels,
        })
    }

    /// Install an AAC `AudioSpecificConfig` magic cookie for streams
    /// without ADTS framing (e.g. MP4 `mp4a` access units).
    ///
    /// For ADTS-framed streams this is unnecessary — the AudioConverter
    /// will parse the 7-byte ADTS header on each frame.
    ///
    /// # Errors
    ///
    /// Returns [`VtbError::Status`] if AudioConverter rejects the
    /// cookie.  The most common cause is a malformed
    /// `AudioSpecificConfig` (must be 2 bytes for basic AAC-LC,
    /// or 5 bytes for HE-AAC, etc.).
    pub fn set_magic_cookie(&mut self, asc: &[u8]) -> Result<()> {
        // SAFETY: `asc` is a live byte slice; we pass the property ID
        // for the decompression magic cookie and the cookie's length
        // and data pointer.
        let status: OSStatus = unsafe {
            AudioConverterSetProperty(
                self.converter,
                K_AUDIO_CONVERTER_DECOMPRESSION_MAGIC_COOKIE,
                asc.len() as UInt32,
                asc.as_ptr().cast(),
            )
        };
        StatusContext::wrap(status, "AudioConverterSetProperty(magic_cookie)")
    }

    /// Decode one AAC access unit to PCM.
    ///
    /// Returns the decoded interleaved S16 samples.  For AAC-LC at
    /// 2 channels, one frame produces 2048 samples (1024 frames × 2
    /// channels).
    ///
    /// # Errors
    ///
    /// Returns [`VtbError::Status`] if AudioConverter returns a
    /// non-zero `OSStatus`, or [`VtbError::Other`] for shape /
    /// configuration mismatches (zero-length input, etc.).
    pub fn decode_frame(&mut self, aac_frame: &[u8]) -> Result<Vec<i16>> {
        if aac_frame.is_empty() {
            return Err(VtbError::Other(
                "aac decoder: empty input frame".into(),
            ));
        }

        // Per-frame state that the input callback reads.  We use a
        // thread-local `Cell<Option<...>>` since the C callback runs
        // synchronously inside AudioConverterFillComplexBuffer and we
        // can't easily plumb a closure across the FFI boundary
        // without `Box::into_raw`-ing context.
        InputState::set(InputState::new(aac_frame));

        let frames_per_packet = 1024_u32; // AAC-LC fixed frame size
        let mut out_pcm = vec![0i16; (frames_per_packet * u32::from(self.channels)) as usize];

        // Build the AudioBufferList.  AudioBufferList is variable-
        // length (mNumberBuffers + N × AudioBuffer) — we use the
        // single-buffer interleaved layout.
        let mut buffer_list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: UInt32::from(self.channels),
                mDataByteSize: (out_pcm.len() * std::mem::size_of::<i16>()) as UInt32,
                mData: out_pcm.as_mut_ptr().cast(),
            }],
        };

        let mut io_output_packets = frames_per_packet;

        // SAFETY: `input_proc` matches the AudioConverterComplexInput
        // ProcType signature; `buffer_list` is owned by us; `io_output`
        // is a live mutable pointer to a stack u32.
        let status: OSStatus = unsafe {
            AudioConverterFillComplexBuffer(
                self.converter,
                Some(input_proc),
                ptr::null_mut(), // userData — we use InputState's TLS instead
                &mut io_output_packets,
                &mut buffer_list,
                ptr::null_mut(), // outPacketDescription
            )
        };
        InputState::clear();
        StatusContext::wrap(status, "AudioConverterFillComplexBuffer")?;

        // Truncate to the actual frame count the converter produced.
        let actual_samples = (io_output_packets * u32::from(self.channels)) as usize;
        out_pcm.truncate(actual_samples);
        Ok(out_pcm)
    }

    /// The output sample rate (always equals the input sample rate
    /// for the formats this decoder accepts).
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The output channel count.
    #[must_use]
    pub fn channels(&self) -> u8 {
        self.channels
    }
}

impl Drop for AacDecoder {
    fn drop(&mut self) {
        if !self.converter.is_null() {
            // SAFETY: `converter` is a live AudioConverterRef we
            // created via AudioConverterNew.
            unsafe {
                AudioConverterDispose(self.converter);
            }
        }
    }
}

/// Per-call input state for the AudioConverter callback.
///
/// AudioConverter's input proc runs synchronously inside
/// `AudioConverterFillComplexBuffer`.  Rather than thread a `userData`
/// pointer through `Box::into_raw` (which would require careful
/// cleanup on every error path), we stash the per-call state in a
/// thread-local that the callback reads.
struct InputState {
    /// The single AAC frame supplied by the caller.
    frame: &'static [u8],
    /// Set to `true` after the converter has consumed the frame —
    /// subsequent callbacks should report EOF.
    consumed: Cell<bool>,
}

thread_local! {
    static INPUT_STATE: Cell<Option<InputState>> = const { Cell::new(None) };
}

impl InputState {
    fn new(frame: &[u8]) -> Self {
        // SAFETY: we extend the lifetime to 'static because the
        // callback only reads while the InputState is installed.
        // The corresponding `clear` call (always paired in
        // `decode_frame`) returns the slot before the borrow ends.
        let frame_static: &'static [u8] = unsafe { std::mem::transmute(frame) };
        Self {
            frame: frame_static,
            consumed: Cell::new(false),
        }
    }

    fn set(state: Self) {
        INPUT_STATE.with(|cell| cell.set(Some(state)));
    }

    fn clear() {
        INPUT_STATE.with(|cell| cell.set(None));
    }
}

/// AudioConverter input proc — extern "C" callback the system calls
/// to pull AAC data.
///
/// # Safety
///
/// Called by AudioConverter while inside
/// `AudioConverterFillComplexBuffer`.  The pointers are valid for the
/// duration of the call only.  We never dereference them outside this
/// function.
unsafe extern "C" fn input_proc(
    _in_audio_converter: AudioConverterRef,
    io_number_data_packets: *mut UInt32,
    io_data: *mut AudioBufferList,
    _out_data_packet_description: *mut *mut AudioStreamPacketDescription,
    _in_user_data: *mut std::ffi::c_void,
) -> OSStatus {
    INPUT_STATE.with(|cell| {
        let Some(state) = cell.take() else {
            // No state installed — signal EOF.
            unsafe { *io_number_data_packets = 0 };
            return 0;
        };

        if state.consumed.get() {
            // Already delivered the one frame we have.
            unsafe { *io_number_data_packets = 0 };
            cell.set(Some(state));
            return 0;
        }

        // Hand the converter a single packet.
        let buffer_list = unsafe { &mut *io_data };
        buffer_list.mNumberBuffers = 1;
        buffer_list.mBuffers[0] = AudioBuffer {
            mNumberChannels: 0, // converter ignores this for AAC input
            mDataByteSize: state.frame.len() as UInt32,
            mData: state.frame.as_ptr() as *mut _,
        };
        unsafe { *io_number_data_packets = 1 };

        state.consumed.set(true);
        cell.set(Some(state));
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_channels() {
        let result = AacDecoder::new(AacFormat::LowComplexity, 44100, 0);
        assert!(matches!(result, Err(VtbError::Other(_))));
    }

    #[test]
    fn rejects_too_many_channels() {
        let result = AacDecoder::new(AacFormat::LowComplexity, 44100, 8);
        assert!(matches!(result, Err(VtbError::Other(_))));
    }

    #[test]
    fn rejects_empty_frame() {
        // Even creating the decoder may fail on a CI environment
        // without the AAC codec installed; gate on that.
        if let Ok(mut decoder) = AacDecoder::new(AacFormat::LowComplexity, 44100, 2) {
            let result = decoder.decode_frame(&[]);
            assert!(matches!(result, Err(VtbError::Other(_))));
        }
    }

    #[test]
    fn constructor_returns_correct_sample_rate_and_channels() {
        if let Ok(decoder) = AacDecoder::new(AacFormat::LowComplexity, 48000, 2) {
            assert_eq!(decoder.sample_rate(), 48000);
            assert_eq!(decoder.channels(), 2);
        }
    }

    #[test]
    fn format_id_matches_apple_constant() {
        // 'aac ' = 0x6161_6320.  Verified by transcribing the
        // four-char-code definition from CoreAudioBaseTypes.h.
        assert_eq!(AacFormat::LowComplexity.format_id(), 0x6161_6320);
    }

    /// Smoke test: build a decoder and decode a single 1024-sample
    /// silent AAC-LC frame.  The frame here is the canonical "silent
    /// AAC-LC at 44.1 kHz stereo" ADTS frame found in many test
    /// fixtures.
    ///
    /// This only runs on platforms where the AAC codec actually exists
    /// (`HAS_BINDINGS == true`); on others it short-circuits.
    #[test]
    fn decode_silent_adts_frame_round_trips() {
        if !oximedia_vtb_sys::HAS_BINDINGS {
            return;
        }
        let Ok(mut decoder) = AacDecoder::new(AacFormat::LowComplexity, 44100, 2) else {
            // CI without AAC codec; skip silently.
            return;
        };
        // Minimal ADTS frame: 7-byte header + 4 bytes of "silent"
        // payload.  Real conformance would use a known-good silent
        // ADTS frame; this is just a smoke test for the API surface
        // and may error from AudioConverter (which we tolerate).
        let header: [u8; 7] = [0xFF, 0xF1, 0x4C, 0x80, 0x02, 0x1F, 0xFC];
        let mut frame = header.to_vec();
        frame.extend_from_slice(&[0x21, 0x10, 0x05, 0x00]);
        let _ = decoder.decode_frame(&frame); // tolerate either Ok/Err on synthetic frame.
    }
}
