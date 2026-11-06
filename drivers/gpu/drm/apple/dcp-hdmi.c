// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * HDMI converter service requests on T6030 and T8142 MacBook Pros.
 *
 * T6030 uses its first external coprocessor and ATC3; T8142 uses DCPEXT1
 * and ATC2. Both drive a four-lane DisplayPort PHY into an on-board DP-to-HDMI
 * converter. Power, hotplug and the fixed route work as for the M1/M2 Pro
 * MacBook Pro HDMI port (hdmi-hpd, hdmi-pwren and dp2hdmi-pwren GPIOs, see
 * dcp.c and dcp-fabric.c).
 *
 * Like any DisplayPort sink, the converter asks the source for service by
 * pulsing HPD low for 0.5 to 1 ms (IRQ_HPD). It does that on link status
 * changes and while it trains an HDMI FRL link to the display, which the
 * modes beyond HDMI 2.0 rates need. The DPTX firmware services a request
 * only once it is told that the sink raised one. This file tells it:
 *
 *  - for every short low pulse on the HPD GPIO while the HDMI link is up;
 *  - every 100 ms for a while after the firmware configured an HBR3 link,
 *    the rate those modes use, because not every request of the converter
 *    shows up as a pulse on the GPIO.
 *
 * T6030 and T8142 external coprocessors with a fixed HDMI route use this.
 */

#include <kunit/visibility.h>
#include <linux/atomic.h>
#include <linux/device.h>
#include <linux/gpio/consumer.h>
#include <linux/jiffies.h>
#include <linux/ktime.h>
#include <linux/of.h>
#include <linux/slab.h>
#include <linux/workqueue.h>

#include <drm/drm_mode.h>

#include "afk.h"
#include "dcp-hdmi.h"
#include "dcp-internal.h"
#include "dptxep.h"

/* Longest HPD low time still taken as a service request (DP: at most 1 ms). */
#define DCP_HDMI_IRQ_HPD_MAX_NS		(2 * NSEC_PER_MSEC)
/* Requests are polled this long after an HBR3 link configuration ... */
#define DCP_HDMI_SERVICE_WINDOW_MS	10000
/* ... at this period. */
#define DCP_HDMI_SERVICE_PERIOD_MS	100

/* DPTX remote port call: the sink raised an IRQ_HPD service request. */
#define DPTX_GROUP_HOTPLUG		8
#define DPTX_CMD_SINK_IRQ		9
#define DPTX_SINK_IRQ_PAD		16

struct dcp_hdmi {
	struct apple_dcp *dcp;
	struct delayed_work work;
	/* Hard IRQ context only: time of the last falling HPD edge, or 0. */
	u64 falling_ns;
	/* Confirmed long-low/rising cable changes, separate from IRQ_HPD. */
	atomic64_t attachment_generation;
	/* Short pulses seen and not yet passed on. */
	atomic_t pulses;
	/* Poll requests until this time (jiffies); 0 when not polling. */
	unsigned long poll_until;
	/* The HPD GPIO can be read from hard IRQ context. */
	bool timed_edges;
	/* Work context only. */
	bool announced;
};

static bool dcp_hdmi_link_up(struct apple_dcp *dcp)
{
	/* A Type-C display that borrowed this pipeline has its own HPD. */
	return !READ_ONCE(dcp->crashed) && !READ_ONCE(dcp->active_typec_route) &&
	       READ_ONCE(dcp->dptxport[0].enabled) &&
	       READ_ONCE(dcp->dptxport[0].connected);
}

/*
 * No hpd_mutex: the firmware may be waiting for this request while another
 * DPTX call made under that lock is still outstanding. AFK runs concurrent
 * service calls on one endpoint.
 */
static int dcp_hdmi_sink_irq(struct apple_dcp *dcp)
{
	struct apple_epic_service *service = READ_ONCE(dcp->dptxport[0].service);

	if (!service)
		return -ENODEV;
	return afk_service_call(service, DPTX_GROUP_HOTPLUG, DPTX_CMD_SINK_IRQ,
				NULL, 0, DPTX_SINK_IRQ_PAD, NULL, 0,
				DPTX_SINK_IRQ_PAD);
}

static void dcp_hdmi_work(struct work_struct *work)
{
	struct dcp_hdmi *hdmi = container_of(to_delayed_work(work),
					     struct dcp_hdmi, work);
	struct apple_dcp *dcp = hdmi->dcp;
	unsigned long until = READ_ONCE(hdmi->poll_until);
	bool pulse = atomic_xchg(&hdmi->pulses, 0) > 0;
	bool poll = until && time_before(jiffies, until);
	int ret;

	if (!pulse && !poll)
		return;
	/* The link rate can be set before the connection is marked up. */
	if (!dcp_hdmi_link_up(dcp))
		goto next;

	ret = dcp_hdmi_sink_irq(dcp);
	if (ret) {
		dev_warn_ratelimited(dcp->dev,
				     "HDMI: converter service request not delivered: %d\n",
				     ret);
		/* Don't keep polling a firmware that refuses the request. */
		cmpxchg(&hdmi->poll_until, until, 0);
		return;
	}
	if (!hdmi->announced) {
		hdmi->announced = true;
		dev_info(dcp->dev, "HDMI: forwarding converter service requests (%s)\n",
			 pulse ? "HPD pulse" : "HBR3 link");
	} else {
		dev_dbg(dcp->dev, "HDMI: converter service request (%s)\n",
			pulse ? "HPD pulse" : "poll");
	}

next:
	if (poll)
		mod_delayed_work(system_freezable_wq, &hdmi->work,
				 msecs_to_jiffies(DCP_HDMI_SERVICE_PERIOD_MS));
}

static bool dcp_hdmi_is_irq_hpd(u64 low_ns)
{
	return low_ns <= DCP_HDMI_IRQ_HPD_MAX_NS;
}

/* Hard IRQ state update only: no GPIO, firmware or workqueue operations. */
VISIBLE_IF_KUNIT bool dcp_hdmi_record_hpd_edge(struct apple_dcp *dcp, int level,
					       u64 now)
{
	struct dcp_hdmi *hdmi = dcp->hdmi;
	u64 low;

	if (!hdmi || !hdmi->timed_edges || level < 0)
		return false;

	if (!level) {
		hdmi->falling_ns = now;
		return false;
	}

	/* A rising edge without a falling one: the pulse ended before we read it. */
	low = hdmi->falling_ns ? now - hdmi->falling_ns : 0;
	hdmi->falling_ns = 0;
	if (!dcp_hdmi_is_irq_hpd(low)) {
		atomic64_inc(&hdmi->attachment_generation);
		return false;
	}
	return true;
}

/* No GPIO reads or firmware calls: safe under IOMFB and modes locks. */
u64 dcp_hdmi_attachment_generation(struct apple_dcp *dcp)
{
	return dcp->hdmi ? atomic64_read(&dcp->hdmi->attachment_generation) : 0;
}

/* Hard IRQ context, from the HPD GPIO's edge handler, on every edge. */
void dcp_hdmi_hpd_edge(struct apple_dcp *dcp)
{
	struct dcp_hdmi *hdmi = dcp->hdmi;
	u64 now;

	if (!hdmi || !hdmi->timed_edges)
		return;
	now = ktime_get_ns();
	if (!dcp_hdmi_record_hpd_edge(dcp, gpiod_get_value(dcp->hdmi_hpd), now))
		return;

	atomic_inc(&hdmi->pulses);
	mod_delayed_work(system_freezable_wq, &hdmi->work, 0);
}

/* From DPTX DidChangeLinkConfiguration, with the rate that is now set. */
void dcp_hdmi_link_configured(struct apple_dcp *dcp, u32 link_rate)
{
	struct dcp_hdmi *hdmi = dcp->hdmi;
	unsigned long until;

	if (!hdmi)
		return;
	if (link_rate != LINK_RATE_HBR3) {
		WRITE_ONCE(hdmi->poll_until, 0);
		return;
	}

	until = jiffies + msecs_to_jiffies(DCP_HDMI_SERVICE_WINDOW_MS);
	WRITE_ONCE(hdmi->poll_until, until ?: 1);
	mod_delayed_work(system_freezable_wq, &hdmi->work, 0);
}

void dcp_hdmi_enable(struct apple_dcp *dcp)
{
	if (dcp->hdmi)
		enable_delayed_work(&dcp->hdmi->work);
}

/* Returns with no request in flight; later edges queue nothing. */
void dcp_hdmi_disable(struct apple_dcp *dcp)
{
	if (dcp->hdmi)
		disable_delayed_work_sync(&dcp->hdmi->work);
}

static void dcp_hdmi_release(void *data)
{
	struct dcp_hdmi *hdmi = data;

	/* The HPD IRQ was requested later, so it is already freed. */
	hdmi->dcp->hdmi = NULL;
	disable_delayed_work_sync(&hdmi->work);
}

/*
 * Called from probe once the HPD GPIO is known and before its IRQ is
 * requested, so that the IRQ is released before this state on unbind.
 */
VISIBLE_IF_KUNIT int dcp_hdmi_create(struct apple_dcp *dcp, bool timed_edges)
{
	struct dcp_hdmi *hdmi;
	int ret;

	hdmi = devm_kzalloc(dcp->dev, sizeof(*hdmi), GFP_KERNEL);
	if (!hdmi)
		return -ENOMEM;

	hdmi->dcp = dcp;
	atomic_set(&hdmi->pulses, 0);
	atomic64_set(&hdmi->attachment_generation, 0);
	hdmi->timed_edges = timed_edges;
	INIT_DELAYED_WORK(&hdmi->work, dcp_hdmi_work);
	/* Balanced by dcp_hdmi_enable() once the DPTX endpoint may be used. */
	disable_delayed_work(&hdmi->work);

	ret = devm_add_action_or_reset(dcp->dev, dcp_hdmi_release, hdmi);
	if (ret)
		return ret;

	dcp->hdmi = hdmi;
	return 0;
}

int dcp_hdmi_init(struct apple_dcp *dcp)
{
	int ret;

	if (!dcp->hdmi_hpd ||
	    dcp->fixed_connector_type != DRM_MODE_CONNECTOR_HDMIA ||
	    (!of_device_is_compatible(dcp->dev->of_node, "apple,t6030-dcpext") &&
	     !of_device_is_compatible(dcp->dev->of_node, "apple,t8142-dcpext")))
		return 0;

	ret = dcp_hdmi_create(dcp, !gpiod_cansleep(dcp->hdmi_hpd));
	if (ret)
		return ret;
	if (!dcp->hdmi->timed_edges)
		dev_info(dcp->dev, "HDMI: HPD GPIO can sleep, converter requests are polled only\n");
	return 0;
}
