//! HEVC / H.265 RTP depacketization (RFC 7798).
//!
//! HEVC's RTP payload format is structurally similar to H.264's RFC 6184
//! but uses a **2-byte NAL unit header** (vs H.264's 1-byte) and a
//! different set of RTP packetization "types":
//!
//! 1. **Single NAL unit packet** (type 0..47) — one RTP packet carries
//!    one whole NAL unit. The first two bytes of the payload *are* the
//!    NAL unit header.
//! 2. **AP** (Aggregation Packet, type 48) — one packet carries N NAL
//!    units that share a timestamp. Each NAL is prefixed with a 2-byte
//!    big-endian length, mirroring H.264 STAP-A. Used for sending
//!    `VPS + SPS + PPS` together (and often + an IRAP slice) in one
//!    packet.
//! 3. **FU** (Fragmentation Unit, type 49) — one NAL unit *split across*
//!    multiple RTP packets. Mandatory for any frame larger than the
//!    MTU (~1400 bytes) — i.e. nearly every IRAP.
//!
//! The PACI type (50) carries optional payload-content information
//! around a wrapped NAL.  Real-world streams rarely use it; this
//! depacketizer rejects PACI with a `Protocol` error.
//!
//! ## Output
//!
//! Same shape as the H.264 depacketizer — an [`HevcAccessUnit`] with
//! Annex-B framed bytes (each NAL prefixed by `00 00 00 01`) plus a
//! `keyframe` flag set when an IRAP NAL (types 16..=23 — BLA, IDR,
//! CRA) appears in the unit.
//!
//! ## Pipeline
//!
//! ```text
//!   RTSP client → InterleavedPacket.data → RtpPacket::parse → payload, marker
//!                                                            │
//!                                                            ▼
//!                                              HevcDepacketizer::process
//!                                                            │
//!                                                            ▼
//!                                              Option<HevcAccessUnit { annex_b, .. }>
//!                                                            │
//!                                                            ▼
//!                                              VTB / software HEVC decoder
//! ```

use crate::error::NetError;

/// HEVC NAL unit-type field is bits 1..7 of the *first* NAL header byte.
fn nal_type_of(header_byte0: u8) -> u8 {
    (header_byte0 >> 1) & 0x3F
}

/// RTP payload "type" for HEVC Aggregation Packets (AP).
const RTP_TYPE_AP: u8 = 48;
/// RTP payload "type" for HEVC Fragmentation Units (FU).
const RTP_TYPE_FU: u8 = 49;
/// RTP payload "type" for HEVC PACI packets (rarely seen).
const RTP_TYPE_PACI: u8 = 50;

/// HEVC IRAP (Intra Random Access Point) NAL types — the keyframe-class
/// equivalents.  Covers BLA_W_LP through CRA_NUT (spec Table 7-1, types
/// 16..=23 inclusive).
fn is_irap(nal_type: u8) -> bool {
    (16..=23).contains(&nal_type)
}

/// Annex-B 4-byte start code prefixed in front of every emitted NAL.
const START_CODE: &[u8] = &[0x00, 0x00, 0x00, 0x01];

/// One complete HEVC access unit — i.e. one frame's worth of NAL units.
#[derive(Debug, Clone)]
pub struct HevcAccessUnit {
    /// Annex-B framed bytestream (each NAL prefixed by `00 00 00 01`).
    pub annex_b: Vec<u8>,
    /// True if this access unit contains an IRAP NAL (BLA / IDR / CRA).
    pub keyframe: bool,
}

/// Stateful RFC 7798 depacketizer.
///
/// Feed each RTP packet's payload and marker bit to [`Self::process`];
/// receive `Some(HevcAccessUnit)` whenever the marker bit signals end of
/// frame.  The depacketizer holds internal buffers for FU reassembly and
/// the in-flight access unit; it is not [`Sync`] and is intended for
/// use from a single demuxer thread.
#[derive(Debug, Default)]
pub struct HevcDepacketizer {
    /// Bytes accumulated for a FU in progress (with the synthesized 2-byte
    /// NAL header at the start).
    fu_buffer: Vec<u8>,
    /// True while we're between a FU "start" packet and its "end".
    fu_in_progress: bool,
    /// Annex-B bytes for the access unit currently being assembled.
    current_au: Vec<u8>,
    /// Set the moment any IRAP NAL is appended to the current access unit.
    current_au_has_keyframe: bool,
}

impl HevcDepacketizer {
    /// Construct an empty depacketizer.
    ///
    /// # Example
    ///
    /// ```
    /// use oximedia_net::depacketize::HevcDepacketizer;
    /// let dep = HevcDepacketizer::new();
    /// assert!(!dep.has_pending_fragment());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True while we're mid-FU and waiting for further fragments.
    ///
    /// Useful for upstream sequence-loss detection: if the caller's
    /// sequence tracker reports a gap while this returns `true`, the
    /// in-progress NAL is unrecoverable and should be dropped via
    /// [`Self::reset`].
    #[must_use]
    pub fn has_pending_fragment(&self) -> bool {
        self.fu_in_progress
    }

    /// Drop any in-progress fragment and unfinished access unit.
    ///
    /// Call after detecting RTP packet loss in the middle of a FU
    /// sequence, or when re-syncing after PAUSE/PLAY.
    pub fn reset(&mut self) {
        self.fu_buffer.clear();
        self.fu_in_progress = false;
        self.current_au.clear();
        self.current_au_has_keyframe = false;
    }

    /// Feed one RTP packet's HEVC payload.
    ///
    /// `payload` is the bytes *after* the RTP header (the slice returned
    /// by [`RtpPacket::parse`](crate::rtsp::RtpPacket::parse) as
    /// `pkt.payload`).  `marker` is the RTP marker bit; per RFC 7798
    /// § 4.4 this is set on the last packet of an access unit.
    ///
    /// Returns `Ok(Some(HevcAccessUnit))` when `marker` is true and at
    /// least one NAL has accumulated since the last AU.  Returns
    /// `Ok(None)` when more packets are needed.
    ///
    /// # Errors
    ///
    /// Returns [`NetError::Protocol`] for empty payloads, malformed AP
    /// length prefixes, truncated FU headers, or PACI / reserved payload
    /// types.
    ///
    /// # Example
    ///
    /// ```
    /// use oximedia_net::depacketize::HevcDepacketizer;
    ///
    /// // A single NAL unit packet carrying a 2-byte HEVC NAL header
    /// // (F=0, type=1=TRAIL_N, layer=0, tid=1) plus 2 bytes of slice
    /// // payload, with marker=true to close the access unit.
    /// let mut dep = HevcDepacketizer::new();
    /// let payload = [0x02, 0x01, 0xAA, 0xBB];
    /// let au = dep.process(&payload, true).unwrap().expect("AU emitted");
    /// assert_eq!(&au.annex_b[..4], &[0, 0, 0, 1]);
    /// assert_eq!(&au.annex_b[4..], &payload);
    /// assert!(!au.keyframe);
    /// ```
    pub fn process(
        &mut self,
        payload: &[u8],
        marker: bool,
    ) -> Result<Option<HevcAccessUnit>, NetError> {
        if payload.len() < 2 {
            return Err(NetError::Protocol(
                "HEVC RTP payload shorter than 2-byte NAL header".into(),
            ));
        }
        let rtp_type = nal_type_of(payload[0]);

        match rtp_type {
            0..=47 => self.append_nal(payload),
            RTP_TYPE_AP => self.handle_ap(payload)?,
            RTP_TYPE_FU => self.handle_fu(payload)?,
            RTP_TYPE_PACI => {
                return Err(NetError::Protocol(
                    "HEVC PACI payload type not supported".into(),
                ));
            }
            _ => {
                return Err(NetError::Protocol(format!(
                    "invalid HEVC RTP payload type: {rtp_type}"
                )));
            }
        }

        if marker {
            Ok(self.take_access_unit())
        } else {
            Ok(None)
        }
    }

    /// Internal: append a single NAL unit to the in-flight access unit,
    /// prepending the Annex-B start code and tracking IRAP presence.
    fn append_nal(&mut self, nal: &[u8]) {
        if nal.is_empty() {
            return;
        }
        let nal_type = nal_type_of(nal[0]);
        if is_irap(nal_type) {
            self.current_au_has_keyframe = true;
        }
        self.current_au.extend_from_slice(START_CODE);
        self.current_au.extend_from_slice(nal);
    }

    /// Internal: pull the assembled AU out, leaving the buffers empty.
    fn take_access_unit(&mut self) -> Option<HevcAccessUnit> {
        if self.current_au.is_empty() {
            return None;
        }
        let annex_b = std::mem::take(&mut self.current_au);
        let keyframe = self.current_au_has_keyframe;
        self.current_au_has_keyframe = false;
        Some(HevcAccessUnit { annex_b, keyframe })
    }

    /// AP: payload = [AP NAL header (2 bytes)] [(2-byte len)(NAL bytes)]+
    ///
    /// Per RFC 7798 § 4.4.2, each aggregated NAL is prefixed by a 2-byte
    /// big-endian length followed by the NAL unit's own bytes (which
    /// include the NAL's normal 2-byte header).
    fn handle_ap(&mut self, payload: &[u8]) -> Result<(), NetError> {
        // Skip the 2-byte AP-payload-header that introduces the packet.
        let mut i = 2;
        while i < payload.len() {
            if i + 2 > payload.len() {
                return Err(NetError::Protocol(
                    "AP length field truncated".into(),
                ));
            }
            let len = u16::from_be_bytes([payload[i], payload[i + 1]]) as usize;
            i += 2;
            if len == 0 {
                return Err(NetError::Protocol("AP zero-length NAL".into()));
            }
            if i + len > payload.len() {
                return Err(NetError::Protocol(
                    "AP NAL extends past payload".into(),
                ));
            }
            self.append_nal(&payload[i..i + len]);
            i += len;
        }
        Ok(())
    }

    /// FU: payload = [PayloadHdr (2 bytes)] [FU header (1 byte)]
    ///                                     [fragmented NAL payload]
    ///
    /// Per RFC 7798 § 4.4.3, the FU header byte is:
    ///   S (1 bit) | E (1 bit) | FuType (6 bits)
    ///
    /// The synthesized NAL header is:
    ///   byte0 = (PayloadHdr[0] & 0x81) | (FuType << 1)
    ///   byte1 = PayloadHdr[1]
    ///
    /// I.e. the F bit + LayerID-bit-6 come from the PayloadHdr's first
    /// byte; the NAL type comes from the FU header's FuType field; and
    /// the second header byte (LayerID lower 5 + TID 3) carries through
    /// from the PayloadHdr unchanged.
    fn handle_fu(&mut self, payload: &[u8]) -> Result<(), NetError> {
        if payload.len() < 3 {
            return Err(NetError::Protocol("FU header truncated".into()));
        }
        let fu_header = payload[2];
        let start = fu_header & 0x80 != 0;
        let end = fu_header & 0x40 != 0;
        let nal_type_inner = fu_header & 0x3F;

        if start {
            // Start of a new fragmented NAL.  If we were already mid-FU
            // (a packet-loss scenario), the partial buffer is
            // unrecoverable — drop it and start fresh.
            self.fu_buffer.clear();
            let header_byte0 = (payload[0] & 0x81) | (nal_type_inner << 1);
            let header_byte1 = payload[1];
            self.fu_buffer.push(header_byte0);
            self.fu_buffer.push(header_byte1);
            self.fu_in_progress = true;
        }

        if !self.fu_in_progress {
            // Middle/end fragment without a corresponding start — packet
            // loss recovery.  Skip silently; the upstream sequence
            // tracker is the right place to surface the loss.
            return Ok(());
        }

        // Append the fragment payload (everything after the 3-byte FU
        // header: 2 bytes PayloadHdr + 1 byte FU header).
        if payload.len() > 3 {
            self.fu_buffer.extend_from_slice(&payload[3..]);
        }

        if end {
            let nal = std::mem::take(&mut self.fu_buffer);
            self.fu_in_progress = false;
            self.append_nal(&nal);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HEVC NAL type 1 = TRAIL_N (non-IRAP).
    const NAL_TYPE_TRAIL_N: u8 = 1;
    /// HEVC NAL type 19 = IDR_W_RADL (IRAP).
    const NAL_TYPE_IDR_W_RADL: u8 = 19;
    /// HEVC NAL type 21 = CRA_NUT (IRAP).
    const NAL_TYPE_CRA_NUT: u8 = 21;

    /// Helper: build a 2-byte HEVC NAL header for the given type, with
    /// F=0, LayerID=0, TID=1.  Matches the spec convention.
    fn hevc_nal_header(nal_type: u8) -> [u8; 2] {
        let byte0 = (nal_type & 0x3F) << 1;
        let byte1 = 0x01;
        [byte0, byte1]
    }

    /// Helper: a single-NAL RTP payload of the given type + body.
    fn single_nal(nal_type: u8, body: &[u8]) -> Vec<u8> {
        let hdr = hevc_nal_header(nal_type);
        let mut out = vec![hdr[0], hdr[1]];
        out.extend_from_slice(body);
        out
    }

    /// Helper: AP payload aggregating the given NAL units (each NAL
    /// already includes its own 2-byte NAL header).
    fn ap(nals: &[&[u8]]) -> Vec<u8> {
        // AP-payload-header: type=48, otherwise zero.
        let hdr = hevc_nal_header(RTP_TYPE_AP);
        let mut out = vec![hdr[0], hdr[1]];
        for nal in nals {
            let len = nal.len() as u16;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(nal);
        }
        out
    }

    /// Helper: FU packet for a NAL of `nal_type`, carrying `fragment` as
    /// the payload chunk, with start/end flags as requested.
    fn fu(nal_type: u8, start: bool, end: bool, fragment: &[u8]) -> Vec<u8> {
        // PayloadHdr: type=49, F=0, layer=0, tid=1.
        let hdr = hevc_nal_header(RTP_TYPE_FU);
        // FU header: S | E | FuType.
        let mut fu_header = nal_type & 0x3F;
        if start {
            fu_header |= 0x80;
        }
        if end {
            fu_header |= 0x40;
        }
        let mut out = vec![hdr[0], hdr[1], fu_header];
        out.extend_from_slice(fragment);
        out
    }

    #[test]
    fn single_nal_packet_emits_au_on_marker() {
        let mut dep = HevcDepacketizer::new();
        let payload = single_nal(NAL_TYPE_TRAIL_N, b"slice-bytes");
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        assert_eq!(&au.annex_b[..4], &[0, 0, 0, 1]);
        assert_eq!(&au.annex_b[4..], payload.as_slice());
        assert!(!au.keyframe, "TRAIL_N is not IRAP");
    }

    #[test]
    fn idr_single_nal_sets_keyframe_flag() {
        let mut dep = HevcDepacketizer::new();
        let payload = single_nal(NAL_TYPE_IDR_W_RADL, b"idr-slice");
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        assert!(au.keyframe);
    }

    #[test]
    fn cra_single_nal_sets_keyframe_flag() {
        let mut dep = HevcDepacketizer::new();
        let payload = single_nal(NAL_TYPE_CRA_NUT, b"cra-slice");
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        assert!(au.keyframe);
    }

    #[test]
    fn no_marker_no_au_emitted() {
        let mut dep = HevcDepacketizer::new();
        let payload = single_nal(NAL_TYPE_TRAIL_N, b"slice");
        let result = dep.process(&payload, false).expect("ok");
        assert!(result.is_none(), "AU must wait for marker bit");
    }

    #[test]
    fn ap_packs_multiple_nals_into_one_au() {
        let mut dep = HevcDepacketizer::new();
        let vps = single_nal(32, b"vps");
        let sps = single_nal(33, b"sps");
        let pps = single_nal(34, b"pps");
        let payload = ap(&[&vps, &sps, &pps]);
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        // Three NALs, each with its own Annex-B start code.
        let parts: Vec<&[u8]> = au.annex_b.split(|b| *b == 0).collect();
        let start_codes = au
            .annex_b
            .windows(4)
            .filter(|w| *w == &[0, 0, 0, 1])
            .count();
        assert_eq!(start_codes, 3);
        assert!(!au.keyframe, "VPS/SPS/PPS are not IRAP slices");
        let _ = parts;
    }

    #[test]
    fn ap_with_idr_sets_keyframe() {
        let mut dep = HevcDepacketizer::new();
        let sps = single_nal(33, b"sps");
        let idr = single_nal(NAL_TYPE_IDR_W_RADL, b"idr");
        let payload = ap(&[&sps, &idr]);
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        assert!(au.keyframe);
    }

    #[test]
    fn ap_rejects_truncated_length() {
        let mut dep = HevcDepacketizer::new();
        // AP header + a single byte after — not enough for the 2-byte
        // length field.
        let payload = vec![hevc_nal_header(RTP_TYPE_AP)[0], hevc_nal_header(RTP_TYPE_AP)[1], 0xFF];
        let result = dep.process(&payload, true);
        assert!(matches!(result, Err(NetError::Protocol(_))));
    }

    #[test]
    fn fu_reassembles_split_nal() {
        let mut dep = HevcDepacketizer::new();
        // A FU sequence carrying an IDR_W_RADL NAL split across 3
        // packets.  Each fragment carries part of the slice payload.
        let frag_a = fu(NAL_TYPE_IDR_W_RADL, true, false, b"part-A");
        let frag_b = fu(NAL_TYPE_IDR_W_RADL, false, false, b"part-B");
        let frag_c = fu(NAL_TYPE_IDR_W_RADL, false, true, b"part-C");

        assert!(dep.process(&frag_a, false).expect("ok").is_none());
        assert!(dep.has_pending_fragment());
        assert!(dep.process(&frag_b, false).expect("ok").is_none());
        assert!(dep.has_pending_fragment());
        let au = dep
            .process(&frag_c, true)
            .expect("ok")
            .expect("AU emitted on marker");
        assert!(!dep.has_pending_fragment());

        assert!(au.keyframe);
        // Annex-B start code + synthesized 2-byte NAL header + parts.
        let expected_header = hevc_nal_header(NAL_TYPE_IDR_W_RADL);
        assert_eq!(&au.annex_b[..4], &[0, 0, 0, 1]);
        assert_eq!(&au.annex_b[4..6], &expected_header);
        assert_eq!(&au.annex_b[6..], b"part-Apart-Bpart-C");
    }

    #[test]
    fn fu_middle_fragment_without_start_is_skipped() {
        let mut dep = HevcDepacketizer::new();
        // Middle fragment shows up without ever seeing a start — packet
        // loss scenario.  Depacketizer must not crash and must not emit
        // anything when the marker eventually arrives if nothing else is
        // accumulated.
        let middle = fu(NAL_TYPE_TRAIL_N, false, false, b"orphan");
        dep.process(&middle, false).expect("ok");
        assert!(!dep.has_pending_fragment());
        let au = dep.process(&middle, true).expect("ok");
        assert!(au.is_none());
    }

    #[test]
    fn reset_drops_in_flight_state() {
        let mut dep = HevcDepacketizer::new();
        let frag_a = fu(NAL_TYPE_TRAIL_N, true, false, b"part-A");
        dep.process(&frag_a, false).expect("ok");
        assert!(dep.has_pending_fragment());

        dep.reset();
        assert!(!dep.has_pending_fragment());

        // After reset, a new single-NAL packet with marker emits an AU
        // unaffected by the abandoned FU.
        let payload = single_nal(NAL_TYPE_TRAIL_N, b"fresh");
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        assert_eq!(&au.annex_b[4..], payload.as_slice());
    }

    #[test]
    fn paci_payload_type_is_rejected() {
        let mut dep = HevcDepacketizer::new();
        let payload = vec![hevc_nal_header(RTP_TYPE_PACI)[0], hevc_nal_header(RTP_TYPE_PACI)[1]];
        let result = dep.process(&payload, true);
        assert!(matches!(result, Err(NetError::Protocol(_))));
    }

    #[test]
    fn truncated_payload_rejected() {
        let mut dep = HevcDepacketizer::new();
        // Only 1 byte — not enough for the 2-byte HEVC NAL header.
        let result = dep.process(&[0x02], true);
        assert!(matches!(result, Err(NetError::Protocol(_))));
    }

    #[test]
    fn empty_au_returns_none_on_marker() {
        let mut dep = HevcDepacketizer::new();
        // Marker on first call but no NAL accumulated yet — nothing to
        // emit.  We need at least one valid NAL beforehand; otherwise
        // the call errors on the short payload.
        let payload = single_nal(NAL_TYPE_TRAIL_N, b"slice");
        let au = dep
            .process(&payload, true)
            .expect("ok")
            .expect("AU emitted");
        assert!(!au.annex_b.is_empty());
        // Subsequent marker with no further input returns None.
        let au2 = dep.process(&payload, false).expect("ok");
        assert!(au2.is_none());
    }
}
