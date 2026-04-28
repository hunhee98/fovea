/*
 * Public C API for fovea-mv: prediction-block motion-vector access on a
 * decoded de265_image. See `de265_internals.h` for the contract; this
 * file provides:
 *
 *   - The `de265_image::internals_get_PB_layout` and
 *     `de265_image::internals_get_PB_info` member implementations.
 *   - The `extern "C"` C-ABI wrappers that Rust / Python bindings call.
 *
 * Distributed under LGPL-3.0 (same as the rest of libde265).
 */

#include "de265_internals.h"
#include "image.h"
#include "slice.h"

void de265_image::internals_get_PB_layout(int *width_in_units,
                                          int *height_in_units,
                                          int *log2_unit_size) const
{
    *width_in_units = pb_info.width_in_units;
    *height_in_units = pb_info.height_in_units;
    *log2_unit_size = pb_info.log2unitSize;
}

void de265_image::internals_get_PB_info(de265_PB_info_t *out) const
{
    const int unit_pixels = 1 << pb_info.log2unitSize;
    const int n = pb_info.width_in_units * pb_info.height_in_units;

    for (int i = 0; i < n; ++i) {
        const PBMotion &mv = pb_info[i];

        // Compute pixel-space position of this PB cell so we can resolve
        // which slice header it belongs to.
        const int xPix = (i % pb_info.width_in_units) * unit_pixels;
        const int yPix = (i / pb_info.width_in_units) * unit_pixels;

        int16_t refPOC0 = -1;
        int16_t refPOC1 = -1;
        const int shdr_idx = get_SliceHeaderIndex(xPix, yPix);
        if (shdr_idx >= 0 && shdr_idx < (int)slices.size()) {
            const slice_segment_header *shdr = slices[shdr_idx];
            if (mv.refIdx[0] >= 0 && mv.refIdx[0] < MAX_NUM_REF_PICS) {
                refPOC0 = (int16_t)shdr->RefPicList_POC[0][mv.refIdx[0]];
            }
            if (mv.refIdx[1] >= 0 && mv.refIdx[1] < MAX_NUM_REF_PICS) {
                refPOC1 = (int16_t)shdr->RefPicList_POC[1][mv.refIdx[1]];
            }
        }

        de265_PB_info_t &dst = out[i];
        if (mv.predFlag[0]) {
            dst.refPOC0 = refPOC0;
            dst.mv0_x = mv.mv[0].x;
            dst.mv0_y = mv.mv[0].y;
        } else {
            dst.refPOC0 = -1;
            dst.mv0_x = 0;
            dst.mv0_y = 0;
        }
        if (mv.predFlag[1]) {
            dst.refPOC1 = refPOC1;
            dst.mv1_x = mv.mv[1].x;
            dst.mv1_y = mv.mv[1].y;
        } else {
            dst.refPOC1 = -1;
            dst.mv1_x = 0;
            dst.mv1_y = 0;
        }
    }
}

extern "C" {

LIBDE265_API void de265_internals_get_PB_info_layout(
    const struct de265_image *img,
    int *width_in_units,
    int *height_in_units,
    int *log2_unit_size)
{
    img->internals_get_PB_layout(width_in_units, height_in_units, log2_unit_size);
}

LIBDE265_API void de265_internals_get_PB_info(
    const struct de265_image *img,
    de265_PB_info *out)
{
    img->internals_get_PB_info(out);
}

} // extern "C"
