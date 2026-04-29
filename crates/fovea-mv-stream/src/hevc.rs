//! HEVC source backed by libde265 (with the fovea-mv `de265_internals`
//! API that exposes prediction-block motion vectors).
//!
//! Used as the codec branch when `FfmpegSource::open_input` detects an
//! HEVC stream. H.264 keeps its FFmpeg `+export_mvs` path (zero-decode
//! cost) — see the rest of `lib.rs`.

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    missing_docs
)]

use std::ffi::CStr;
use std::ptr::NonNull;

use thiserror::Error;

mod ffi {
    include!(concat!(env!("OUT_DIR"), "/libde265_bindings.rs"));
}

/// Decoder-side error.
#[derive(Debug, Error)]
pub enum HevcError {
    #[error("libde265 decoder error: {0}")]
    Decoder(String),
    #[error("decoder allocation failed")]
    Alloc,
}

pub type HevcResult<T> = Result<T, HevcError>;

/// Single prediction-block motion entry, mirroring `de265_PB_info`
/// (raster order in the PB grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PbInfo {
    pub ref_poc0: i16,
    pub ref_poc1: i16,
    pub mv0_x: i16,
    pub mv0_y: i16,
    pub mv1_x: i16,
    pub mv1_y: i16,
}

impl PbInfo {
    /// `true` if at least one direction has a valid reference.
    pub fn has_motion(&self) -> bool {
        self.ref_poc0 != -1 || self.ref_poc1 != -1
    }
}

/// Frame-level coding-block prediction-mode aggregate, mirroring
/// `de265_CB_stats`. Reads PredMode directly from the decoder's
/// `cb_info` (zero decoder-path changes, see
/// `vendor/libde265/libde265/de265_internals.cc`).
///
/// In a P / B slice (`slice_type_first` of 0 or 1), `intra_pixels /
/// total_pixels` measures the fraction of the frame the encoder coded
/// as intra — the direct, non-heuristic version of what the existing
/// HEVC path derives from "no reference frame ⇒ intra".
///
/// `slice_type_first`: 0 = B, 1 = P, 2 = I, -1 = unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CbStats {
    pub total_cells: u32,
    pub intra_cells: u32,
    pub inter_cells: u32,
    pub skip_cells: u32,
    pub intra_pixels: u64,
    pub inter_pixels: u64,
    pub skip_pixels: u64,
    pub total_pixels: u64,
    pub slice_type_first: i32,
}

impl CbStats {
    /// `intra_pixels / total_pixels`, 0.0 when the frame is empty.
    pub fn intra_ratio(&self) -> f32 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.intra_pixels as f32 / self.total_pixels as f32
        }
    }

    /// `skip_pixels / total_pixels`. Encoder said "this region didn't
    /// change at all" — a strong idle signal that the existing
    /// heuristic path can't surface.
    pub fn skip_ratio(&self) -> f32 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.skip_pixels as f32 / self.total_pixels as f32
        }
    }
}

/// Owned libde265 decoder.
pub struct HevcDecoder {
    ctx: NonNull<ffi::de265_decoder_context>,
}

// SAFETY: libde265's decoder context is internally synchronized; we wrap
// it as Send-only (not Sync) since we don't share &HevcDecoder across
// threads.
unsafe impl Send for HevcDecoder {}

impl HevcDecoder {
    /// Spin up a fresh decoder.
    pub fn new() -> HevcResult<Self> {
        // SAFETY: well-formed call to a public C API; returns NULL on alloc
        // failure, which we propagate as HevcError::Alloc.
        let ctx = unsafe { ffi::de265_new_decoder() };
        let ctx = NonNull::new(ctx as *mut ffi::de265_decoder_context).ok_or(HevcError::Alloc)?;
        // SAFETY: handle came from de265_new_decoder; one worker thread is
        // the conservative default for our single-stream consumer.
        unsafe { ffi::de265_start_worker_threads(ctx.as_ptr(), 1) };
        Ok(Self { ctx })
    }

    /// Push raw bitstream bytes to the decoder. May be called repeatedly.
    ///
    /// `pts` is opaque to libde265 — it stores the value verbatim and
    /// returns it via [`DecodedFrame::pts`] when the matching frame
    /// emerges. Callers who don't have a meaningful timestamp can pass
    /// `0`. fovea-mv passes the source's per-packet PTS in
    /// microseconds so [`crate::MvPacket::ts_us`] is populated for
    /// HEVC sources just like it is for H.264.
    pub fn push(&mut self, data: &[u8], pts: i64) -> HevcResult<()> {
        // SAFETY: pointer valid for `len` bytes; libde265 copies internally.
        let err = unsafe {
            ffi::de265_push_data(
                self.ctx.as_ptr(),
                data.as_ptr() as *const _,
                data.len() as _,
                pts,
                std::ptr::null_mut(),
            )
        };
        check(err)
    }

    /// Signal end of input. After this, drain frames with [`Self::decode_step`]
    /// until both the loop and `next_picture` are empty.
    pub fn flush(&mut self) -> HevcResult<()> {
        // SAFETY: handle valid for the lifetime of `self`.
        let err = unsafe { ffi::de265_flush_data(self.ctx.as_ptr()) };
        check(err)
    }

    /// Signal that the buffer just pushed completes a NAL unit. libde265
    /// parses NAL boundaries from Annex-B start codes, but if we hand it
    /// only one NAL with no trailing start code it treats the buffer as
    /// "more bytes coming". For mp4-derived input we know the boundary
    /// after every push and signal it explicitly.
    pub fn push_end_of_nal(&mut self) {
        // SAFETY: returns void; handle valid.
        unsafe { ffi::de265_push_end_of_NAL(self.ctx.as_ptr()) };
    }

    /// Signal end of an access unit (frame).
    pub fn push_end_of_frame(&mut self) {
        // SAFETY: returns void; handle valid.
        unsafe { ffi::de265_push_end_of_frame(self.ctx.as_ptr()) };
    }

    /// One iteration of libde265's "decode/display" loop. Returns the
    /// `more` hint plus an optional picture pointer if one is ready.
    /// Picture pointers are **borrowed** — invalidated by the next
    /// [`Self::decode_step`] call.
    pub fn decode_step(&mut self) -> HevcResult<(bool, Option<DecodedFrame<'_>>)> {
        let mut more: i32 = 0;
        // SAFETY: handle valid; `more` is an out-param scalar.
        let _err = unsafe { ffi::de265_decode(self.ctx.as_ptr(), &mut more as *mut _) };
        // libde265 returns transient errors during normal operation —
        // "waiting for input data", "coded parameter out of range" mid-
        // stream when initialization NALs haven't been processed yet,
        // checksum mismatches when MV side data is what we want, etc.
        // The C-level idiom (and what `dec265.cc` does) is to ignore
        // the err and trust `next_picture` as the source-of-truth for
        // produced frames. We mirror that here.
        // SAFETY: picture borrows from the decoder; lifetime is bounded
        // by the next decode_step call (which may free the picture).
        let pic = unsafe { ffi::de265_get_next_picture(self.ctx.as_ptr()) };
        if pic.is_null() {
            Ok((more != 0, None))
        } else {
            // SAFETY: pic is non-null and valid until the next decode_step.
            let frame = unsafe { DecodedFrame::from_raw(pic) };
            Ok((true, Some(frame)))
        }
    }
}

impl Drop for HevcDecoder {
    fn drop(&mut self) {
        // SAFETY: handle valid until drop runs once.
        unsafe { ffi::de265_free_decoder(self.ctx.as_ptr()) };
    }
}

/// A picture borrowed from the decoder. Holds enough metadata to read
/// MVs without exposing FFI types upward.
pub struct DecodedFrame<'a> {
    raw: *const ffi::de265_image,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> DecodedFrame<'a> {
    /// SAFETY: `raw` must outlive the returned borrow (i.e. caller must
    /// not advance the decoder until the frame is dropped).
    unsafe fn from_raw(raw: *const ffi::de265_image) -> Self {
        Self { raw, _marker: std::marker::PhantomData }
    }

    /// Width of luma plane in pixels.
    pub fn width(&self) -> u32 {
        // SAFETY: raw points to a live de265_image while self exists.
        unsafe { ffi::de265_get_image_width(self.raw, 0) as u32 }
    }

    /// Height of luma plane in pixels.
    pub fn height(&self) -> u32 {
        unsafe { ffi::de265_get_image_height(self.raw, 0) as u32 }
    }

    /// Presentation timestamp (caller-attached on push; 0 by default).
    pub fn pts(&self) -> i64 {
        unsafe { ffi::de265_get_image_PTS(self.raw) }
    }

    /// PB grid size — `(width_in_units, height_in_units, log2_unit_size)`.
    pub fn pb_layout(&self) -> (u32, u32, u32) {
        let mut w: i32 = 0;
        let mut h: i32 = 0;
        let mut log2u: i32 = 0;
        unsafe {
            ffi::de265_internals_get_PB_info_layout(
                self.raw,
                &mut w as *mut _,
                &mut h as *mut _,
                &mut log2u as *mut _,
            );
        }
        (w as u32, h as u32, log2u as u32)
    }

    /// Frame-level CB prediction-mode aggregate. One C call, no
    /// allocation beyond the small return struct.
    pub fn cb_stats(&self) -> CbStats {
        let mut raw: ffi::de265_CB_stats = unsafe { std::mem::zeroed() };
        unsafe {
            ffi::de265_internals_get_CB_stats(self.raw, &mut raw as *mut _);
        }
        CbStats {
            total_cells: raw.total_cells,
            intra_cells: raw.intra_cells,
            inter_cells: raw.inter_cells,
            skip_cells: raw.skip_cells,
            intra_pixels: raw.intra_pixels,
            inter_pixels: raw.inter_pixels,
            skip_pixels: raw.skip_pixels,
            total_pixels: raw.total_pixels,
            slice_type_first: raw.slice_type_first,
        }
    }

    /// Pull all PB-cell motion entries in raster order.
    pub fn pb_info(&self) -> Vec<PbInfo> {
        let (w, h, _log2u) = self.pb_layout();
        let n = (w as usize) * (h as usize);
        let mut buf: Vec<ffi::de265_PB_info> = Vec::with_capacity(n);
        unsafe {
            ffi::de265_internals_get_PB_info(self.raw, buf.as_mut_ptr());
            buf.set_len(n);
        }
        buf.into_iter()
            .map(|c| PbInfo {
                ref_poc0: c.refPOC0,
                ref_poc1: c.refPOC1,
                mv0_x: c.mv0_x,
                mv0_y: c.mv0_y,
                mv1_x: c.mv1_x,
                mv1_y: c.mv1_y,
            })
            .collect()
    }
}

fn check(err: ffi::de265_error) -> HevcResult<()> {
    // libde265 distinguishes hard errors (< DE265_OK) from soft warnings
    // (>= 1000). The standard idiom is `de265_isOK(err)` which is true
    // for both OK and any warning. Warnings during decode (e.g. "decoder
    // stalled" while waiting for more bytes) must not abort the stream.
    let ok = unsafe { ffi::de265_isOK(err) };
    if ok != 0 {
        Ok(())
    } else {
        // SAFETY: de265_get_error_text returns a pointer to static
        // immutable storage owned by libde265.
        let msg = unsafe {
            CStr::from_ptr(ffi::de265_get_error_text(err))
                .to_string_lossy()
                .into_owned()
        };
        Err(HevcError::Decoder(msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_constructs_and_drops() {
        let _dec = HevcDecoder::new().expect("new HevcDecoder");
        // dropping here should not panic / leak
    }
}
