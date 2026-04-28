//! fovea-mv-stream
//!
//! Source adapters for fovea-mv. Wraps `ffmpeg-next` to feed motion-vector
//! packets into `fovea-mv-core`.
//!
//! Step 2 scope (current): file source — open + decoder init + packet iteration
//! emitting [`MvPacket`] with `frame_type` + `ts_us`. MV extraction
//! (`+export_mvs` side data) lands in step 2.4.

#![warn(missing_docs)]

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

/// FFmpeg-backed file source.
///
/// Opens an `mp4` (or any libavformat-supported container) file, locates the
/// best video stream, and prepares a decoder context.
pub struct FfmpegSource {
    input: Input,
    video_stream_idx: usize,
    decoder: ffmpeg::decoder::Video,
    info: VideoInfo,
    /// Reusable scratch frame; avoids per-packet allocation.
    frame_scratch: VideoFrame,
    /// Reusable packet returned to caller; cleared each iteration.
    out_packet: MvPacket,
    /// `true` once container EOF reached and decoder flushed.
    drained: bool,
    /// Lazy YUV→RGB scaler. Constructed on first `last_frame_rgb()` call.
    rgb_scaler: Option<Scaler>,
    /// Reusable RGB destination frame.
    rgb_frame: VideoFrame,
}

impl FfmpegSource {
    /// Open a file and prepare a decoder for the best video stream.
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_file_with(path, OpenOptions::default())
    }

    /// Open a file with explicit decoder options.
    pub fn open_file_with(path: impl AsRef<Path>, opts: OpenOptions) -> Result<Self> {
        Self::open_input(path.as_ref(), &NetworkOptions::default(), &opts)
    }

    /// Open any libavformat-supported URL. Accepts `rtsp://`, `rtsps://`,
    /// `http(s)://`, `file://`, and bare paths.
    ///
    /// `network` is currently a stub; option fields land in step 002.2.
    pub fn open_url(url: &str, network: NetworkOptions) -> Result<Self> {
        Self::open_url_with(url, network, OpenOptions::default())
    }

    /// Open a URL with both network and decoder options.
    pub fn open_url_with(
        url: &str,
        network: NetworkOptions,
        decoder: OpenOptions,
    ) -> Result<Self> {
        Self::open_input(Path::new(url), &network, &decoder)
    }

    fn open_input(
        input_ref: &Path,
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

        let input = ffmpeg::format::input_with_dictionary(input_ref, dict)?;

        let stream = input
            .streams()
            .best(MediaType::Video)
            .ok_or(SourceError::NoVideoStream)?;
        let video_stream_idx = stream.index();

        let codec_params = stream.parameters();
        let codec_id = codec_params.id();
        // Step 2 only wires up H.264. Other codecs surface explicitly.
        if codec_id != ffmpeg::codec::Id::H264 {
            return Err(SourceError::UnsupportedCodec(codec_id));
        }

        let mut context = ffmpeg::codec::context::Context::from_parameters(codec_params)?;
        // Enable motion-vector export as side data + apply opt-in skip flags.
        // Must be set before opening the decoder (avcodec_open2).
        // SAFETY: as_mut_ptr returns the live AVCodecContext owned by `context`.
        // We only set scalar fields; no aliasing or lifetime extension.
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
        let decoder = context.decoder().video()?;

        let frame_rate = stream.avg_frame_rate();
        let time_base = stream.time_base();
        let duration_us = {
            let d = input.duration();
            if d > 0 { Some(d) } else { None }
        };
        let nb = stream.frames();
        let nb_frames_hint = if nb > 0 { Some(nb) } else { None };

        let info = VideoInfo {
            codec: codec_id,
            width: decoder.width(),
            height: decoder.height(),
            frame_rate: (frame_rate.numerator(), frame_rate.denominator()),
            time_base: (time_base.numerator(), time_base.denominator()),
            duration_us,
            nb_frames_hint,
        };

        Ok(Self {
            input,
            video_stream_idx,
            decoder,
            info,
            frame_scratch: VideoFrame::empty(),
            out_packet: MvPacket::empty(),
            drained: false,
            rgb_scaler: None,
            rgb_frame: VideoFrame::empty(),
        })
    }

    /// Pull the next decoded packet from the source.
    ///
    /// Returns `Ok(Some(&MvPacket))` for each successfully decoded picture and
    /// `Ok(None)` once the stream is fully drained.
    ///
    /// The borrowed [`MvPacket`] is a view into a buffer reused on the next
    /// call. Clone if you need to keep it.
    pub fn next_packet(&mut self) -> Result<Option<&MvPacket>> {
        loop {
            // 1) Try to receive an already-decoded frame.
            match self.decoder.receive_frame(&mut self.frame_scratch) {
                Ok(()) => {
                    populate_packet(&mut self.out_packet, &self.frame_scratch, &self.info);
                    return Ok(Some(&self.out_packet));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                    // Need more input.
                }
                Err(ffmpeg::Error::Eof) => {
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            }

            // 2) Feed more input. After EOF, send a flush (None) once.
            if self.drained {
                return Ok(None);
            }

            let mut next_pkt = ffmpeg::Packet::empty();
            match next_pkt.read(&mut self.input) {
                Ok(()) => {
                    if next_pkt.stream() == self.video_stream_idx {
                        self.decoder.send_packet(&next_pkt)?;
                    } else {
                        // Non-video packet: skip and try reading another.
                        continue;
                    }
                }
                Err(ffmpeg::Error::Eof) => {
                    self.decoder.send_eof()?;
                    self.drained = true;
                }
                Err(e) => return Err(e.into()),
            }
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

    /// Borrow the underlying decoder. Exposed for step 2.3+.
    pub fn decoder(&self) -> &ffmpeg::decoder::Video {
        &self.decoder
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
        let w = self.decoder.width();
        let h = self.decoder.height();
        if w == 0 || h == 0 {
            return Err(SourceError::Ffmpeg(ffmpeg::Error::InvalidData));
        }
        let src_format = self.decoder.format();
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
}
