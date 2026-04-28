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

#ifdef __cplusplus
}
#endif

#endif // DE265_INTERNALS_H
