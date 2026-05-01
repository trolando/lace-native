/*
 * lace_helpers.c — Non-inline wrappers for static inline Lace functions.
 *
 * lace_get_worker() and lace_worker_id() are static inline in lace.h
 * and cannot be called directly via FFI. This provides extern wrappers.
 */

#include "lace.h"

lace_worker* lace_get_worker_ext(void) {
    return lace_get_worker();
}

int lace_worker_id_ext(void) {
    return lace_worker_id();
}
