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

void de265_image::internals_get_CB_stats(de265_CB_stats_t *out) const
{
    out->total_cells   = 0;
    out->intra_cells   = 0;
    out->inter_cells   = 0;
    out->skip_cells    = 0;
    out->intra_pixels  = 0;
    out->inter_pixels  = 0;
    out->skip_pixels   = 0;
    out->total_pixels  = 0;
    out->slice_type_first = -1;

    if (!slices.empty() && slices[0] != nullptr) {
        out->slice_type_first = (int32_t)slices[0]->slice_type;
    }

    const int n = cb_info.width_in_units * cb_info.height_in_units;
    for (int i = 0; i < n; ++i) {
        const CB_ref_info &cb = cb_info[i];

        // log2CbSize == 0 marks an unset / partial cell (see set_log2CbSize
        // comment) — skip so corrupted streams don't poison the aggregate.
        if (cb.log2CbSize == 0) continue;

        // Each CB cell covers (1 << log2unitSize) luma pixels per side; a
        // CU of size (1 << log2CbSize) spans (1 << (log2CbSize - log2unitSize))
        // such cells per side. We iterate at cell granularity (1 cell per
        // index) and weight pixel counts by the cell's own footprint, so
        // the per-cell pixel contribution is just (1 << 2*log2unitSize).
        const uint64_t cell_pixels =
            (uint64_t)1 << (2 * cb_info.log2unitSize);

        out->total_cells  += 1;
        out->total_pixels += cell_pixels;

        switch (cb.PredMode) {
            case MODE_INTRA:
                out->intra_cells  += 1;
                out->intra_pixels += cell_pixels;
                break;
            case MODE_INTER:
                out->inter_cells  += 1;
                out->inter_pixels += cell_pixels;
                break;
            case MODE_SKIP:
                out->skip_cells   += 1;
                out->skip_pixels  += cell_pixels;
                break;
            default:
                // Defensive: unknown values get counted in totals only.
                break;
        }
    }
}

void de265_image::internals_get_TU_stats(de265_TU_stats_t *out) const
{
    out->total_cells   = 0;
    out->nonzero_cells = 0;
    out->total_pixels  = 0;
    out->nonzero_pixels = 0;

    const uint64_t cell_pixels =
        (uint64_t)1 << (2 * tu_info.log2unitSize);
    const int n = tu_info.width_in_units * tu_info.height_in_units;
    for (int i = 0; i < n; ++i) {
        out->total_cells  += 1;
        out->total_pixels += cell_pixels;
        // libde265 sets TU_FLAG_NONZERO_COEFF in tu_info[idx] from
        // `set_nonzero_coefficient` during slice decode whenever a
        // transform unit carries at least one non-zero residual
        // coefficient (i.e. cbf_luma == 1, or chroma equivalents
        // contributed to a nonzero coded block).
        if (tu_info[i] & TU_FLAG_NONZERO_COEFF) {
            out->nonzero_cells  += 1;
            out->nonzero_pixels += cell_pixels;
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

LIBDE265_API void de265_internals_get_CB_stats(
    const struct de265_image *img,
    de265_CB_stats *out)
{
    img->internals_get_CB_stats(out);
}

LIBDE265_API void de265_internals_get_TU_stats(
    const struct de265_image *img,
    de265_TU_stats *out)
{
    img->internals_get_TU_stats(out);
}

} // extern "C"
