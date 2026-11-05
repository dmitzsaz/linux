/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
/* Copyright 2026 Ryan Murray */
#ifndef __APPLE_DCP_REFRESH_H__
#define __APPLE_DCP_REFRESH_H__

#include <linux/types.h>

/* Accept nominal 120 Hz and its 1000/1001 fractional timing. */
static inline bool dcp_refresh_is_promotion(u32 rate)
{
	return rate >= (120U << 16) - (120U << 16) / 500 &&
		rate <= (120U << 16) + (120U << 16) / 500;
}

/* All rates are unsigned 16.16 Hz, including fractional fixed timings. */
static inline bool dcp_refresh_range(u32 rate, u32 minimum, u32 maximum,
				     bool internal, u32 *low, u32 *high)
{
	if (!minimum && !maximum && internal && dcp_refresh_is_promotion(rate)) {
		minimum = 24U << 16;
		maximum = rate;
	}
	/* A partial, inverted or out-of-mode range leaves a fixed timing. */
	if (minimum < (1U << 16) || minimum >= maximum ||
	    maximum > rate || maximum > (65535U << 16))
		return false;
	*low = minimum;
	*high = maximum;
	return true;
}

/*
 * A deadline long past asks for the next permitted refresh; 120 is not Hz.
 * Desktop 26.6 has its own presentation clock. Use an expired deadline for
 * fixed ProMotion on H17 firmware 27 too, as its original host path did,
 * rather than assuming that the ARM counter is in the same clock domain.
 */
static inline u64 dcp_present_deadline(bool have_surface, bool required)
{
	return have_surface && required ? 120 : 0;
}

#endif
