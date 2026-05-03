/*
 * lace_helpers.c — Non-inline wrappers for static inline Lace functions.
 *
 * Several Lace functions are static inline in lace.h and cannot be
 * called directly via FFI. This provides extern wrappers.
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

int lace_is_stolen_task_ext(lace_task *t) {
    return lace_is_stolen_task(t);
}

int lace_is_completed_task_ext(lace_task *t) {
    return lace_is_completed_task(t);
}

void *lace_task_result_ext(lace_task *t) {
    return lace_task_result(t);
}

void lace_make_all_shared_ext(void) {
    lace_make_all_shared();
}

void lace_count_report_ext(void) {
#ifdef LACE_COUNT_EVENTS
    lace_count_report();
#endif
}
