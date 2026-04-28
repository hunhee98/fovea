//! fovea-mv-stream
//!
//! Source adapters for fovea-mv. Wraps `ffmpeg-next` to feed motion-vector
//! packets into `fovea-mv-core`.
//!
//! Step 2 scope (current): file source — open + decoder init + packet iteration
//! emitting [`MvPacket`] with `frame_type` + `ts_us`. MV extraction
//! (`+export_mvs` side data) lands in step 2.4.

#![warn(missing_docs)]

pub mod hevc;

use std::path::Path;

use ffmpeg_next as ffmpeg;
use ffmpeg::format::context::Input;
use ffmpeg::media::Type as MediaType;
use ffmpeg::util::frame::video::Video as VideoFrame;
use ffmpeg::util::picture::Type as PictureType;
use ffmpeg::software::scaling::{context::Context as Scaler, flag::Flags as ScalerFlags};
use ffmpeg::util::format::pixel::Pixel;
use fovea_mv_core::{FrameType, MotionVector, MvPacket};
use thiserror::Error;

/// Error type for source operations.
#[derive(Debug, Error)]
pub enum SourceError {
    /// ffmpeg-next returned an error.
    #[error("ffmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg::Error),
    /// No video stream found in the container.
    #[error("no video stream in input")]
    NoVideoStream,
    /// Codec is not supported (currently only H.264 is wired up).
    #[error("unsupported codec: {0:?}")]
    UnsupportedCodec(ffmpeg::codec::Id),
    /// A live source (RTSP / RTMP / SRT / RTP / UDP) reached EOF or its
    /// connection was reset. For file inputs, end-of-stream is reported as
    /// `Ok(None)` from `next_packet` and never as this variant.
    #[error("live source disconnected")]
    Disconnected,
    /// Underlying I/O exceeded the configured `read_timeout_ms` while
    /// waiting for the next packet from a live source.
    #[error("live source read timed out")]
    ReadTimeout,
    /// libde265 (HEVC backend) returned an error.
    #[error("hevc backend error: {0}")]
    Hevc(String),
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, SourceError>;

/// Static metadata of a video stream, captured at open time.
#[derive(Debug, Clone)]
pub struct VideoInfo {
    /// Codec identifier (e.g. H264).
    pub codec: ffmpeg::codec::Id,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Frame rate as (num, den). May be 0/0 if unavailable.
    pub frame_rate: (i32, i32),
    /// Time base of the stream (PTS units; seconds = num/den).
    pub time_base: (i32, i32),
    /// Duration in microseconds, or `None` if not known.
    pub duration_us: Option<i64>,
    /// Total frame count if reported by the container, else `None`.
    pub nb_frames_hint: Option<i64>,
}

/// Decoder configuration toggles.
///
/// `+export_mvs` is always enabled; the rest are opt-in fast paths described
/// as "A'" in `docs/05.exec-plans/001-mvtrigger-mvp.md`. Enabling them speeds
/// up decode but **corrupts reconstructed pixel output** (motion vectors
/// remain accurate because they come from the bitstream, not from
/// reconstructed pixels).
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenOptions {
    /// Set `skip_loop_filter = all` on the decoder. Skips deblocking.
    pub skip_loop_filter_all: bool,
    /// Set `skip_idct = all` on the decoder. Skips inverse DCT — reference
    /// frames will be wrong, so don't read pixels from this source.
    pub skip_idct_all: bool,
}

/// RTSP transport mode.
///
/// TCP is the default — most cameras support it, it's firewall-friendly,
/// and packets aren't dropped silently. UDP exists for low-latency LAN
/// deployments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtspTransport {
    /// `rtsp_transport=tcp` (recommended, default).
    Tcp,
    /// `rtsp_transport=udp`.
    Udp,
}

impl RtspTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

/// Network-source configuration.
///
/// Defaults are tuned for a typical IP-camera deployment: TCP transport,
/// 5 s open and read timeouts, no reconnect.
#[derive(Debug, Clone)]
pub struct NetworkOptions {
    /// RTSP transport (ignored for non-RTSP URLs).
    pub transport: RtspTransport,
    /// Maximum time to wait for `avformat_open_input` to succeed, in
    /// milliseconds. Translated to libavformat's microsecond-grained
    /// `stimeout` option (which covers both TCP socket connect and the
    /// initial protocol handshake).
    pub open_timeout_ms: u32,
    /// Maximum time to wait for the next packet during the decode loop, in
    /// milliseconds. Used by Step 002.3; the option dictionary path also
    /// passes this value as `stimeout` so libavformat enforces it on the
    /// socket layer.
    pub read_timeout_ms: u32,
    /// Number of automatic reopens on transient network failures. Used by
    /// Step 002.3. `0` means "no reconnect" (current behavior).
    pub max_reconnects: u32,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            transport: RtspTransport::Tcp,
            open_timeout_ms: 5_000,
            read_timeout_ms: 5_000,
            max_reconnects: 0,
        }
    }
}

/// Decoder backend chosen at open time based on the input codec.
///
/// H.264 keeps its FFmpeg `+export_mvs` zero-decode-cost MV path. HEVC
/// uses our vendored libde265 + `de265_internals` extension because
/// FFmpeg upstream's HEVC decoder does not emit motion-vector side data.
enum DecoderBackend {
    H264(ffmpeg::decoder::Video),
    Hevc(hevc::HevcDecoder),
}

/// FFmpeg-backed file source.
///
/// Opens an `mp4` (or any libavformat-supported container) file, locates the
/// best video stream, and prepares a decoder context. The decoder may be
/// FFmpeg's avcodec (H.264) or libde265 (HEVC) — see [`DecoderBackend`].
pub struct FfmpegSource {
    input: Input,
    video_stream_idx: usize,
    backend: DecoderBackend,
    info: VideoInfo,
    /// Reusable scratch frame; avoids per-packet allocation. Used only by
    /// the H.264 backend.
    frame_scratch: VideoFrame,
    /// Reusable packet returned to caller; cleared each iteration.
    out_packet: MvPacket,
    /// `true` once container EOF reached and decoder flushed.
    drained: bool,
    /// Lazy YUV→RGB scaler. Constructed on first `last_frame_rgb()` call.
    rgb_scaler: Option<Scaler>,
    /// Reusable RGB destination frame.
    rgb_frame: VideoFrame,
    /// Original input string (URL or path). Required for reconnect.
    saved_input: String,
    /// Network options carried for reconnect.
    saved_network: NetworkOptions,
    /// Decoder options carried for reconnect.
    saved_decoder: OpenOptions,
    /// `true` if the input scheme implies a continuous network stream.
    is_live: bool,
    /// Reconnect attempts already used.
    reconnects_used: u32,
    /// Time-base of the selected stream. Reserved for future HEVC PTS
    /// passthrough (libde265 forwards the `pts` argument from
    /// `de265_push_data` to the decoded picture).
    #[allow(dead_code)]
    stream_time_base: (i32, i32),
    /// Most recent inter-frame's PB info (HEVC) so we can defer copying
    /// until `next_packet` returns the borrow.
    hevc_scratch: Vec<hevc::PbInfo>,
    /// Outcome of the last libde265 decode step's `more` flag.
    last_hevc_more: Option<bool>,
}

/// Returns `true` if a URL refers to a live network source whose `EOF` should
/// be treated as a disconnect rather than end-of-stream.
fn is_live_scheme(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        "rtsp://", "rtsps://", "rtmp://", "rtmps://", "srt://", "udp://", "rtp://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

/// Convert an mp4 length-prefixed (`hvc1` / `avc1`) NAL chunk into
/// Annex-B (start-code prefixed). If the input already starts with an
/// Annex-B start code we pass it through unchanged.
fn mp4_to_annexb_if_needed(input: &[u8]) -> Vec<u8> {
    // Annex-B start codes: 00 00 00 01 or 00 00 01
    if input.starts_with(&[0, 0, 0, 1]) || input.starts_with(&[0, 0, 1]) {
        return input.to_vec();
    }
    let mut out = Vec::with_capacity(input.len() + 16);
    let mut i = 0;
    while i + 4 <= input.len() {
        let nal_len = u32::from_be_bytes(input[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        if i + nal_len > input.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&input[i..i + nal_len]);
        i += nal_len;
    }
    out
}

/// Convert a `hvcC` extradata blob (ISO/IEC 14496-15 §8) into a
/// concatenated Annex-B byte stream. Returns `Err` if the blob is
/// truncated or the configurationVersion field is not 1.
fn hvcc_extradata_to_annexb(hvcc: &[u8]) -> std::result::Result<Vec<u8>, &'static str> {
    // Fixed-size hvcC header is 22 bytes; numOfArrays follows at offset 22.
    if hvcc.len() < 23 {
        return Err("hvcC extradata shorter than 23 bytes");
    }
    if hvcc[0] != 1 {
        return Err("hvcC configurationVersion != 1");
    }
    let num_arrays = hvcc[22] as usize;
    let mut out = Vec::with_capacity(hvcc.len() + num_arrays * 4);
    let mut p = 23usize;
    for _ in 0..num_arrays {
        if p + 3 > hvcc.len() {
            return Err("hvcC truncated array header");
        }
        // hvcc[p]: array_completeness | reserved | NAL_unit_type — ignored.
        let num_nalus = u16::from_be_bytes([hvcc[p + 1], hvcc[p + 2]]) as usize;
        p += 3;
        for _ in 0..num_nalus {
            if p + 2 > hvcc.len() {
                return Err("hvcC truncated NAL length field");
            }
            let nal_len = u16::from_be_bytes([hvcc[p], hvcc[p + 1]]) as usize;
            p += 2;
            if p + nal_len > hvcc.len() {
                return Err("hvcC truncated NAL payload");
            }
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&hvcc[p..p + nal_len]);
            p += nal_len;
        }
    }
    Ok(out)
}

/// `true` if the underlying error originated from the network layer (socket
/// reset, host unreachable, etc.).
fn is_network_class_error(e: &ffmpeg::Error) -> bool {
    use ffmpeg::Error::*;
    matches!(
        e,
        Other { errno } if matches!(
            *errno,
            // POSIX network errors libavformat surfaces verbatim
            libc::ECONNREFUSED
                | libc::ECONNRESET
                | libc::ECONNABORTED
                | libc::EHOSTUNREACH
                | libc::ENETUNREACH
                | libc::ENETRESET
                | libc::ENOTCONN
                | libc::EPIPE
                | libc::EIO,
        )
    )
}

/// `true` for the "operation timed out" family.
fn is_timeout_error(e: &ffmpeg::Error) -> bool {
    use ffmpeg::Error::*;
    matches!(
        e,
        Other { errno } if matches!(*errno, libc::ETIMEDOUT | libc::EAGAIN)
    )
}

impl FfmpegSource {
    /// Open a file and prepare a decoder for the best video stream.
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_file_with(path, OpenOptions::default())
    }

    /// Open a file with explicit decoder options.
    pub fn open_file_with(path: impl AsRef<Path>, opts: OpenOptions) -> Result<Self> {
        let p = path.as_ref();
        let s = p
            .to_str()
            .ok_or(SourceError::Ffmpeg(ffmpeg::Error::InvalidData))?;
        Self::open_input(s, &NetworkOptions::default(), &opts)
    }

    /// Open any libavformat-supported URL. Accepts `rtsp://`, `rtsps://`,
    /// `http(s)://`, `file://`, and bare paths.
    pub fn open_url(url: &str, network: NetworkOptions) -> Result<Self> {
        Self::open_url_with(url, network, OpenOptions::default())
    }

    /// Open a URL with both network and decoder options.
    pub fn open_url_with(
        url: &str,
        network: NetworkOptions,
        decoder: OpenOptions,
    ) -> Result<Self> {
        Self::open_input(url, &network, &decoder)
    }

    fn open_input(
        input_str: &str,
        network: &NetworkOptions,
        opts: &OpenOptions,
    ) -> Result<Self> {
        // One-time global init. Idempotent across calls.
        ffmpeg::init()?;

        // Build the libavformat option dictionary. Unknown keys are ignored
        // by demuxers that don't recognize them, so it's safe to set
        // RTSP-specific options unconditionally.
        let mut dict = ffmpeg::Dictionary::new();
        dict.set("rtsp_transport", network.transport.as_str());
        // FFmpeg 5+ exposes a microsecond `timeout` on the underlying
        // protocols (tcp / udp / rtsp). We bind it to the open timeout
        // here; the per-packet read deadline lives in `next_packet`.
        // The legacy `stimeout` key is dead in FFmpeg ≥ 5 — verified during
        // step 002.2 by an open against an unroutable host taking 75 s when
        // `stimeout` was set, vs sub-second when `timeout` is set.
        let open_timeout_us = (network.open_timeout_ms as u64).saturating_mul(1_000);
        let timeout_str = open_timeout_us.to_string();
        dict.set("timeout", &timeout_str);

        let input = ffmpeg::format::input_with_dictionary(Path::new(input_str), dict)?;

        let stream = input
            .streams()
            .best(MediaType::Video)
            .ok_or(SourceError::NoVideoStream)?;
        let video_stream_idx = stream.index();

        let codec_params = stream.parameters();
        let codec_id = codec_params.id();
        // Codec allowlist:
        // - H.264: FFmpeg `+export_mvs` side-data path.
        // - HEVC : libde265 + `de265_internals` (vendored). FFmpeg
        //          upstream does not expose HEVC MVs.
        // Other codecs (VP9, AV1, MPEG-4, ...) are rejected up front.
        let backend = match codec_id {
            ffmpeg::codec::Id::H264 => {
                let mut context = ffmpeg::codec::context::Context::from_parameters(codec_params.clone())?;
                // Enable motion-vector export as side data + apply opt-in
                // skip flags. Must be set before opening the decoder.
                // SAFETY: as_mut_ptr returns the live AVCodecContext owned
                // by `context`; we only set scalar fields.
                unsafe {
                    let raw = context.as_mut_ptr();
                    (*raw).flags2 |= ffmpeg_sys_next::AV_CODEC_FLAG2_EXPORT_MVS;
                    if opts.skip_loop_filter_all {
                        (*raw).skip_loop_filter = ffmpeg_sys_next::AVDiscard::AVDISCARD_ALL;
                    }
                    if opts.skip_idct_all {
                        (*raw).skip_idct = ffmpeg_sys_next::AVDiscard::AVDISCARD_ALL;
                    }
                }
                DecoderBackend::H264(context.decoder().video()?)
            }
            ffmpeg::codec::Id::HEVC => {
                let mut dec = hevc::HevcDecoder::new()
                    .map_err(|_| SourceError::Hevc(String::from("libde265 init failed")))?;
                // Push hvcC parameter sets up front for mp4-muxed inputs.
                // Raw .h265 streams have inline VPS/SPS/PPS so extradata
                // is empty there.
                // SAFETY: codec_params alive; extradata pointer + size
                // are read-only fields on AVCodecParameters.
                let extradata: Vec<u8> = unsafe {
                    let raw = codec_params.as_ptr();
                    let p = (*raw).extradata;
                    let n = (*raw).extradata_size as usize;
                    if p.is_null() || n == 0 {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(p, n).to_vec()
                    }
                };
                if !extradata.is_empty() {
                    let annexb = hvcc_extradata_to_annexb(&extradata).map_err(|msg| {
                        SourceError::Hevc(format!("hvcC parse failed: {msg}"))
                    })?;
                    dec.push(&annexb).map_err(|e| {
                        SourceError::Hevc(format!("push hvcC parameter sets: {e}"))
                    })?;
                    dec.push_end_of_nal();
                }
                DecoderBackend::Hevc(dec)
            }
            _ => return Err(SourceError::UnsupportedCodec(codec_id)),
        };

        let frame_rate = stream.avg_frame_rate();
        let time_base = stream.time_base();
        let duration_us = {
            let d = input.duration();
            if d > 0 { Some(d) } else { None }
        };
        let nb = stream.frames();
        let nb_frames_hint = if nb > 0 { Some(nb) } else { None };

        // Width/height: come from the H.264 decoder context, or for HEVC
        // are read from the codec parameters at open time. ffmpeg-next
        // doesn't expose width/height on `Parameters` directly, so we
        // reach into the raw AVCodecParameters.
        let (width, height) = match &backend {
            DecoderBackend::H264(d) => (d.width(), d.height()),
            DecoderBackend::Hevc(_) => {
                // SAFETY: codec_params is alive; raw pointer accesses
                // the immutable AVCodecParameters fields width/height.
                let raw = unsafe { codec_params.as_ptr() };
                let w = unsafe { (*raw).width as u32 };
                let h = unsafe { (*raw).height as u32 };
                (w, h)
            }
        };

        let info = VideoInfo {
            codec: codec_id,
            width,
            height,
            frame_rate: (frame_rate.numerator(), frame_rate.denominator()),
            time_base: (time_base.numerator(), time_base.denominator()),
            duration_us,
            nb_frames_hint,
        };

        Ok(Self {
            input,
            video_stream_idx,
            backend,
            info,
            frame_scratch: VideoFrame::empty(),
            out_packet: MvPacket::empty(),
            drained: false,
            rgb_scaler: None,
            rgb_frame: VideoFrame::empty(),
            saved_input: input_str.to_string(),
            saved_network: network.clone(),
            saved_decoder: *opts,
            is_live: is_live_scheme(input_str),
            reconnects_used: 0,
            stream_time_base: (time_base.numerator(), time_base.denominator()),
            hevc_scratch: Vec::new(),
            last_hevc_more: None,
        })
    }

    /// `true` when the source was opened against a live network protocol
    /// (RTSP, RTMP, SRT, RTP, UDP). For these, EOF from the demuxer is
    /// reported as [`SourceError::Disconnected`] rather than `Ok(None)`.
    pub fn is_live(&self) -> bool {
        self.is_live
    }

    /// Number of automatic reconnect attempts already used.
    pub fn reconnects_used(&self) -> u32 {
        self.reconnects_used
    }

    /// Close the current input + decoder and reopen using the saved URL and
    /// options. Counts against `max_reconnects`.
    fn reconnect(&mut self) -> Result<()> {
        if self.reconnects_used >= self.saved_network.max_reconnects {
            return Err(SourceError::Disconnected);
        }
        let url = self.saved_input.clone();
        let net = self.saved_network.clone();
        let dec = self.saved_decoder;
        let new = Self::open_input(&url, &net, &dec)?;
        let used = self.reconnects_used + 1;
        *self = new;
        self.reconnects_used = used;
        Ok(())
    }

    /// Pull the next decoded packet from the source.
    ///
    /// Returns `Ok(Some(&MvPacket))` for each successfully decoded picture and
    /// `Ok(None)` once the stream is fully drained.
    ///
    /// The borrowed [`MvPacket`] is a view into a buffer reused on the next
    /// call. Clone if you need to keep it.
    pub fn next_packet(&mut self) -> Result<Option<&MvPacket>> {
        match &mut self.backend {
            DecoderBackend::H264(_) => self.next_packet_h264(),
            DecoderBackend::Hevc(_) => self.next_packet_hevc(),
        }
    }

    fn next_packet_h264(&mut self) -> Result<Option<&MvPacket>> {
        loop {
            let dec = match &mut self.backend {
                DecoderBackend::H264(d) => d,
                _ => unreachable!(),
            };
            // 1) Try to receive an already-decoded frame.
            match dec.receive_frame(&mut self.frame_scratch) {
                Ok(()) => {
                    populate_packet(&mut self.out_packet, &self.frame_scratch, &self.info);
                    return Ok(Some(&self.out_packet));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {}
                Err(ffmpeg::Error::Eof) => {
                    if self.is_live {
                        match self.try_reconnect_if_live() {
                            Ok(true) => continue,
                            Ok(false) => return Err(SourceError::Disconnected),
                            Err(e) => return Err(e),
                        }
                    }
                    return Ok(None);
                }
                Err(e) => return Err(self.classify_decoder_error(e)),
            }

            if self.drained {
                if self.is_live {
                    match self.try_reconnect_if_live() {
                        Ok(true) => continue,
                        Ok(false) => return Err(SourceError::Disconnected),
                        Err(e) => return Err(e),
                    }
                }
                return Ok(None);
            }

            let mut next_pkt = ffmpeg::Packet::empty();
            match next_pkt.read(&mut self.input) {
                Ok(()) => {
                    if next_pkt.stream() == self.video_stream_idx {
                        if let DecoderBackend::H264(d) = &mut self.backend {
                            d.send_packet(&next_pkt)?;
                        }
                    } else {
                        continue;
                    }
                }
                Err(ffmpeg::Error::Eof) => {
                    if let DecoderBackend::H264(d) = &mut self.backend {
                        d.send_eof()?;
                    }
                    self.drained = true;
                }
                Err(e) => {
                    if let Some(err) = self.handle_demuxer_error(e) {
                        return Err(err);
                    }
                    // Reconnected — restart the loop on the new backend.
                    continue;
                }
            }
        }
    }

    fn next_packet_hevc(&mut self) -> Result<Option<&MvPacket>> {
        loop {
            // 1) Try to pull a frame, populating in-place if present.
            let got = self.try_pull_hevc_frame()?;
            if let Some(()) = got {
                return Ok(Some(&self.out_packet));
            }

            if self.drained {
                // Final pass: if the previous pull saw `more == false`,
                // there is nothing more to produce.
                let more = self.last_hevc_more.unwrap_or(false);
                if !more {
                    return Ok(None);
                }
            }

            // 2) Need more input. Read next FFmpeg packet, convert from
            // mp4 length-prefix to Annex-B if necessary, push to libde265.
            let mut next_pkt = ffmpeg::Packet::empty();
            match next_pkt.read(&mut self.input) {
                Ok(()) => {
                    if next_pkt.stream() != self.video_stream_idx {
                        continue;
                    }
                    let bytes = match next_pkt.data() {
                        Some(d) => d,
                        None => continue,
                    };
                    let annexb = mp4_to_annexb_if_needed(bytes);
                    if let DecoderBackend::Hevc(d) = &mut self.backend {
                        d.push(&annexb)
                            .map_err(|e| SourceError::Hevc(format!("{e}")))?;
                        // Each FFmpeg packet is one access unit of NALs.
                        d.push_end_of_nal();
                    }
                }
                Err(ffmpeg::Error::Eof) => {
                    if let DecoderBackend::Hevc(d) = &mut self.backend {
                        d.flush()
                            .map_err(|e| SourceError::Hevc(format!("{e}")))?;
                    }
                    self.drained = true;
                }
                Err(e) => {
                    if let Some(err) = self.handle_demuxer_error(e) {
                        return Err(err);
                    }
                    continue;
                }
            }
        }
    }

    /// Single libde265 step. If a frame is produced, fully populate
    /// `self.out_packet` from the borrow before it expires.
    fn try_pull_hevc_frame(&mut self) -> Result<Option<()>> {
        let dec = match &mut self.backend {
            DecoderBackend::Hevc(d) => d,
            _ => unreachable!(),
        };
        let (more, frame_opt) = dec
            .decode_step()
            .map_err(|e| SourceError::Hevc(format!("{e}")))?;
        self.last_hevc_more = Some(more);

        let Some(frame) = frame_opt else {
            return Ok(None);
        };

        let (pb_w, _pb_h, log2u) = frame.pb_layout();
        let pixels_per_unit = 1u32 << log2u;
        self.hevc_scratch.clear();
        self.hevc_scratch.extend(frame.pb_info());

        self.out_packet.clear();
        // libde265 PTS pass-through: we never set PTS on push, so this is
        // 0. v1 leaves ts_us = 0 and lets the user detect HEVC by
        // inspecting `info().codec`.
        self.out_packet.ts_us = 0;
        let any_motion = self.hevc_scratch.iter().any(|c| c.has_motion());
        self.out_packet.frame_type = if any_motion {
            FrameType::P
        } else {
            FrameType::I
        };

        let mb_w = self.info.width.div_ceil(16);
        let mb_h = self.info.height.div_ceil(16);
        self.out_packet.total_mb = mb_w.saturating_mul(mb_h);

        let mut intra_pixels: u64 = 0;
        for (idx, pb) in self.hevc_scratch.iter().enumerate() {
            let cx = ((idx as u32) % pb_w) * pixels_per_unit;
            let cy = ((idx as u32) / pb_w) * pixels_per_unit;
            if pb.ref_poc0 != -1 {
                self.out_packet.mvs.push(MotionVector {
                    w: pixels_per_unit as u8,
                    h: pixels_per_unit as u8,
                    src_x: cx as i16,
                    src_y: cy as i16,
                    dst_x: cx as i16,
                    dst_y: cy as i16,
                    motion_x: pb.mv0_x as i32,
                    motion_y: pb.mv0_y as i32,
                    motion_scale: 4,
                    source: -1,
                });
            } else {
                intra_pixels += (pixels_per_unit as u64) * (pixels_per_unit as u64);
            }
        }

        if matches!(self.out_packet.frame_type, FrameType::I) {
            self.out_packet.intra_count = self.out_packet.total_mb;
        } else {
            let intra_mb = (intra_pixels / 256) as u32;
            self.out_packet.intra_count = intra_mb.min(self.out_packet.total_mb);
        }
        Ok(Some(()))
    }

    /// Map a decoder-side ffmpeg error into our [`SourceError`]. For live
    /// sources, network-class failures are reclassified.
    fn classify_decoder_error(&self, e: ffmpeg::Error) -> SourceError {
        if self.is_live && is_network_class_error(&e) {
            SourceError::Disconnected
        } else {
            SourceError::Ffmpeg(e)
        }
    }

    /// Map a demuxer-side ffmpeg error during `read_packet`. The
    /// timeout-vs-disconnect classification needs both branches because
    /// libavformat surfaces them as different `Error` variants.
    fn classify_demuxer_error(&self, e: ffmpeg::Error) -> SourceError {
        if self.is_live {
            if is_timeout_error(&e) {
                return SourceError::ReadTimeout;
            }
            if is_network_class_error(&e) {
                return SourceError::Disconnected;
            }
        }
        SourceError::Ffmpeg(e)
    }

    /// If a reconnect budget remains, do it and return `Ok(true)`. If the
    /// budget is exhausted, return `Ok(false)` so the caller can surface a
    /// terminal `Disconnected` error.
    fn try_reconnect_if_live(&mut self) -> Result<bool> {
        if !self.is_live {
            return Ok(false);
        }
        if self.reconnects_used >= self.saved_network.max_reconnects {
            return Ok(false);
        }
        self.reconnect()?;
        Ok(true)
    }

    /// Demuxer-error handler that prefers reconnect over surfacing the
    /// failure. Returns `Some(error)` when the caller must give up, or
    /// `None` when a reconnect succeeded and the loop should retry.
    fn handle_demuxer_error(&mut self, e: ffmpeg::Error) -> Option<SourceError> {
        let classified = self.classify_demuxer_error(e);
        let recoverable = matches!(
            classified,
            SourceError::Disconnected | SourceError::ReadTimeout
        );
        if recoverable {
            match self.try_reconnect_if_live() {
                Ok(true) => None,
                Ok(false) => Some(classified),
                Err(e) => Some(e),
            }
        } else {
            Some(classified)
        }
    }

    /// Read-only access to captured metadata.
    pub fn info(&self) -> &VideoInfo {
        &self.info
    }

    /// Index of the selected video stream within the container.
    pub fn video_stream_index(&self) -> usize {
        self.video_stream_idx
    }

    /// Borrow the underlying H.264 FFmpeg decoder, if present. Returns
    /// `None` for HEVC (libde265) sources.
    pub fn h264_decoder(&self) -> Option<&ffmpeg::decoder::Video> {
        match &self.backend {
            DecoderBackend::H264(d) => Some(d),
            DecoderBackend::Hevc(_) => None,
        }
    }

    /// Borrow the underlying input context.
    pub fn input(&self) -> &Input {
        &self.input
    }

    /// Diagnostic: raw pointer to the most recently decoded `AVFrame`.
    ///
    /// Valid only between `next_packet` calls and only if at least one packet
    /// has been returned. Used by tests to inspect pixel planes when
    /// validating decoder skip-flag behavior.
    ///
    /// # Safety
    ///
    /// Caller must not retain the pointer past the next `next_packet` /
    /// `drop` and must not mutate the frame.
    pub fn last_frame_ptr(&self) -> *const ffmpeg_sys_next::AVFrame {
        // SAFETY: borrows the live AVFrame owned by `frame_scratch`. The
        // returned raw pointer is read-only and aliases the borrow's
        // lifetime; caller-side safety contract is documented above.
        unsafe { self.frame_scratch.as_ptr() }
    }

    /// Convert the most recently decoded frame to packed RGB24 and return
    /// a contiguous (height × width × 3) byte buffer.
    ///
    /// Allocates fresh on the first call and reuses the destination frame
    /// across subsequent calls. Layout: `[r, g, b, r, g, b, ...]` row-major,
    /// no padding within a row (we copy out of FFmpeg's strided buffer).
    ///
    /// Returns an error if no frame has been decoded yet, or if scaling fails.
    pub fn last_frame_rgb(&mut self) -> Result<Vec<u8>> {
        let dec = match &self.backend {
            DecoderBackend::H264(d) => d,
            DecoderBackend::Hevc(_) => {
                return Err(SourceError::Hevc(String::from(
                    "RGB decode not yet wired up for HEVC sources",
                )))
            }
        };
        let w = dec.width();
        let h = dec.height();
        if w == 0 || h == 0 {
            return Err(SourceError::Ffmpeg(ffmpeg::Error::InvalidData));
        }
        let src_format = dec.format();
        if src_format == Pixel::None {
            return Err(SourceError::Ffmpeg(ffmpeg::Error::InvalidData));
        }
        let scaler = match &mut self.rgb_scaler {
            Some(s) => s,
            None => {
                let s = Scaler::get(
                    src_format,
                    w,
                    h,
                    Pixel::RGB24,
                    w,
                    h,
                    ScalerFlags::BILINEAR,
                )?;
                self.rgb_scaler = Some(s);
                self.rgb_scaler.as_mut().unwrap()
            }
        };
        scaler.run(&self.frame_scratch, &mut self.rgb_frame)?;

        // Copy out of strided RGB into tightly packed Vec<u8>.
        let stride = self.rgb_frame.stride(0);
        let plane = self.rgb_frame.data(0);
        let row_bytes = (w as usize) * 3;
        let mut out = Vec::with_capacity(row_bytes * h as usize);
        for y in 0..h as usize {
            let off = y * stride;
            out.extend_from_slice(&plane[off..off + row_bytes]);
        }
        Ok(out)
    }
}

/// Map ffmpeg picture::Type to our coarser `FrameType`.
fn map_picture_type(p: PictureType) -> FrameType {
    match p {
        PictureType::I | PictureType::SI => FrameType::I,
        PictureType::P | PictureType::SP => FrameType::P,
        PictureType::B | PictureType::BI => FrameType::B,
        _ => FrameType::Other,
    }
}

/// Convert frame PTS (in stream time_base) to microseconds.
///
/// Returns `0` if PTS is unset (FFmpeg `AV_NOPTS_VALUE`). Negative-on-unknown
/// is documented at the [`MvPacket::ts_us`] level; callers may treat 0 as
/// "near-zero" and rely on monotonic sequence rather than absolute time.
fn pts_to_microseconds(pts: Option<i64>, time_base: (i32, i32)) -> i64 {
    let Some(pts) = pts else { return 0 };
    let (num, den) = time_base;
    if den == 0 {
        return 0;
    }
    // microseconds = pts * num * 1_000_000 / den
    // Use i128 to avoid overflow on large pts.
    let v = pts as i128 * num as i128 * 1_000_000 / den as i128;
    v as i64
}

fn populate_packet(packet: &mut MvPacket, frame: &VideoFrame, info: &VideoInfo) {
    packet.clear();
    packet.frame_type = map_picture_type(frame.kind());
    packet.ts_us = pts_to_microseconds(frame.pts(), info.time_base);
    extract_motion_vectors(packet, frame);
    fill_macroblock_counts(packet, info);
}

/// Compute `total_mb` from frame dimensions and approximate `intra_count`
/// from MV coverage.
///
/// Approximation:
/// - I-frames: every MB is intra → `intra_count = total_mb`.
/// - P/B-frames: sum block areas of forward (`source == -1`) motion vectors.
///   Treat that as inter coverage in pixels; the rest is intra.
///   B-frames with backward-only blocks (rare) get slightly over-counted as
///   intra. Acceptable for trigger heuristics; precise mb_type decoding is a
///   future improvement.
fn fill_macroblock_counts(packet: &mut MvPacket, info: &VideoInfo) {
    let mb_w = info.width.div_ceil(16);
    let mb_h = info.height.div_ceil(16);
    let total_mb = mb_w.saturating_mul(mb_h);
    packet.total_mb = total_mb;

    if matches!(packet.frame_type, FrameType::I) {
        packet.intra_count = total_mb;
        return;
    }

    let mut inter_pixels: u64 = 0;
    for mv in &packet.mvs {
        if mv.source == -1 {
            inter_pixels += (mv.w as u64) * (mv.h as u64);
        }
    }
    let inter_mb = (inter_pixels / 256) as u32;
    packet.intra_count = total_mb.saturating_sub(inter_mb.min(total_mb));
}

/// Read `AV_FRAME_DATA_MOTION_VECTORS` side data from the frame and append
/// each entry into `packet.mvs`.
///
/// FFmpeg attaches MV side data when the decoder has `AV_CODEC_FLAG2_EXPORT_MVS`
/// set. Each entry is an `AVMotionVector`; we copy field-by-field into our
/// stable [`MotionVector`].
fn extract_motion_vectors(packet: &mut MvPacket, frame: &VideoFrame) {
    use ffmpeg_sys_next::{
        av_frame_get_side_data, AVFrameSideDataType::AV_FRAME_DATA_MOTION_VECTORS, AVMotionVector,
    };

    // SAFETY: frame.as_ptr() is a valid AVFrame for the lifetime of `frame`.
    // av_frame_get_side_data returns a borrowed pointer (does not transfer
    // ownership). The returned slice is valid until the frame is reused.
    unsafe {
        let raw_frame = frame.as_ptr();
        let sd = av_frame_get_side_data(raw_frame, AV_FRAME_DATA_MOTION_VECTORS);
        if sd.is_null() {
            return;
        }
        let size = (*sd).size as usize;
        if size == 0 {
            return;
        }
        let count = size / std::mem::size_of::<AVMotionVector>();
        let data = (*sd).data as *const AVMotionVector;
        let mvs: &[AVMotionVector] = std::slice::from_raw_parts(data, count);

        packet.mvs.reserve(count);
        for m in mvs {
            packet.mvs.push(MotionVector {
                w: m.w,
                h: m.h,
                src_x: m.src_x,
                src_y: m.src_y,
                dst_x: m.dst_x,
                dst_y: m.dst_y,
                motion_x: m.motion_x,
                motion_y: m.motion_y,
                motion_scale: m.motion_scale,
                source: m.source as i8,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_file_rejects_missing_path() {
        let result = FfmpegSource::open_file("/nonexistent/path/clip.mp4");
        assert!(result.is_err());
    }

    #[test]
    fn is_live_scheme_recognizes_rtsp_family() {
        assert!(is_live_scheme("rtsp://cam.local/stream"));
        assert!(is_live_scheme("RTSPS://cam.local/stream"));
        assert!(is_live_scheme("rtmp://server/live"));
        assert!(is_live_scheme("srt://10.0.0.1:1234"));
        assert!(is_live_scheme("udp://239.0.0.1:1234"));
        assert!(is_live_scheme("rtp://239.0.0.1:1234"));
    }

    #[test]
    fn is_live_scheme_rejects_files_and_http() {
        assert!(!is_live_scheme("/path/to/file.mp4"));
        assert!(!is_live_scheme("file:///path/to/file.mp4"));
        // HTTP-served static videos do reach a real EOF; we don't treat
        // them as live.
        assert!(!is_live_scheme("http://example.com/video.mp4"));
        assert!(!is_live_scheme("https://example.com/video.mp4"));
    }

    #[test]
    fn timeout_classifier_matches_etimedout_and_eagain() {
        let etimed = ffmpeg::Error::Other { errno: libc::ETIMEDOUT };
        let eagain = ffmpeg::Error::Other { errno: libc::EAGAIN };
        let other = ffmpeg::Error::Eof;
        assert!(is_timeout_error(&etimed));
        assert!(is_timeout_error(&eagain));
        assert!(!is_timeout_error(&other));
    }

    #[test]
    fn network_classifier_matches_econn_family() {
        for errno in [
            libc::ECONNREFUSED,
            libc::ECONNRESET,
            libc::EHOSTUNREACH,
            libc::ENETUNREACH,
        ] {
            assert!(is_network_class_error(&ffmpeg::Error::Other { errno }));
        }
        assert!(!is_network_class_error(&ffmpeg::Error::Eof));
    }
}
