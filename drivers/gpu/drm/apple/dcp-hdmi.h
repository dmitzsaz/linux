/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
#ifndef __APPLE_DCP_HDMI_H__
#define __APPLE_DCP_HDMI_H__

#include <linux/types.h>

struct apple_dcp;

/*
 * Service requests of the DP-to-HDMI converter behind fixed HDMI ports.
 * Every call is a no-op for any other DCP.
 */
int dcp_hdmi_init(struct apple_dcp *dcp);
void dcp_hdmi_enable(struct apple_dcp *dcp);
void dcp_hdmi_disable(struct apple_dcp *dcp);
void dcp_hdmi_hpd_edge(struct apple_dcp *dcp);
u64 dcp_hdmi_attachment_generation(struct apple_dcp *dcp);
void dcp_hdmi_link_configured(struct apple_dcp *dcp, u32 link_rate);

#if IS_ENABLED(CONFIG_KUNIT)
int dcp_hdmi_create(struct apple_dcp *dcp, bool timed_edges);
bool dcp_hdmi_record_hpd_edge(struct apple_dcp *dcp, int level, u64 now);
#endif

#endif /* __APPLE_DCP_HDMI_H__ */
