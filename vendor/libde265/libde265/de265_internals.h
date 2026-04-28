/*
 * Public C API for fovea-mv: read motion-vector data from a decoded
 * de265_image. Adapted from Christian Feldmann's analyzer fork
 * (https://github.com/ChristianFeldmann/libde265, branch `internal`,
 * commit 26b3b25, 2018-10-07), reduced to the prediction-block MV
 * subset that fovea-mv uses.
 *
 * The MV data itself is already populated by upstream libde265 during
 * normal HEVC decode (see motion.cc, de265_image::set_mv_info). This
 * header only exposes a public accessor — no decode-path changes.
 *
 * Distributed under the same LGPL-3.0 terms as the rest of libde265.
 */

#ifndef DE265_INTERNALS_H
#define DE265_INTERNALS_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>

#include "de265.h"

/// One prediction-block worth of motion data.
///
/// `refPOC0 == -1` means "no forward (L0) prediction for this PB".
/// `refPOC1 == -1` means "no backward (L1) prediction".
/// MV components are in quarter-pel units (1/4-pixel).
typedef struct de265_PB_info_t {
    int16_t refPOC0;
    int16_t refPOC1;
    int16_t mv0_x;
    int16_t mv0_y;
    int16_t mv1_x;
    int16_t mv1_y;
} de265_PB_info;

/// Layout of the PB grid for a decoded image.
///
/// Each cell covers `(1 << log2_unit_size)` pixels per side. Total cells
/// = `width_in_units * height_in_units`.
LIBDE265_API void de265_internals_get_PB_info_layout(
    const struct de265_image *img,
    int *width_in_units,
    int *height_in_units,
    int *log2_unit_size);

/// Fill `out` with one [`de265_PB_info`] per PB cell, raster-scan order.
///
/// `out` must have capacity for at least `width_in_units * height_in_units`
/// entries (call `de265_internals_get_PB_info_layout` first).
LIBDE265_API void de265_internals_get_PB_info(
    const struct de265_image *img,
    de265_PB_info *out);

/// Frame-level coding-block prediction-mode statistics.
///
/// Aggregates over every coding-block cell in the decoded image. The
/// "weighted by area" fields count the number of luma-pixel cells
/// covered by the corresponding mode (`intra_pixels` is the sum, over
/// every cell whose `PredMode == MODE_INTRA`, of `(1 << 2*log2CbSize)`
/// — i.e. the number of luma pixels that block represents). This lets
/// callers compute "intra coverage ratio = intra_pixels / total_pixels"
/// without needing a per-cell layout.
///
/// In a P-slice or B-slice, `intra_pixels / total_pixels` measures the
/// fraction of the frame for which the encoder gave up motion
/// prediction and re-coded as intra. This is a low-cost proxy for "new
/// content" / "scene change" / "lighting change" / "smoke" without
/// access to the residual coefficients themselves.
///
/// `slice_type_first` is the slice_type of the first slice in the
/// image (0 = B, 1 = P, 2 = I). If the image is a single I-slice every
/// cell is trivially intra and the ratio is uninformative; callers
/// should gate on `slice_type_first != 2`.
typedef struct de265_CB_stats_t {
    uint32_t total_cells;
    uint32_t intra_cells;
    uint32_t inter_cells;
    uint32_t skip_cells;
    uint64_t intra_pixels;
    uint64_t inter_pixels;
    uint64_t skip_pixels;
    uint64_t total_pixels;
    int32_t  slice_type_first;
} de265_CB_stats;

/// Fill `out` with the frame-level CB prediction-mode aggregate.
///
/// O(width_in_units * height_in_units). Reads only the already-decoded
/// `cb_info` array; does not touch residual coefficients or the pixel
/// buffer. Safe to call from any thread once the image is fully decoded.
LIBDE265_API void de265_internals_get_CB_stats(
    const struct de265_image *img,
    de265_CB_stats *out);

#ifdef __cplusplus
}
#endif

#endif // DE265_INTERNALS_H
