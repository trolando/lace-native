/*
 * lace_helpers.c — Non-inline wrappers for static inline Lace functions.
 *
 * lace_get_worker(), lace_worker_id(), lace_is_worker(), and lace_rng()
 * are static inline in lace.h and cannot be called directly via FFI.
 * This provides extern wrappers.
 */

#include "lace.h"

lace_worker* lace_get_worker_ext(void) {
    return lace_get_worker();
}

int lace_worker_id_ext(void) {
    return lace_worker_id();
}

int lace_is_worker_ext(void) {
    return lace_is_worker();
}

uint64_t lace_rng_ext(lace_worker *lw) {
    return lace_rng(lw);
}
