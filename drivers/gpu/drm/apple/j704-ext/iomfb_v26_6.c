// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * J613 (M3 Air 13 inch) experimental internal display: macOS 26.6.2 (25G83) IOMFB over DCPLink.
 * J615 uses its own panel size and requires the experimental chosen switch.
 *
 * Copyright The Asahi Linux Contributors
 * Based on the M3 Pro DCP lab client by Eryk Wieliczko.
 *
 * iBoot leaves the DCP running, scanning out the boot framebuffer. This
 * variant never boots, stops or powers the coprocessor down. In order:
 *
 *  probe  the firmware must be the board's qualified image, the boot loader
 *         must have handed the display over (apple,j613-25g83-mapping-handoff, or the
 *         board's own marker), and the PMP must be running and have
 *         acknowledged the display request;
 *  bind   the DCP CPU must still be running; its RTKit session is woken;
 *  start  DCPLink is brought up, then the start signal (A401) and the first
 *         client open (A457) run with every callback answered in the
 *         kernel; the panel mode is taken from PreferredTimingElements;
 *  KMS    only after all of that does drm/apple remove the boot framebuffer
 *         driver and register the DRM device.
 *
 * Any failure before KMS leaves the boot framebuffer in place. Everything the
 * firmware has seen (the RTKit session, the RPC memory, the buffers it
 * allocated, the scanout buffers of failed swaps) is kept until reboot, and
 * the module cannot be unloaded once RTKit is up.
 *
 * Scanout is one plane, XRGB8888 or ARGB8888, linear, exactly the mode size.
 * Each flush is one swap, completed when the firmware reports that swap id,
 * and then the page-flip event is sent. The notch rows (apple,notch-height,
 * unless appledrm.t6030_show_notch=1) are outside the mode and stay black.
 */

#include <linux/backlight.h>
#include <linux/delay.h>
#include <linux/debugfs.h>
#include <linux/device.h>
#include <linux/dma-mapping.h>
#include <linux/iommu.h>
#include <linux/ktime.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/of_address.h>
#include <linux/of_device.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/scatterlist.h>
#include <linux/sizes.h>
#include <linux/slab.h>
#include <linux/soc/apple/rtkit.h>
#include <linux/unaligned.h>
#include <linux/wait.h>
#include <linux/workqueue.h>

#include <drm/drm_atomic.h>
#include <drm/drm_color_mgmt.h>
#include <drm/drm_fb_dma_helper.h>
#include <drm/drm_fourcc.h>
#include <drm/drm_framebuffer.h>
#include <drm/drm_gem_dma_helper.h>
#include <drm/drm_print.h>
#include <drm/drm_vblank.h>

#include "dcp.h"
#include "dcp-internal.h"
#include "dcp-lifecycle.h"
#include "iomfb_internal.h"
#include "iomfb_v26_6.h"
#include "j613-25g83.h"
#include "iomfb_v26_6_link.h"
#include "iomfb_v26_6_swap.h"
#include "parser.h"

#define A(n) DCP_V26_TAG('A', n)
#define D(n) DCP_V26_TAG('D', n)

/* Exact image from the same-boot proxy observation. No 14.x ABI alias. */
#define DCP_V26_J613_FIRMWARE_UUID "C042E95C-B9D8-3F0E-94B3-582A08AA6FDD"

#define DCP_V26_CPU_CONTROL	0x44
#define DCP_V26_CPU_STATUS	0x48
#define DCP_V26_MAX_PROPERTIES	256
/* SDK 27 publishes many more properties (D413..D422, D550..D570) than 25G83. */
#define DCP_V26_MAX_RAW		160
#define DCP_V26_MAX_RAW_BYTES	SZ_8M
#define DCP_V26_MAX_BUFFERS	64
#define DCP_V26_MAX_BUFFER	SZ_16M
#define DCP_V26_MAX_BUFFER_BYTES SZ_64M
#define DCP_V26_MAX_PUBLISHED	128
/* IOMFB layer of the primary plane. */
#define DCP_V26_LAYER		0
#define DCP_V26_BLACK		0xff000000
/* Swaps kept for the present history, and the swap-complete payload size. */
#define DCP_V26_HISTORY		256
#define DCP_V26_D590_SIZE	0x730

/* SDR user maximum of the J613/25G83 laptop panels; the DT gives the panel's own. */
/* Reported until the first request; the firmware keeps its own until then. */
#define DCP_V26_DEFAULT_NITS	140
/* How long a brightness request waits for a flip to carry it. */
#define DCP_V26_BL_FLIP_WAIT	msecs_to_jiffies(25)

/* Fail closed outside the single qualified board and firmware identity. */
struct dcp_v26_board {
	/* In log messages. */
	const char *name;
	const char *dcp_compatible;
	/* The machine compatible; NULL for any machine with this DCP. */
	const char *machine;
	/* The firmware image whose layouts were checked for this board. */
	const char *firmware_uuid;
	/* Set to 1 by the boot loader on the DCP, display and PIODMA nodes. */
	const char *handoff;
	/* Backlight maximum in nits; the DT maximum is capped to it. */
	u32 max_nits;
	/* Native panel size, notch rows included; 0: any, from the boot framebuffer. */
	u32 panel_width, panel_height;
	/* The panel may have a 120 Hz variable-refresh (ProMotion) timing. */
	bool promotion;
	/* If set and true, start without apple,pmp-report (not measured). */
	const bool *without_pmp;
	/*
	 * SDK 27 (macOS 27 DCP firmware, J704): its own callback table, A408
	 * swaps and start sequence (dcp_v27_callback()), not the 25G83 ones.
	 */
	bool sdk27;
	/*
	 * An external DCP (dcpext) of the same firmware: no panel, no boot
	 * framebuffer, no backlight; the display may be absent.
	 */
	bool external;
	/* The compatible of its display subsystem node. */
	const char *display_compatible;
};

static const struct dcp_v26_board dcp_v26_board_j613 = {
	.name = "J613/25G83",
	.dcp_compatible = "apple,t8122-dcp",
	.machine = "apple,j613",
	.firmware_uuid = DCP_V26_J613_FIRMWARE_UUID,
	.handoff = "apple,j613-25g83-mapping-handoff",
	.max_nits = 525,
	.panel_width = 2560,
	.panel_height = 1664,
};
/* Experimental J615 panel: own 2880x1864 geometry and DT brightness cap. */
static const struct dcp_v26_board dcp_v26_board_j615 = {
	.name = "J615/25G83 (experimental)",
	.dcp_compatible = "apple,t8122-dcp",
	.machine = "apple,j615",
	.firmware_uuid = DCP_V26_J613_FIRMWARE_UUID,
	.handoff = "apple,j613-25g83-mapping-handoff",
	.max_nits = 525,
	.panel_width = 2880,
	.panel_height = 1864,
};
/*
 * J704 (MacBook Pro 14, M5): the macOS 27.0 DCP firmware iBoot left running.
 * Its DCPLink READY carries the image hash 0x15a5c96b; nothing else is
 * accepted. The 3024x1964 panel has a 74-row notch, which iBoot hides.
 */
static const struct dcp_v26_board dcp_v26_board_j704 = {
	.name = "J704/27.0",
	.dcp_compatible = "apple,t8142-dcp-v27",
	.machine = "apple,j704",
	.handoff = "apple,j704-mapping-handoff",
	.max_nits = 600,
	.panel_width = 3024,
	.panel_height = 1964,
	.promotion = true,
	.sdk27 = true,
	.display_compatible = "apple,t8142-display-subsystem",
};
/*
 * J704 external DCP (dcpext1, USB-C): the same macOS 27.0 DCP image iBoot also
 * started for the external pipe (identical UUID and DCPLink READY hash).
 */
static const struct dcp_v26_board dcp_v26_board_j704_ext = {
	.name = "J704-ext/27.0",
	.dcp_compatible = "apple,t8142-dcpext-v27",
	.machine = "apple,j704",
	.handoff = "apple,j704-mapping-handoff",
	.sdk27 = true,
	.external = true,
	.display_compatible = "apple,t8142-ext-display-subsystem",
};
static const struct dcp_v26_board *const dcp_v26_boards[] = {
	&dcp_v26_board_j613,
	&dcp_v26_board_j615,
	&dcp_v26_board_j704,
	&dcp_v26_board_j704_ext,
};

#define DCP_V27_READY_MSG	0x004015a5c96b0101ULL

struct dcp_v26_property {
	u32 service;
	char key[64];
	u64 value;
};

struct dcp_v26_raw {
	u32 service;
	char key[64];
	void *data;
	u32 size;
};

/* One swap's timeline, CLOCK_MONOTONIC; 0 where it did not get that far. */
struct dcp_v26_present_record {
	ktime_t start;		/* swap start sent */
	ktime_t submitted;	/* swap start answered, swap sent */
	ktime_t replied;	/* swap reply received */
	ktime_t completed;	/* swap-complete message received */
	u32 id;
	u8 surface;
	u8 timed;
};

/* The last publication of a runtime property: its id, then 12 bytes. */
struct dcp_v26_published {
	u32 id;
	u8 data[12];
	u64 count;
};

struct dcp_v26_buffer {
	void *cpu;
	dma_addr_t iova;
	phys_addr_t phys;
	size_t size;
	bool retired;
	bool piodma_mapped;
};

static void dcp_v26_ext_hotplug(struct work_struct *work);

struct apple_dcp_v26 {
	struct device *dev;
	const struct dcp_v26_board *board;
	/* Cleared when KMS unbinds; the firmware session outlives it. */
	struct apple_dcp *dcp;
	struct apple_rtkit *rtk;
	struct dcp_v26_link link;

	/* Owns the RPC stream: start, swaps and idle callbacks. */
	struct mutex lock;
	bool ctm_valid;
	u64 ctm_calls;
	u32 ctm_status, ctm_get_status;
	u64 ctm_readback[9];
	struct work_struct idle_work;
	/* J704 external: D575 hotplug -> modes + connector state (dcp_v26_ext_hotplug()). */
	struct work_struct ext_hotplug_work;
	bool ext_hotplug_connected;
	/*
	 * J704 external: the firmware pipe takes swaps (set after A413 succeeds,
	 * cleared at unplug). A swap to a pipe that is off is never answered, and
	 * the timed-out call would end the session.
	 */
	bool ext_pipe_on;
	/* J704 external: the timing/color pair last set with A413 (under lock). */
	u32 ext_timing_id;
	u32 ext_color_id;
	bool failed;
	bool started;

	/* Boot framebuffer and native panel timing (notch rows included). */
	u32 stride;
	u32 fb_width, fb_height;
	u32 panel_width, panel_height;
	/* The panel's notch rows (apple,notch-height), hidden or not. */
	u32 notch_rows;
	u64 clock_rate;
	/* The mode is a variable-refresh (ProMotion) timing. */
	bool vrr;
	/*
	 * Set by the modeset: the CRTC shows the 120 Hz ProMotion mode, whose
	 * swaps carry timing words, and its refresh (for debugfs).
	 */
	bool timed;
	u32 refresh;

	struct dcp_v26_property properties[DCP_V26_MAX_PROPERTIES];
	u32 property_count;
	struct dcp_v26_raw raw[DCP_V26_MAX_RAW];
	u32 raw_count, raw_bytes;
	void *chunk;
	u32 chunk_size, chunk_offset;
	u64 analytics;
	u8 analytics_last[4164];
	struct dcp_v26_published published[DCP_V26_MAX_PUBLISHED];
	u32 published_count;
	u64 publications;

	struct dcp_v26_buffer buffers[DCP_V26_MAX_BUFFERS];
	u32 buffer_count;
	u64 buffer_bytes;
	struct platform_device *piodma;
	struct iommu_domain *piodma_domain;

	/* Scanned out until the next swap completes. */
	struct drm_framebuffer *active_fb;
	u64 swaps;
	u64 swap_ns_max;

	/* The last swaps and the last two swap-complete payloads, for debugfs. */
	struct dcp_v26_present_record history[DCP_V26_HISTORY];
	u64 history_count;
	u8 d590[2][DCP_V26_D590_SIZE];
	ktime_t d590_time[2];
	u64 d590_count;

	/* SDK 27: PMGR bandwidth scratch and doorbell (D003), and logged callbacks. */
	u64 bw_scratch, bw_doorbell;
	DECLARE_BITMAP(v27_logged, 1000);

	/* Panel brightness, carried by swaps. Under the lock. */
	u32 bl_nits;		/* requested */
	u32 bl_sent;		/* carried by the last brightness swap */
	u64 bl_seq;		/* bumped by every change of the target */
	u64 bl_swaps;
	bool bl_owned;		/* requested at least once: the driver sets it */
	bool bl_off;		/* the CRTC is off: the target is 0 */
	bool bl_pending;	/* the target has not been carried yet */
	bool bl_queued;		/* the backlight device is being registered */
	bool bl_refused;	/* SDK 27: the firmware refused a brightness swap */
	wait_queue_head_t bl_wait;

	/* The backlight device, registered after the first frame. */
	struct mutex bl_lock;
	struct work_struct bl_register_work;
	struct backlight_device *bl_dev;
	bool bl_detached;
	u32 bl_max;
};

/* One DCP session per boot: RTKit is never started twice. */
static bool dcp_v26_session;

static bool j613_25g83_swap_timestamps = true;
module_param(j613_25g83_swap_timestamps, bool, 0644);
MODULE_PARM_DESC(j613_25g83_swap_timestamps,
		 "J613/25G83: time-stamp swaps on a 120 Hz variable-refresh panel so they are shown at 120 Hz, not 60 Hz (default: on)");

static unsigned int j613_25g83_refresh = 120;
module_param(j613_25g83_refresh, uint, 0444);
MODULE_PARM_DESC(j613_25g83_refresh,
		 "J613/25G83: preferred refresh of the built-in display: 120 (variable refresh, ProMotion) or 60 (locked); both modes stay available (default: 120)");

static bool j613_25g83_flip_timestamp = true;
module_param(j613_25g83_flip_timestamp, bool, 0644);
MODULE_PARM_DESC(j613_25g83_flip_timestamp,
		 "J613/25G83: stamp page-flip events with the arrival of the firmware's swap-complete message; off: when the flush ends, less 1 ms (default: on)");

static int dcp_v26_callback(void *cookie, u32 tag, const void *input, u32 in_size,
			    void *output, u32 out_size);
static int dcp_v27_callback(struct apple_dcp_v26 *v26, u32 tag, const u8 *in, u32 in_size,
			    u8 *out, u32 out_size);

static unsigned int j704_dcp_layer = 2;
module_param(j704_dcp_layer, uint, 0444);
MODULE_PARM_DESC(j704_dcp_layer,
		 "J704/27.0: IOMFB layer of the primary plane, 0..3 (default: 2, as on J813)");

static bool j704_dcp_boot_chain = true;
module_param(j704_dcp_boot_chain, bool, 0444);
MODULE_PARM_DESC(j704_dcp_boot_chain,
		 "J704/27.0: answer start_hardware_boot (D120) with the 25G83 chain at its SDK-27 numbers (default: on)");

static bool j704_dcp_client_open = true;
module_param(j704_dcp_client_open, bool, 0444);
MODULE_PARM_DESC(j704_dcp_client_open,
		 "J704/27.0: send first_client_open (A466) after the start signal (default: on)");

static bool j704_dcp_service_match = true;
module_param(j704_dcp_service_match, bool, 0444);
MODULE_PARM_DESC(j704_dcp_service_match,
		 "J704/27.0: answer the PMU/backlight service matches (D206/D207) with A136/A137 (default: on)");
static void dcp_v26_backlight_register(struct work_struct *work);
static void dcp_v26_backlight_detach(struct apple_dcp_v26 *v26);

/* Called with the lock held, or from a callback. */
static int dcp_v26_call(struct apple_dcp_v26 *v26, u32 tag, const void *in, u32 in_size,
			void *out, u32 out_size, u32 completion)
{
	int ret;

	if (v26->failed)
		return -EIO;
	dev_dbg(v26->dev, "call %#x in %u out %u\n", tag, in_size, out_size);
	ret = dcp_v26_link_call(&v26->link, tag, in, in_size, out, out_size, completion,
				dcp_v26_callback, v26);
	/*
	 * A refused swap leaves the link intact; its caller decides. So does an
	 * external swap that was answered but never completed (unplugged display).
	 */
	if (ret && ret != -EREMOTEIO && !(ret == -ENOLINK && v26->board->external)) {
		v26->failed = true;
		dev_err(v26->dev, "DCP call %#x failed: %d; recovery requires a reboot\n",
			tag, ret);
	}
	return ret;
}

/* A call with an optional u32 input of 1 and an optional u32 result to check. */
static int dcp_v26_simple_call(struct apple_dcp_v26 *v26, u32 tag, bool input, bool reply,
			       bool expected)
{
	__le32 one = cpu_to_le32(1), result = 0;
	int ret;

	ret = dcp_v26_call(v26, tag, input ? &one : NULL, input ? 4 : 0,
			   reply ? &result : NULL, reply ? 4 : 0, 0);
	if (!ret && reply && le32_to_cpu(result) != expected)
		return -EPROTO;
	return ret;
}

static struct dcp_v26_property *dcp_v26_property(struct apple_dcp_v26 *v26, u32 service,
						 const u8 *key, bool create)
{
	struct dcp_v26_property *p;
	u32 i;

	if (!memchr(key, 0, 64))
		return ERR_PTR(-EINVAL);
	for (i = 0; i < v26->property_count; i++) {
		p = &v26->properties[i];
		if (p->service == service && !strcmp(p->key, key))
			return p;
	}
	if (!create)
		return NULL;
	if (v26->property_count == DCP_V26_MAX_PROPERTIES)
		return ERR_PTR(-ENOSPC);
	p = &v26->properties[v26->property_count++];
	p->service = service;
	strscpy(p->key, key, sizeof(p->key));
	return p;
}

static int dcp_v26_raw_property(struct apple_dcp_v26 *v26, u32 service, const u8 *key,
				const void *data, u32 size)
{
	u32 i, old_size = 0;
	void *copy;

	if (!memchr(key, 0, 64) || size > SZ_1M)
		return -EINVAL;
	for (i = 0; i < v26->raw_count; i++)
		if (v26->raw[i].service == service && !strcmp(v26->raw[i].key, key))
			break;
	if (i == DCP_V26_MAX_RAW)
		return -ENOSPC;
	if (i < v26->raw_count)
		old_size = v26->raw[i].size;
	if (v26->raw_bytes - old_size + size > DCP_V26_MAX_RAW_BYTES)
		return -ENOSPC;
	copy = kvmemdup(data, size ?: 1, GFP_KERNEL);
	if (!copy)
		return -ENOMEM;
	if (i == v26->raw_count)
		v26->raw_count++;
	kvfree(v26->raw[i].data);
	v26->raw[i].service = service;
	strscpy(v26->raw[i].key, key, sizeof(v26->raw[i].key));
	v26->raw[i].data = copy;
	v26->raw[i].size = size;
	v26->raw_bytes += size - old_size;
	return 0;
}

/* The firmware's allocate-buffer callback wants a physically contiguous buffer. */
static int dcp_v26_alloc(struct apple_dcp_v26 *v26, u64 request, u32 *id)
{
	struct dcp_v26_buffer *buf;
	struct sg_table table;
	dma_addr_t iova;
	size_t size;
	void *cpu;
	int ret;

	if (!request || request > DCP_V26_MAX_BUFFER)
		return -EINVAL;
	size = ALIGN(request, SZ_16K);
	if (v26->buffer_count == DCP_V26_MAX_BUFFERS ||
	    v26->buffer_bytes + size > DCP_V26_MAX_BUFFER_BYTES)
		return -ENOSPC;
	cpu = dma_alloc_attrs(v26->dev, size, &iova, GFP_KERNEL, DMA_ATTR_FORCE_CONTIGUOUS);
	if (!cpu)
		return -ENOMEM;
	ret = dma_get_sgtable_attrs(v26->dev, &table, cpu, iova, size,
				    DMA_ATTR_FORCE_CONTIGUOUS);
	if (ret)
		goto free;
	if (table.orig_nents != 1 || table.sgl->length < size) {
		sg_free_table(&table);
		ret = -ERANGE;
		goto free;
	}
	buf = &v26->buffers[v26->buffer_count];
	buf->phys = sg_phys(table.sgl);
	sg_free_table(&table);
	memset(cpu, 0, size);
	dma_wmb();
	buf->cpu = cpu;
	buf->iova = iova;
	buf->size = size;
	v26->buffer_bytes += size;
	/* The firmware takes id 0 as a failure. */
	*id = ++v26->buffer_count;
	dev_dbg(v26->dev, "DCP buffer %u: %zu bytes at %pad\n", *id, size, &iova);
	return 0;
free:
	dma_free_attrs(v26->dev, size, cpu, iova, DMA_ATTR_FORCE_CONTIGUOUS);
	return ret;
}

/* Maps a firmware buffer at the same address for the PIODMA stream. */
static int dcp_v26_map_piodma(struct apple_dcp_v26 *v26, u32 id)
{
	struct dcp_v26_buffer *buf = &v26->buffers[id - 1];
	struct device_node *node;
	size_t offset;
	u32 marker;
	int ret;

	if (buf->retired)
		return -EINVAL;
	if (buf->piodma_mapped)
		return 0;
	if (!v26->piodma) {
		node = of_get_child_by_name(v26->dev->of_node, "piodma");
		if (!node || !of_device_is_available(node) ||
		    of_property_read_u32(node, v26->board->handoff, &marker) || marker != 1) {
			of_node_put(node);
			return -ENODEV;
		}
		v26->piodma = of_platform_device_create(node, NULL, v26->dev);
		if (!v26->piodma) {
			of_node_put(node);
			return -ENOMEM;
		}
		ret = dma_set_mask_and_coherent(&v26->piodma->dev, DMA_BIT_MASK(42));
		if (!ret)
			ret = of_dma_configure(&v26->piodma->dev, node, true);
		of_node_put(node);
		/* Never destroy a device attached to a locked DART stream. */
		if (ret)
			return ret;
		v26->piodma_domain = iommu_get_domain_for_dev(&v26->piodma->dev);
	}
	if (IS_ERR_OR_NULL(v26->piodma_domain))
		return -EIO;
	ret = iommu_map(v26->piodma_domain, buf->iova, buf->phys, buf->size,
			IOMMU_READ | IOMMU_WRITE, GFP_KERNEL);
	if (ret)
		return ret;
	for (offset = 0; offset < buf->size; offset += SZ_16K)
		if (iommu_iova_to_phys(v26->piodma_domain, buf->iova + offset) !=
		    buf->phys + offset)
			return -EIO;
	buf->piodma_mapped = true;
	dev_dbg(v26->dev, "PIODMA mapped DCP buffer %u: %zu bytes at %pad\n",
		 id, buf->size, &buf->iova);
	return 0;
}

/*
 * Keeps the last publication of each runtime property, keyed by its first
 * word (the property id). The M1/M2 firmware reports the brightness it
 * applies this way; kept here, it can be compared with brightness requests
 * on a running system before anything relies on its layout.
 */
static void dcp_v26_publish(struct apple_dcp_v26 *v26, const u8 *in)
{
	u32 id = get_unaligned_le32(in), i;

	v26->publications++;
	for (i = 0; i < v26->published_count; i++)
		if (v26->published[i].id == id)
			break;
	if (i == DCP_V26_MAX_PUBLISHED)
		return;
	if (i == v26->published_count)
		v26->published_count++;
	v26->published[i].id = id;
	memcpy(v26->published[i].data, in + 4, sizeof(v26->published[i].data));
	v26->published[i].count++;
	dev_dbg(v26->dev, "published %u: %*ph\n", id, 12, in + 4);
}

/* Runs on the thread that owns the RPC stream; the lock is held. */
static int dcp_v26_callback(void *cookie, u32 tag,
		    const void *input, u32 in_size, void *output, u32 out_size)
{
	struct apple_dcp_v26 *v26 = cookie;
	const u8 *in = input;
	u8 *out = output;
	struct dcp_v26_property *p;
	u32 id;
	u32 count, offset, service, key_offset;
	int ret;

#define SHAPE(i, o) (in_size == (i) && out_size == (o))
	if (v26->board->sdk27)
		return dcp_v27_callback(v26, tag, input, in_size, output, out_size);
	/* The transport zeroes output, so absent optional metadata stays absent. */
	if (tag == D(114) && SHAPE(4164, 4100)) {
		/* J613 CoreAnalyticsSendEvent: name[64], serialized dictionary[4096],
		 * nullable flag. Retain the latest event locally. The dictionary is
		 * in/out: firmware parses it even on failure, so a zero-filled reply
		 * is invalid. Return its own serialized dictionary without changes.
		 * The final u32 is the method status (zero = accepted).
		 */
		if (!memchr(in, 0, 64) || in[4160] > 1 ||
		    (!in[4160] && in[64] != 'd'))
			return -EINVAL;
		memcpy(v26->analytics_last, in, sizeof(v26->analytics_last));
		v26->analytics++;
		if (!in[4160])
			memcpy(out, in + 64, 4096);
		put_unaligned_le32(0, out + 4096);
		dev_info(v26->dev, "J613 analytics event %llu: %.64s (retained locally)\n",
			 v26->analytics, in);
		return 0;
	}
	if (tag == D(590) && SHAPE(0x730, 0)) {
		id = v26->d590_count++ % ARRAY_SIZE(v26->d590);
		memcpy(v26->d590[id], in, DCP_V26_D590_SIZE);
		v26->d590_time[id] = v26->link.callback_time;
		return 0;
	}
	if (tag == D(588) && SHAPE(0, 0))
		return 0;
	if (tag == D(599) && SHAPE(0, 0))
		return dcp_v26_simple_call(v26, A(412), false, true, true);
	if ((tag == D(108) || tag == D(109) || tag == D(110) || tag == D(111) ||
	     tag == D(112) || tag == D(113) || tag == D(0) || tag == D(1)) && SHAPE(0, 4)) {
		out[0] = 1;
		return 0;
	}
	if (tag == D(575) && SHAPE(88, 76)) {
		if (!(in[84] & 1))
			memcpy(out, in + 8, 76);
		return 0;
	}
	if (tag == D(576) && SHAPE(4, 0))
		return 0;
	if (tag == D(300) && SHAPE(16, 0)) {
		dcp_v26_publish(v26, in);
		return 0;
	}
	if (tag == D(127) && SHAPE(4, 4)) {
		count = get_unaligned_le32(in);
		if (v26->chunk || !count || count > 0x100001)
			return -EINVAL;
		v26->chunk_size = count - 1;
		v26->chunk_offset = 0;
		v26->chunk = kvzalloc(max_t(u32, 1, v26->chunk_size), GFP_KERNEL);
		if (!v26->chunk)
			return -ENOMEM;
		out[0] = 1;
		return 0;
	}
	if (tag == D(128) && SHAPE(0x1008, 4)) {
		offset = get_unaligned_le32(in + 0x1000);
		count = get_unaligned_le32(in + 0x1004);
		if (!v26->chunk || offset != v26->chunk_offset || count > 4096 ||
		    offset > v26->chunk_size || count > v26->chunk_size - offset)
			return -EINVAL;
		memcpy(v26->chunk + offset, in, count);
		v26->chunk_offset += count;
		out[0] = 1;
		return 0;
	}
	if (tag == D(129) && SHAPE(64, 4)) {
		if (!v26->chunk || v26->chunk_offset != v26->chunk_size || !memchr(in, 0, 64) ||
		    v26->raw_count == DCP_V26_MAX_RAW || v26->raw_bytes + v26->chunk_size > SZ_8M)
			return -EINVAL;
		ret = dcp_v26_raw_property(v26, 0, in, v26->chunk, v26->chunk_size);
		if (ret)
			return ret;
		kvfree(v26->chunk);
		v26->chunk = NULL;
		dev_info(v26->dev, "J613 property %.64s: %u bytes\n", in, v26->chunk_size);
		out[0] = 1;
		return 0;
	}
	if (tag == D(408) && SHAPE(8, 8)) {
		count = get_unaligned_le32(in + 4);
		if (memcmp(in, "VORP", 4) || count > 1)
			return -EINVAL;
		/* Same-boot ADT clock observation, supplied by the guarded runner. */
		put_unaligned_le64(count ? 0 : v26->clock_rate, out);
		return 0;
	}
	if (tag == D(125) && SHAPE(100, 36)) {
		count = get_unaligned_le32(in + 64);
		if (count > 8)
			return -EINVAL;
		memcpy(out, in + 68, count * 4);
		return 0;
	}
	if (tag == D(6) && SHAPE(84, 80)) {
		if (!(in[80] & 1))
			memcpy(out, in, 80);
		return 0;
	}
	if ((tag == D(122) || tag == D(123)) && SHAPE(0, 4))
		return 0;
	if (tag == D(121) && SHAPE(0, 4)) {
		static const struct { u32 tag; bool in, reply, expected; } sequence[] = {
			{A(389)}, {A(446), false, true, true}, {A(29)},
			{A(468), true}, {A(0), true, true, true}, {A(465), false, true, true},
		};
		for (count = 0; count < ARRAY_SIZE(sequence); count++) {
			ret = dcp_v26_simple_call(v26, sequence[count].tag, sequence[count].in,
					  sequence[count].reply, sequence[count].expected);
			if (ret)
				return ret;
		}
		out[0] = 1;
		return 0;
	}
	if (tag == D(101) && SHAPE(0, 4)) {
		put_unaligned_le32(v26->stride, out);
		return 0;
	}
	if (tag == D(3) && SHAPE(4, 60)) {
		put_unaligned_le32(1, out + 56); /* Explicitly decline bandwidth service. */
		return 0;
	}
	if (tag == D(451) && SHAPE(20, 28)) {
		u32 alignment = get_unaligned_le32(in + 12);
		u64 size = get_unaligned_le64(in + 4);
		struct dcp_v26_buffer *buf;

		dev_dbg(v26->dev, "allocation flags %#x size %llu alignment %u\n",
			get_unaligned_le32(in), size, alignment);
		if (get_unaligned_le32(in) != 0x703 || !is_power_of_2(alignment) ||
		    alignment > SZ_16K || in[16] || in[17] || in[18])
			return -EINVAL;
		ret = dcp_v26_alloc(v26, size, &id);
		if (ret)
			return ret;
		buf = &v26->buffers[id - 1];
		if ((buf->phys | buf->iova) & (alignment - 1))
			return -EINVAL;
		put_unaligned_le64(buf->phys, out);
		put_unaligned_le64(buf->iova, out + 8);
		put_unaligned_le64(buf->size, out + 16);
		put_unaligned_le32(id, out + 24);
		return 0;
	}
	/* Map a buffer for PIODMA; the AP virtual address is not used. */
	if (tag == D(201) && SHAPE(12, 16)) {
		u64 buffer = get_unaligned_le64(in);

		if (!buffer || buffer > v26->buffer_count || get_unaligned_le32(in + 8) != 1)
			return -EINVAL;
		ret = dcp_v26_map_piodma(v26, buffer);
		if (ret)
			return ret;
		put_unaligned_le32(buffer, out);
		put_unaligned_le64(v26->buffers[buffer - 1].iova, out + 4);
		return 0;
	}
	/* Release: the buffer stays allocated and mapped until reboot. */
	if (tag == D(454) && SHAPE(4, 4)) {
		id = get_unaligned_le32(in);
		if (!id || id > v26->buffer_count || v26->buffers[id - 1].retired)
			return -EINVAL;
		v26->buffers[id - 1].retired = true;
		out[0] = 1;
		return 0;
	}
	if (tag == D(582) && SHAPE(8, 4))
		return 0; /* Optional default surface is not allocated by this host. */
	if (tag == D(400) && SHAPE(76, 0xc04)) {
		count = get_unaligned_le32(in + 68);
		service = get_unaligned_le32(in);
		if (count > 0xc00 || (in[72] & 1) || !memchr(in + 4, 0, 64))
			return -EINVAL;
		for (offset = 0; offset < v26->raw_count; offset++) {
			if (v26->raw[offset].service != service || strcmp(v26->raw[offset].key, in + 4))
				continue;
			if (v26->raw[offset].size > count)
				return -ENOSPC;
			memcpy(out, v26->raw[offset].data, v26->raw[offset].size);
			put_unaligned_le32(v26->raw[offset].size, out + 0xc00);
			break;
		}
		return 0;
	}
	if (tag == D(401) && SHAPE(80, 12)) {
		p = dcp_v26_property(v26, get_unaligned_le32(in), in + 4, false);
		if (IS_ERR(p))
			return PTR_ERR(p);
		if (p) {
			put_unaligned_le64(p->value, out);
			out[8] = 1;
		}
		return 0;
	}
	if ((tag == D(413) && SHAPE(4168, 4)) ||
	    ((tag == D(552) || tag == D(561)) && SHAPE(4164, 4)) ||
	    (tag == D(567) && SHAPE(128, 4))) {
		key_offset = tag == D(413) ? 4 : 0;
		service = key_offset ? get_unaligned_le32(in) : 0;
		if (tag != D(567) && (in[key_offset + 4160] & 1)) {
			out[0] = 1;
			return 0;
		}
		count = tag == D(567) ? strnlen(in + 64, 64) : 4096;
		ret = dcp_v26_raw_property(v26, service, in + key_offset, in + key_offset + 64, count);
		if (!ret)
			out[0] = 1;
		return ret;
	}
	if ((tag == D(563) && SHAPE(76, 4)) || (tag == D(565) && SHAPE(72, 4)) ||
	    (tag == D(414) && SHAPE(80, 4)) || (tag == D(415) && SHAPE(76, 4)) ||
	    ((tag == D(102) || tag == D(104)) && SHAPE(68, 0))) {
		key_offset = (tag == D(414) || tag == D(415)) ? 4 : 0;
		service = key_offset ? get_unaligned_le32(in) : 0;
		count = (tag == D(563) || tag == D(414)) ? 8 : 4;
		if (out_size && (in[key_offset + 64 + count] & 1)) {
			out[0] = 1;
			return 0;
		}
		p = dcp_v26_property(v26, service, in + key_offset, true);
		if (IS_ERR(p))
			return PTR_ERR(p);
		p->value = count == 8 ? get_unaligned_le64(in + key_offset + 64) :
				       get_unaligned_le32(in + key_offset + 64);
		if (tag == D(104))
			p->value = in[64];
		if (out_size)
			out[0] = 1;
		return 0;
	}
	if (tag == D(406) && SHAPE(72, 0))
		return 0;
	if (tag == D(107) && SHAPE(64, 0)) {
		p = dcp_v26_property(v26, 0, in, false);
		if (IS_ERR(p))
			return PTR_ERR(p);
		if (p) {
			*p = v26->properties[--v26->property_count];
			memset(&v26->properties[v26->property_count], 0, sizeof(*p));
		}
		return 0;
	}
	if ((tag == D(206) || tag == D(207)) && SHAPE(0, 4)) {
		ret = dcp_v26_simple_call(v26, tag == D(206) ? A(131) : A(132), false, true, false);
		if (!ret)
			out[0] = 1;
		return ret;
	}
	if (tag == D(100) && SHAPE(0, 0))
		return dcp_v26_simple_call(v26, A(390), false, true, false);
	dev_err(v26->dev, "J613 unhandled kernel callback %#x %u/%u\n", tag, in_size, out_size);
	return -EOPNOTSUPP;
#undef SHAPE
}

/*
 * SDK 27 (J704) calls: a method's u32 reply is logged, not checked, so that
 * one unexpected answer does not end the session while the numbering is
 * being qualified. Returns the transport result.
 */
static int dcp_v27_call(struct apple_dcp_v26 *v26, u32 number, bool one, bool reply,
			u32 *result)
{
	__le32 in = cpu_to_le32(1), out = 0;
	u32 tag = DCP_V26_TAG('A', number);
	int ret;

	ret = dcp_v26_call(v26, tag, one ? &in : NULL, one ? 4 : 0, reply ? &out : NULL,
			   reply ? 4 : 0, 0);
	if (result)
		*result = le32_to_cpu(out);
	dev_dbg(v26->dev, "IOMFB27 A%03u in=%u out=%u ret=%d reply=%#x\n", number,
		 one ? 4 : 0, reply ? 4 : 0, ret, le32_to_cpu(out));
	return ret;
}

/* A setProperty-style callback: keep the bytes under the key, answer true. */
static int dcp_v27_store(struct apple_dcp_v26 *v26, u32 service, const u8 *key,
			 const u8 *data, u32 size, u8 *out)
{
	int ret;

	if (!memchr(key, 0, 64))
		return -EINVAL;
	ret = dcp_v26_raw_property(v26, service, key, data, size);
	if (ret == -ENOSPC) {
		dev_warn_once(v26->dev, "IOMFB27 property store full; not keeping %.64s\n", key);
		ret = 0;
	}
	if (!ret && out)
		out[0] = 1;
	return ret;
}

static u32 dcp_v27_number(u32 tag)
{
	return ((tag >> 16) & 0xff) * 100 + ((tag >> 8) & 0xff) * 10 + (tag & 0xff) -
	       '0' * 111;
}

/*
 * The SDK-27 callback table. Shapes are the firmware's own size gates
 * (sol-j704-dcp-27-profile-spec); numbering against 25G83: D108..D129 moved
 * to D107..D128 and D582..D599 to D586..D603. A callback this table does not
 * know is answered with zeroes and logged once with its first bytes.
 */
static int dcp_v27_callback(struct apple_dcp_v26 *v26, u32 tag, const u8 *in, u32 in_size,
			    u8 *out, u32 out_size)
{
	u32 n = dcp_v27_number(tag), count, offset, service, id, value;
	struct apple_dcp *dcp = READ_ONCE(v26->dcp);
	struct dcp_v26_property *p;
	int ret;

	if ((tag >> 24) != 'D' || n >= 1000)
		return -EPROTO;
	if (v26->board->external)
		/* Hundreds per hotplug: a console at loglevel 8 slows DCP replies. */
		dev_dbg(v26->dev, "IOMFB27 ext D%03u %#x/%#x: %*phN\n", n, in_size, out_size,
			 (int)min_t(u32, in_size, 16), in);
#define SHAPE(i, o) (in_size == (i) && out_size == (o))
	switch (n) {
	case 0: case 1: case 107: case 108: case 109: case 110: case 111: case 112:
		/* Service creation and the two early queries: true. */
		if (!SHAPE(0, 4))
			break;
		out[0] = 1;
		return 0;
	case 2: case 103: case 116: case 130: case 132: case 208: case 402: case 404:
	case 406: case 407: case 423: case 424: case 576: case 577: case 578: case 579:
	case 580: case 581: case 582: case 584: case 587: case 588: case 590: case 592:
	case 593: case 596: case 597: case 598: case 599: case 600:
		/* Notifications: nothing to answer. */
		if (out_size)
			break;
		return 0;
	case 3:
		if (!SHAPE(4, 0x14))
			break;
		/* Real-time bandwidth: the PMGR scratch word and request doorbell. */
		if (!(get_unaligned_le32(in) & 1)) {
			put_unaligned_le64(v26->bw_scratch, out);
			put_unaligned_le64(v26->bw_doorbell, out + 8);
		}
		return 0;
	case 4: case 5: case 7: case 8: case 115: case 117: case 118: case 119: case 121:
	case 122: case 123: case 133: case 200: case 204: case 205: case 211: case 412:
	case 452: case 453: case 571: case 572: case 573: case 586: case 601: case 602:
		/* Queries without a qualified positive answer: zero (false, status 0). */
		return 0;
	case 6:
		if (!SHAPE(0x54, 0x50))
			break;
		if (!(in[0x50] & 1))
			memcpy(out, in, 0x50);
		return 0;
	case 100:
		if (!SHAPE(0, 0))
			break;
		/* 25G83 D100 -> A390, same number here (0/4). */
		return dcp_v27_call(v26, 390, false, true, NULL);
	case 101:
		if (!SHAPE(0, 4))
			break;
		put_unaligned_le32(v26->stride, out);
		return 0;
	case 102: case 104:
		if (!SHAPE(0x44, 0))
			break;
		p = dcp_v26_property(v26, 0, in, true);
		if (IS_ERR(p))
			return PTR_ERR(p) == -ENOSPC ? 0 : PTR_ERR(p);
		p->value = n == 104 ? in[64] : get_unaligned_le32(in + 64);
		return 0;
	case 106:
		if (!SHAPE(0x40, 0))
			break;
		p = dcp_v26_property(v26, 0, in, false);
		if (IS_ERR(p))
			return PTR_ERR(p);
		if (p) {
			*p = v26->properties[--v26->property_count];
			memset(&v26->properties[v26->property_count], 0, sizeof(*p));
		}
		return 0;
	case 113:
		if (!SHAPE(0x1044, 0x1004))
			break;
		/* CoreAnalyticsSendEvent: hand its serialized dictionary back unchanged. */
		if (!memchr(in, 0, 64) || in[0x1040] > 1)
			return -EINVAL;
		v26->analytics++;
		if (!in[0x1040] && in[64] == 'd')
			memcpy(out, in + 64, 0x1000);
		else
			out[0] = 'd';
		return 0;
	case 114:
		if (!SHAPE(0x10, 8))
			break;
		if (!(in[0xc] & 1))
			memcpy(out, in + 8, 4);
		return 0;
	case 120:
		if (!SHAPE(0, 4))
			break;
		/*
		 * start_hardware_boot. The 25G83 host sends A389 A446 A029 A468(1)
		 * A000(1) A465 here; on SDK 27 those are A389 A454 A027 A478 A000 A475.
		 */
		if (READ_ONCE(j704_dcp_boot_chain)) {
			ret = dcp_v27_call(v26, 389, false, false, NULL);
			if (!ret)
				ret = dcp_v27_call(v26, 454, false, true, NULL);
			if (!ret)
				ret = dcp_v27_call(v26, 27, false, false, NULL);
			if (!ret)
				ret = dcp_v27_call(v26, 478, true, false, NULL);
			if (!ret)
				ret = dcp_v27_call(v26, 0, true, true, NULL);
			if (!ret)
				ret = dcp_v27_call(v26, 475, false, true, NULL);
			if (ret)
				return ret;
		}
		dev_info(v26->dev, "IOMFB27 D120 start_hardware_boot answered true (chain %d)\n",
			 READ_ONCE(j704_dcp_boot_chain));
		out[0] = 1;
		return 0;
	case 124:
		if (!SHAPE(0x64, 0x24))
			break;
		/* read_edt_data: no EDT here; keep the caller's defaults. */
		count = get_unaligned_le32(in + 0x40);
		if (count > 8)
			return -EINVAL;
		memcpy(out, in + 0x44, count * 4);
		return 0;
	case 125:
		if (!SHAPE(0x48, 0x48))
			break;
		return 0;
	case 126:
		if (!SHAPE(4, 4))
			break;
		count = get_unaligned_le32(in);
		if (v26->chunk || !count || count > 0x100001)
			return -EINVAL;
		v26->chunk_size = count - 1;
		v26->chunk_offset = 0;
		v26->chunk = kvzalloc(max_t(u32, 1, v26->chunk_size), GFP_KERNEL);
		if (!v26->chunk)
			return -ENOMEM;
		out[0] = 1;
		return 0;
	case 127:
		if (!SHAPE(0x1008, 4))
			break;
		offset = get_unaligned_le32(in + 0x1000);
		count = get_unaligned_le32(in + 0x1004);
		if (!v26->chunk || offset != v26->chunk_offset || count > 0x1000 ||
		    offset > v26->chunk_size || count > v26->chunk_size - offset)
			return -EINVAL;
		memcpy(v26->chunk + offset, in, count);
		v26->chunk_offset += count;
		out[0] = 1;
		return 0;
	case 128:
		if (!SHAPE(0x40, 4))
			break;
		if (!v26->chunk || v26->chunk_offset != v26->chunk_size || !memchr(in, 0, 64))
			return -EINVAL;
		ret = dcp_v26_raw_property(v26, 0, in, v26->chunk, v26->chunk_size);
		dev_dbg(v26->dev, "IOMFB27 property %.64s: %u bytes (%d)\n", in,
			 v26->chunk_size, ret);
		kvfree(v26->chunk);
		v26->chunk = NULL;
		if (ret)
			return ret;
		out[0] = 1;
		return 0;
	case 201:
		if (!SHAPE(0xc, 0x10))
			break;
		/* Map a firmware buffer for PIODMA at its own address. */
		id = get_unaligned_le64(in);
		if (!id || id > v26->buffer_count || get_unaligned_le32(in + 8) != 1)
			return -EINVAL;
		ret = dcp_v26_map_piodma(v26, id);
		if (ret)
			return ret;
		put_unaligned_le32(id, out);
		put_unaligned_le64(v26->buffers[id - 1].iova, out + 4);
		return 0;
	case 202:
		if (!SHAPE(0x18, 0))
			break;
		return 0; /* Unmap: the buffer stays mapped until reboot. */
	case 206: case 207:
		if (!SHAPE(0, 4))
			break;
		/* 25G83 answers these with A131/A132 (0/4); here they are A136/A137. */
		if (READ_ONCE(j704_dcp_service_match)) {
			ret = dcp_v27_call(v26, n == 206 ? 136 : 137, false, true, &value);
			if (ret)
				return ret;
			out[0] = !value;
		}
		return 0;
	case 209:
		if (!SHAPE(0, 8))
			break;
		put_unaligned_le64(ktime_get_real_ns() / NSEC_PER_MSEC, out);
		return 0;
	case 300:
		if (!SHAPE(0x10, 0))
			break;
		dcp_v26_publish(v26, in);
		return 0;
	case 400:
		if (!SHAPE(0x4c, 0xc04))
			break;
		count = get_unaligned_le32(in + 0x44);
		service = get_unaligned_le32(in);
		if (count > 0xc00 || !memchr(in + 4, 0, 64))
			return -EINVAL;
		for (offset = 0; offset < v26->raw_count; offset++) {
			if (v26->raw[offset].service != service || strcmp(v26->raw[offset].key, in + 4))
				continue;
			if (v26->raw[offset].size <= count) {
				memcpy(out, v26->raw[offset].data, v26->raw[offset].size);
				put_unaligned_le32(v26->raw[offset].size, out + 0xc00);
			}
			break;
		}
		return 0;
	case 401:
		if (!SHAPE(0x50, 0xc))
			break;
		p = dcp_v26_property(v26, get_unaligned_le32(in), in + 4, false);
		if (IS_ERR(p))
			return PTR_ERR(p);
		if (p) {
			put_unaligned_le64(p->value, out);
			out[8] = 1;
		} else if (!(in[0x4c] & 1)) {
			memcpy(out, in + 0x44, 8);
		}
		return 0;
	case 405:
		if (!SHAPE(0x4c, 8))
			break;
		if (!(in[0x48] & 1))
			memcpy(out, in + 0x44, 4);
		return 0;
	case 408:
		if (!SHAPE(8, 8))
			break;
		/*
		 * Clock rates. Selector 0 must cover the panel's video clock: the
		 * firmware compares it with 402278400 and otherwise gives up on its
		 * frequency table. 402280000 is the video clock it then defaults to
		 * (ADT clock-frequencies[6]); selector 1 is the 24 MHz reference.
		 */
		value = get_unaligned_le32(in + 4);
		put_unaligned_le64(value == 0 ? 402280000 : value == 1 ? 24000000 : 0, out);
		return 0;
	case 409:
		if (!SHAPE(0xc, 4))
			break;
		put_unaligned_le32(1, out);
		return 0;
	case 410:
		if (!SHAPE(0x14, 8))
			break;
		if (!(in[0x10] & 1))
			memcpy(out, in + 8, 4);
		put_unaligned_le32(1, out + 4);
		return 0;
	case 411:
		if (!SHAPE(0x10, 0x1c))
			break;
		/* mapDeviceMemoryWithIndex: disp-N, reached physically (dva == address). */
		id = get_unaligned_le32(in + 4);
		if (!dcp || id >= dcp->nr_disp_registers || !dcp->disp_registers[id]) {
			put_unaligned_le32(1, out + 0x18);
			return 0;
		}
		put_unaligned_le64(dcp->disp_registers[id]->start, out);
		put_unaligned_le64(dcp->disp_registers[id]->start, out + 8);
		put_unaligned_le64(resource_size(dcp->disp_registers[id]), out + 0x10);
		return 0;
	case 413: case 418:
		if (!SHAPE(0x1048, 4))
			break;
		if (in[0x1044] & 1) {
			out[0] = 1;
			return 0;
		}
		return dcp_v27_store(v26, get_unaligned_le32(in), in + 4, in + 0x44, 0x1000, out);
	case 414: case 415: case 416: case 417: case 419: case 420: case 421: case 422:
		if (in_size < 0x45 || out_size != 4)
			break;
		return dcp_v27_store(v26, get_unaligned_le32(in), in + 4, in + 0x44,
				     in_size - 0x44, out);
	case 451:
		if (!SHAPE(0x14, 0x14))
			break;
		{
			u64 size = get_unaligned_le64(in + 4);
			u32 options = get_unaligned_le32(in + 0xc);
			struct dcp_v26_buffer *buf;

			dev_dbg(v26->dev, "IOMFB27 D451 allocate %#x size %llu options %#x\n",
				 get_unaligned_le32(in), size, options);
			ret = dcp_v26_alloc(v26, size, &id);
			if (ret)
				return ret;
			buf = &v26->buffers[id - 1];
			put_unaligned_le64(buf->iova, out);
			put_unaligned_le64(buf->size, out + 8);
			put_unaligned_le32(id, out + 0x10);
		}
		return 0;
	case 454:
		if (!SHAPE(4, 4))
			break;
		id = get_unaligned_le32(in);
		if (!id || id > v26->buffer_count || v26->buffers[id - 1].retired)
			return -EINVAL;
		v26->buffers[id - 1].retired = true;
		return 0;
	case 550: case 551: case 552: case 553: case 554: case 555: case 556: case 557:
	case 558: case 559: case 560: case 561: case 562: case 563: case 564: case 565:
	case 566: case 567: case 568: case 569: case 570:
		if (in_size < 0x41 || out_size != 4)
			break;
		return dcp_v27_store(v26, 0, in, in + 0x40, in_size - 0x40, out);
	case 574:
		if (!SHAPE(4, 8))
			break;
		/* Luminance span: binary32 219.0 for selector 0, as on H17P. */
		if (!get_unaligned_le32(in))
			put_unaligned_le64(0x435b0000, out);
		return 0;
	case 575:
		if (!SHAPE(0x58, 0x4c))
			break;
		dev_info(v26->dev, "IOMFB27 hotplug connected=%llu\n", get_unaligned_le64(in));
		if (!(in[0x54] & 1))
			memcpy(out, in + 8, 0x4c);
		if (v26->board->external) {
			WRITE_ONCE(v26->ext_hotplug_connected, !!get_unaligned_le64(in));
			if (!get_unaligned_le64(in))
				WRITE_ONCE(v26->ext_pipe_on, false);
			schedule_work(&v26->ext_hotplug_work);
		}
		return 0;
	case 589:
		if (!SHAPE(0xe4, 0xe0))
			break;
		if (!(in[0xe0] & 1))
			memcpy(out, in, 0xe0);
		return 0;
	case 594:
		if (!SHAPE(DCP_V26_D590_SIZE, 0))
			break;
		id = v26->d590_count++ % ARRAY_SIZE(v26->d590);
		memcpy(v26->d590[id], in, DCP_V26_D590_SIZE);
		v26->d590_time[id] = v26->link.callback_time;
		return 0;
	case 595:
		if (!SHAPE(0x2708, 0x400))
			break;
		/* Batched completion: the ids go back as they came. */
		memcpy(out, in, 0x400);
		count = get_unaligned_le32(in + 0x400);
		if (count && count <= 256) {
			id = v26->d590_count++ % ARRAY_SIZE(v26->d590);
			memcpy(v26->d590[id], in, DCP_V26_D590_SIZE);
			v26->d590_time[id] = v26->link.callback_time;
		}
		return 0;
	case 603:
		if (!SHAPE(0, 0))
			break;
		/* 25G83 D599 -> A412 (is_main_display), same number here. */
		return dcp_v27_call(v26, 412, false, true, NULL);
	default:
		break;
	}
#undef SHAPE
	if (!test_and_set_bit(n, v26->v27_logged))
		dev_warn(v26->dev, "IOMFB27 D%03u %#x/%#x answered with zeroes: %*phN\n", n,
			 in_size, out_size, (int)min_t(u32, in_size, 32), in);
	return 0;
}

static void dcp_v26_idle(struct work_struct *work)
{
	struct apple_dcp_v26 *v26 = container_of(work, struct apple_dcp_v26, idle_work);
	int ret = 0;

	mutex_lock(&v26->lock);
	while (!v26->failed && !ret)
		ret = dcp_v26_link_pump(&v26->link, 0, dcp_v26_callback, v26);
	if (ret && ret != -ETIMEDOUT && ret != -EAGAIN) {
		v26->failed = true;
		dev_err(v26->dev, "DCP notification failed: %d; recovery requires a reboot\n", ret);
	}
	mutex_unlock(&v26->lock);
}

static void dcp_v26_crashed(void *cookie, const void *crashlog, size_t crashlog_size)
{
	struct apple_dcp_v26 *v26 = cookie;
	/* failed is session-owned; no dereference of a detachable KMS device. */
	WRITE_ONCE(v26->failed, true);
	dev_err(v26->dev, "DCP firmware crashed; its buffers are kept until reboot\n");
	dcp_v26_link_fail(&v26->link);
}

static void dcp_v26_recv(void *cookie, u8 endpoint, u64 message)
{
	struct apple_dcp_v26 *v26 = cookie;

	if (endpoint != APPLE_DCP_LINK_ENDPOINT) {
		struct apple_dcp *dcp = READ_ONCE(v26->dcp);

		if (v26->board->external && dcp && dcp->rtk) {
			dcp_v26_ext_route(dcp, endpoint, message);
			return;
		}
		dev_dbg(v26->dev, "ignored endpoint %#x message %#llx\n", endpoint, message);
		return;
	}
	dcp_v26_link_receive(&v26->link, message);
}

/*
 * Buffers the firmware already owns must lie in a reserved region the boot
 * loader described, and be mapped linearly there. The OS log is a physical
 * address, not a DART address.
 */
static int dcp_v26_shmem_setup(void *cookie, struct apple_rtkit_shmem *bfr)
{
	struct apple_dcp_v26 *v26 = cookie;
	struct device *dev = v26->dev;
	struct iommu_domain *domain = iommu_get_domain_for_dev(dev);
	phys_addr_t phys;
	int i;

	if (!READ_ONCE(v26->dcp) || !bfr->size || bfr->size > SZ_16M || !domain)
		return -EINVAL;
	if (!bfr->iova) {
		bfr->buffer = dma_alloc_coherent(dev, bfr->size, &bfr->iova, GFP_KERNEL);
		if (!bfr->buffer)
			return -ENOMEM;
		memset(bfr->buffer, 0, bfr->size);
		dma_wmb();
		return 0;
	}
	phys = iommu_iova_to_phys(domain, bfr->iova);
	for (i = 0; ; i++) {
		struct device_node *node = of_parse_phandle(dev->of_node, "memory-region", i);
		struct resource res;
		size_t off;
		bool oslog, valid;

		if (!node)
			return -ERANGE;
		oslog = of_property_read_bool(node, "apple,dcp-os-log");
		valid = !of_address_to_resource(node, 0, &res);
		of_node_put(node);
		if (!valid)
			continue;
		if (oslog && bfr->iova == res.start)
			phys = res.start;
		if (phys < res.start || phys > res.end || bfr->size - 1 > res.end - phys)
			continue;
		if (!oslog) {
			for (off = 0; off < bfr->size; off += SZ_16K)
				if (iommu_iova_to_phys(domain, bfr->iova + off) != phys + off)
					return -ERANGE;
			if (iommu_iova_to_phys(domain, bfr->iova + bfr->size - 1) !=
			    phys + bfr->size - 1)
				return -ERANGE;
		}
		bfr->buffer = memremap(phys, bfr->size, MEMREMAP_WB);
		if (!bfr->buffer)
			return -ENOMEM;
		bfr->is_mapped = true;
		dev_dbg(dev, "RTKit buffer at %pad: %pa, %zu bytes\n", &bfr->iova, &phys,
			bfr->size);
		return 0;
	}
}

static void dcp_v26_shmem_destroy(void *cookie, struct apple_rtkit_shmem *bfr)
{
	struct apple_dcp_v26 *v26 = cookie;

	if (bfr->is_mapped)
		memunmap(bfr->buffer);
	else
		dma_free_coherent(v26->dev, bfr->size, bfr->buffer, bfr->iova);
}

static const struct apple_rtkit_ops dcp_v26_rtkit_ops = {
	.crashed = dcp_v26_crashed,
	.recv_message = dcp_v26_recv,
	.shmem_setup = dcp_v26_shmem_setup,
	.shmem_destroy = dcp_v26_shmem_destroy,
};

/* Freed only if RTKit never started: the firmware may call into it until reboot. */
static void dcp_v26_release(void *data)
{
	struct apple_dcp_v26 *v26 = data;

	if (!v26->rtk) {
		put_device(v26->dev);
		kfree(v26);
	}
}

/* Read only: bind maps and claims these registers. */
static int dcp_v26_cpu_running(struct device *dev, const char *name)
{
	struct resource *res;
	void __iomem *coproc;
	u32 control;

	res = platform_get_resource_byname(to_platform_device(dev), IORESOURCE_MEM, "coproc");
	if (!res)
		return -ENODEV;
	coproc = ioremap_np(res->start, resource_size(res));
	if (!coproc)
		return -ENOMEM;
	control = readl(coproc + DCP_V26_CPU_CONTROL);
	iounmap(coproc);
	if (!(control & APPLE_DCP_COPROC_CPU_CONTROL_RUN))
		return dev_err_probe(dev, -EBUSY,
				     "%s display not started: the DCP CPU is stopped (%#x); the PMP is not started for it\n",
				     name, control);
	return 0;
}

/*
 * The board of a 14.x IOMFB DCP node: NULL if the node is not one, an error
 * if it is but the machine is not a board this file knows.
 */
const struct dcp_v26_board *iomfb_v26_6_board(struct device *dev)
{
	const struct dcp_v26_board *board, *known = NULL;
	unsigned int i;

	for (i = 0; i < ARRAY_SIZE(dcp_v26_boards); i++) {
		board = dcp_v26_boards[i];
		if (!of_device_is_compatible(dev->of_node, board->dcp_compatible))
			continue;
		if (board->machine && !of_machine_is_compatible(board->machine)) {
			known = board;
			continue;
		}
		return board;
	}
	if (known) {
		dev_err(dev, "25G83 display not started: this machine is none of the 25G83 boards\n");
		return ERR_PTR(-ENODEV);
	}
	return NULL;
}

const char *iomfb_v26_6_board_name(const struct dcp_v26_board *board)
{
	return board->name;
}

/*
 * The PMP device-power entry points are optional at link time: until a
 * provider is built, behave as their !CONFIG_APPLE_PMP stubs do.
 */
static int dcp_v26_pmp_link_device(struct device *dev)
{
	typeof(&apple_pmp_link_device) link;
	int ret;

	link = symbol_get(apple_pmp_link_device);
	if (!link)
		return -ENODEV;
	ret = link(dev);
	symbol_put(apple_pmp_link_device);
	return ret;
}

static int dcp_v26_pmp_set_device_power(u8 command, u16 device_id, u32 enabled)
{
	typeof(&apple_pmp_set_device_power) set;
	int ret;

	set = symbol_get(apple_pmp_set_device_power);
	if (!set)
		return -ENODEV;
	ret = set(command, device_id, enabled);
	symbol_put(apple_pmp_set_device_power);
	return ret;
}

static int keep_display_power(struct device *dev)
{
	struct device_node *node;
	const u8 *table;
	int length, ret = -EINVAL;

	/* The power API is exported before the firmware supplier has probed.
	 * Link to the DT supplier first so coldplug defers and removal is ordered. */
	ret = dcp_v26_pmp_link_device(dev);
	if (ret)
		return ret;
	node = of_parse_phandle(dev->of_node, "apple,pmp", 0);
	if (!node)
		return -ENODEV;
	table = of_get_property(node, "apple,tunable-soc-device", &length);
	/* J613/25G83: logical ID 8, record/bitmap index 11, name DISP. */
	if (table && length == 20 * 0x7c &&
	    get_unaligned_le32(table + 11 * 0x7c) == 8 &&
	    !memcmp(table + 11 * 0x7c + 0x74, "DISP\0\0\0\0", 8))
		ret = dcp_v26_pmp_set_device_power(0x0f, 8, 1);
	of_node_put(node);
	if (ret)
		return ret;
	/* G52's synchronous PMP API checks the command acknowledgement. That
	 * does not qualify domain stability; retain the vote until reboot. */
	dev_info(dev, "J613 native DISP power vote acknowledged: logical-id=8 record=11\n");
	return 0;
}

/* PMGR bandwidth scratch word and doorbell of the SDK-27 D003 reply, from the DT. */
static int dcp_v27_bandwidth(struct apple_dcp_v26 *v26, struct device_node *np)
{
	struct of_phandle_args args;
	struct resource res;
	int ret;

	ret = of_parse_phandle_with_args(np, "apple,bw-scratch", "#apple,bw-scratch-cells",
					 0, &args);
	if (ret)
		return ret;
	ret = args.args_count == 3 ? of_address_to_resource(args.np, args.args[0], &res) :
				     -EINVAL;
	of_node_put(args.np);
	if (ret)
		return ret;
	v26->bw_scratch = res.start + args.args[2];
	ret = of_parse_phandle_with_args(np, "apple,bw-doorbell", "#apple,bw-doorbell-cells",
					 0, &args);
	if (ret)
		return ret;
	ret = args.args_count == 2 ? of_address_to_resource(args.np, args.args[0], &res) :
				     -EINVAL;
	of_node_put(args.np);
	if (ret)
		return ret;
	v26->bw_doorbell = res.start;
	return 0;
}

/* The disp-N register windows that D411 hands out. */
static int dcp_v27_disp_regs(struct apple_dcp *dcp)
{
	struct platform_device *pdev = to_platform_device(dcp->dev);
	struct resource *res;
	char name[16];
	unsigned int i;

	for (i = 0; i < MAX_DISP_REGISTERS; i++) {
		snprintf(name, sizeof(name), "disp-%u", i);
		res = platform_get_resource_byname(pdev, IORESOURCE_MEM, name);
		if (!res)
			break;
		dcp->disp_registers[i] = res;
	}
	dcp->nr_disp_registers = i;
	return i ? 0 : -ENODEV;
}

/*
 * J704: the macOS 27.0 DCP session iBoot left running. The overlay that
 * describes it marks the DCP, display and PIODMA nodes once their live
 * mappings are reserved; the firmware image itself is checked by the DCPLink
 * READY message before the first call.
 */
static int dcp_v27_probe(struct apple_dcp *dcp, const struct dcp_v26_board *board)
{
	struct device *dev = dcp->dev;
	struct device_node *np = dev->of_node, *display, *piodma;
	struct apple_dcp_v26 *v26;
	const char *sdk = NULL;
	u32 markers[3] = {};
	int ret;

	if (of_property_read_string(np, "apple,j704-dcp-sdk", &sdk) || strcmp(sdk, "27.0"))
		return dev_err_probe(dev, -ENODEV, "%s display not started: DCP SDK %s\n",
				     board->name, sdk ?: "(unknown)");
	/*
	 * The display subsystem is found by its compatible. boot.bin v6 carries these nodes in the
	 * boot device tree without the dcp/disp0 aliases, which would switch on Aura's own T8142
	 * display carve-outs; the runtime overlay still provides the alias.
	 */
	display = board->external ? NULL : of_find_node_by_path("disp0");
	if (display && !of_device_is_compatible(display, board->display_compatible)) {
		of_node_put(display);
		display = NULL;
	}
	if (!display)
		display = of_find_compatible_node(NULL, NULL, board->display_compatible);
	piodma = of_get_child_by_name(np, "piodma");
	ret = !of_property_read_u32(np, board->handoff, &markers[0]) && markers[0] == 1 &&
	      display && !of_property_read_u32(display, board->handoff, &markers[1]) &&
	      markers[1] == 1 && piodma &&
	      !of_property_read_u32(piodma, board->handoff, &markers[2]) && markers[2] == 1 ?
	      0 : -ENODEV;
	of_node_put(display);
	of_node_put(piodma);
	if (ret)
		return dev_err_probe(dev, ret, "%s display mapping handoff missing\n", board->name);
	if (!iommu_get_domain_for_dev(dev))
		return dev_err_probe(dev, -ENODEV, "%s display not started: the DCP has no DART domain\n",
				     board->name);
	ret = dcp_v26_cpu_running(dev, board->name);
	if (ret)
		return ret;
	ret = dcp_v27_disp_regs(dcp);
	if (ret)
		return dev_err_probe(dev, ret, "%s display registers missing\n", board->name);

	v26 = kzalloc_obj(*v26);
	if (!v26)
		return -ENOMEM;
	v26->dev = get_device(dev);
	v26->board = board;
	v26->dcp = dcp;
	mutex_init(&v26->lock);
	INIT_WORK(&v26->idle_work, dcp_v26_idle);
	INIT_WORK(&v26->ext_hotplug_work, dcp_v26_ext_hotplug);
	init_waitqueue_head(&v26->bl_wait);
	mutex_init(&v26->bl_lock);
	INIT_WORK(&v26->bl_register_work, dcp_v26_backlight_register);
	ret = devm_add_action_or_reset(dev, dcp_v26_release, v26);
	if (ret)
		return ret;
	ret = dcp_v27_bandwidth(v26, np);
	if (ret && board->external) {
		/* Unknown for DISPEXT: D003 is answered without a scratch word. */
		dev_info(dev, "%s: no bandwidth registers described (%d)\n", board->name, ret);
		v26->bw_scratch = 0;
		v26->bw_doorbell = 0;
		ret = 0;
	}
	if (ret)
		return dev_err_probe(dev, ret, "%s bandwidth registers missing\n", board->name);
	dcp->v26 = v26;
	dev_info(dev, "%s: DCP running, %u display windows, bandwidth %#llx/%#llx\n",
		 board->name, dcp->nr_disp_registers, v26->bw_scratch, v26->bw_doorbell);
	return 0;
}

int iomfb_v26_6_probe(struct apple_dcp *dcp)
{
	struct device *dev = dcp->dev;
	struct device_node *np = dev->of_node;
	const struct dcp_v26_board *board = iomfb_v26_6_board(dev);
	struct apple_dcp_v26 *v26;
	const char *uuid = NULL;
	u32 profile, markers[3];
	struct device_node *display, *piodma;
	int ret;
	u32 version[3];

	if (!IS_ERR_OR_NULL(board) && board->sdk27)
		return dcp_v27_probe(dcp, board);
	if (of_property_count_u32_elems(np, "apple,j613-25g83-profile") != 1 ||
	    of_property_read_u32(np, "apple,j613-25g83-profile", &profile) ||
	    of_property_count_u32_elems(np, "apple,firmware-compat") != 3 ||
	    of_property_read_u32_array(np, "apple,firmware-compat", version, 3) ||
	    of_property_read_string(np, "apple,firmware-uuid", &uuid) ||
	    !j613_25g83_identity(apple_t8122_25g83_board(), profile, version, 3, uuid))
		return -ENODEV;
	if (IS_ERR_OR_NULL(board))
		return -ENODEV;
	if (of_property_read_string(np, "apple,firmware-uuid", &uuid) ||
	    strcmp(uuid, board->firmware_uuid))
		return dev_err_probe(dev, -ENODEV,
				     "%s display not started: DCP firmware %s is not supported\n",
				     board->name, uuid ?: "(unknown)");
	/* The display subsystem sits at the root, outside the simple-bus. */
	display = of_find_node_by_path("disp0");
	if (display && !of_device_is_compatible(display, "apple,t8122-display-subsystem")) {
		of_node_put(display);
		display = NULL;
	}
	piodma = of_get_child_by_name(np, "piodma");
	ret = of_property_count_u32_elems(np, board->handoff) == 1 &&
		!of_property_read_u32(np, board->handoff, &markers[0]) &&
		display && of_property_count_u32_elems(display, board->handoff) == 1 &&
		!of_property_read_u32(display, board->handoff, &markers[1]) &&
		piodma && of_property_count_u32_elems(piodma, board->handoff) == 1 &&
		!of_property_read_u32(piodma, board->handoff, &markers[2]) &&
		j613_25g83_mappings(markers, 3) && apple_j613_25g83_clock_hz() ? 0 : -ENODEV;
	of_node_put(display);
	of_node_put(piodma);
	if (ret)
		return dev_err_probe(dev, ret, "J613/25G83 display mapping or clock handoff missing\n");
	if (!iommu_get_domain_for_dev(dev))
		return dev_err_probe(dev, -ENODEV,
				     "%s display not started: the DCP has no DART domain\n",
				     board->name);
	ret = dcp_v26_cpu_running(dev, board->name);
	if (ret)
		return ret;

	ret = keep_display_power(dev);
	if (ret)
		return dev_err_probe(dev, ret, "J613/25G83 DISP power vote failed\n");
	ret = dcp_v26_cpu_running(dev, board->name);
	if (ret)
		return ret;

	v26 = kzalloc_obj(*v26);
	if (!v26)
		return -ENOMEM;
	v26->dev = get_device(dev);
	v26->board = board;
	v26->dcp = dcp;
	mutex_init(&v26->lock);
	INIT_WORK(&v26->idle_work, dcp_v26_idle);
	INIT_WORK(&v26->ext_hotplug_work, dcp_v26_ext_hotplug);
	init_waitqueue_head(&v26->bl_wait);
	mutex_init(&v26->bl_lock);
	INIT_WORK(&v26->bl_register_work, dcp_v26_backlight_register);
	ret = devm_add_action_or_reset(dev, dcp_v26_release, v26);
	if (ret)
		return ret;
	dcp->v26 = v26;
	dev_info(dev, "PMP running with the display request acknowledged\n");
	return 0;
}

/*
 * The boot framebuffer gives the stride and the panel size. The boot loader either hides
 * the notch rows from it (the default) or, with chosen.asahi,show-notch=1 in m1n1, keeps
 * them; apple,notch-height is recorded either way. A board with a known panel tells the
 * two apart by the height; any other board is taken to have the notch hidden.
 */
static int dcp_v26_geometry(struct apple_dcp_v26 *v26)
{
	struct device_node *fb = of_find_compatible_node(NULL, NULL, "simple-framebuffer");
	u32 notch = 0;
	int ret;

	if (!fb)
		return -ENODEV;
	ret = of_property_read_u32(fb, "width", &v26->fb_width);
	if (!ret)
		ret = of_property_read_u32(fb, "height", &v26->fb_height);
	if (!ret)
		ret = of_property_read_u32(fb, "stride", &v26->stride);
	of_node_put(fb);
	if (ret)
		return ret;
	of_property_read_u32(v26->dev->of_node, "apple,notch-height", &notch);
	/* The firmware takes this stride as its default: 4 bytes per pixel. */
	if (!v26->fb_width || !v26->fb_height || v26->stride != v26->fb_width * 4 ||
	    notch > MAX_NOTCH_HEIGHT)
		return -EINVAL;
	v26->notch_rows = notch;
	v26->panel_width = v26->fb_width;
	v26->panel_height = v26->fb_height + notch;
	/* The boot loader kept the notch rows: the framebuffer is the whole panel. */
	if (v26->board->panel_height && notch && v26->fb_height == v26->board->panel_height)
		v26->panel_height = v26->fb_height;
	/* A board with a known panel takes no other. */
	if (v26->board->panel_width &&
	    (v26->panel_width != v26->board->panel_width ||
	     v26->panel_height != v26->board->panel_height)) {
		dev_err(v26->dev, "boot framebuffer %ux%u with %u notch rows is not the %ux%u %s panel\n",
			v26->fb_width, v26->fb_height, notch, v26->board->panel_width,
			v26->board->panel_height, v26->board->name);
		return -EINVAL;
	}
	return 0;
}

int iomfb_v26_6_bind(struct apple_dcp *dcp)
{
	struct apple_dcp_v26 *v26 = dcp->v26;
	struct device *dev = dcp->dev;
	struct apple_rtkit *rtk;
	u32 control, status;
	int ret;

	if (!v26)
		return -ENODEV;
	if (v26->rtk || dcp_v26_session)
		return dev_err_probe(dev, -EBUSY,
				     "%s display not started: an earlier DCP session is kept; reboot to restart the display\n",
				     v26->board->name);

	control = readl(dcp->coproc_reg + DCP_V26_CPU_CONTROL);
	status = readl(dcp->coproc_reg + DCP_V26_CPU_STATUS);
	dev_info(dev, "DCP CPU control %#x status %#x\n", control, status);
	if (!(control & APPLE_DCP_COPROC_CPU_CONTROL_RUN))
		return dev_err_probe(dev, -EBUSY,
				     "%s display not started: the DCP CPU is stopped, and interrupted firmware is never resumed\n",
				     v26->board->name);

	if (v26->board->external) {
		/* dispext1 framebuffer-width/height in the ADT */
		v26->fb_width = 4096;
		v26->fb_height = 2160;
		v26->stride = 4096 * 4;
		v26->panel_width = v26->fb_width;
		v26->panel_height = v26->fb_height;
		ret = 0;
	} else {
		ret = dcp_v26_geometry(v26);
	}
	if (ret)
		return dev_err_probe(dev, ret,
				     "%s display not started: no usable boot framebuffer\n",
				     v26->board->name);
	v26->clock_rate = v26->board->sdk27 ? 48000000 : apple_j613_25g83_clock_hz();
	if (!v26->clock_rate)
		return dev_err_probe(dev, -EINVAL, "J613/25G83 same-boot clock observation required\n");

	dcp_v26_link_init(&v26->link, dev, NULL);
	if (v26->board->sdk27)
		dcp_v26_link_set_abi(&v26->link, DCP_V27_READY_MSG, D(594), D(595));
	rtk = apple_rtkit_init(dev, v26, "mbox", 0, &dcp_v26_rtkit_ops);
	if (IS_ERR(rtk))
		return dev_err_probe(dev, PTR_ERR(rtk),
				     "%s display not started: RTKit init failed\n",
				     v26->board->name);
	/* From here nothing is freed and the module stays loaded. */
	v26->rtk = rtk;
	v26->link.rtk = rtk;
	dcp_v26_session = true;
	__module_get(THIS_MODULE);

	ret = apple_rtkit_wake(rtk);
	if (ret) {
		v26->failed = true;
		return dev_err_probe(dev, ret,
				     "%s display not started: the DCP RTKit session did not wake; the boot framebuffer stays\n",
				     v26->board->name);
	}
	dev_info(dev, "DCP RTKit session running\n");
	return 0;
}

void iomfb_v26_6_unbind(struct apple_dcp *dcp)
{
	struct apple_dcp_v26 *v26 = dcp->v26;

	dcp->active = false;
	dcp_mode_invalidate(&dcp->mode_state);
	if (!v26)
		return;
	dcp_v26_backlight_detach(v26);
	mutex_lock(&v26->lock);
	WRITE_ONCE(v26->dcp, NULL);
	mutex_unlock(&v26->lock);
	if (v26->rtk)
		dev_info(dcp->dev, "display unbound; the DCP session and its buffers are kept until reboot\n");
}

static int dcp_v26_status_show(struct seq_file *m, void *unused)
{
	struct apple_dcp_v26 *v26 = m->private;
	struct drm_framebuffer *fb;
	u32 i;
	int ret;

	ret = mutex_lock_interruptible(&v26->lock);
	if (ret)
		return ret;
	fb = v26->active_fb;
	seq_printf(m, "started %d\nfailed %d\nswaps %llu\nswap_max_us %llu\n",
		   v26->started, v26->failed, v26->swaps, v26->swap_ns_max / NSEC_PER_USEC);
	seq_printf(m, "active_fb %u\nimported %d\n", fb ? fb->base.id : 0,
		   fb && fb->obj[0]->import_attach);
	seq_printf(m, "panel %ux%u\nboot_fb %ux%u stride %u\nclock %llu\n",
		   v26->panel_width, v26->panel_height, v26->fb_width, v26->fb_height,
		   v26->stride, v26->clock_rate);
	seq_printf(m, "buffers %u bytes %llu\nproperties %u raw %u\nanalytics %llu\n",
		   v26->buffer_count, v26->buffer_bytes, v26->property_count, v26->raw_count,
		   v26->analytics);
	seq_printf(m, "publications %llu\n", v26->publications);
	for (i = 0; i < v26->published_count; i++)
		seq_printf(m, "published %u %*phN x%llu\n", v26->published[i].id,
			   (int)sizeof(v26->published[i].data), v26->published[i].data,
			   v26->published[i].count);
	seq_printf(m, "backlight %d max %u\nbrightness requested %u sent %u owned %d off %d pending %d swaps %llu\n",
		   !!READ_ONCE(v26->bl_dev), v26->bl_max, v26->bl_nits, v26->bl_sent,
		   v26->bl_owned, v26->bl_off, v26->bl_pending, v26->bl_swaps);
	seq_printf(m, "ctm valid %d calls %llu setter %#x getter %#x\n", v26->ctm_valid, v26->ctm_calls, v26->ctm_status, v26->ctm_get_status);
	for (i = 0; i < 9; i++)
		seq_printf(m, "ctm_readback[%u] %#llx\n", i, v26->ctm_readback[i]);
	mutex_unlock(&v26->lock);
	return 0;
}
DEFINE_SHOW_ATTRIBUTE(dcp_v26_status);

struct dcp_v26_present_snapshot {
	struct dcp_v26_present_record history[DCP_V26_HISTORY];
	u8 d590[2][DCP_V26_D590_SIZE];
	ktime_t d590_time[2];
	u64 history_count, d590_count;
	bool vrr;
};

static s64 dcp_v26_since(ktime_t t, ktime_t start)
{
	return t ? ktime_to_ns(ktime_sub(t, start)) : -1;
}

/*
 * The swap timeline, oldest first: times are ns after the swap start, -1
 * where the swap did not get that far. Then the last swap-complete payloads.
 */
static int dcp_v26_present_show(struct seq_file *m, void *unused)
{
	struct apple_dcp_v26 *v26 = m->private;
	struct dcp_v26_present_snapshot *snap;
	u64 first, i;
	int ret;

	snap = kvmalloc_obj(*snap);
	if (!snap)
		return -ENOMEM;
	ret = mutex_lock_interruptible(&v26->lock);
	if (ret) {
		kvfree(snap);
		return ret;
	}
	memcpy(snap->history, v26->history, sizeof(snap->history));
	memcpy(snap->d590, v26->d590, sizeof(snap->d590));
	memcpy(snap->d590_time, v26->d590_time, sizeof(snap->d590_time));
	snap->history_count = v26->history_count;
	snap->d590_count = v26->d590_count;
	snap->vrr = v26->vrr;
	mutex_unlock(&v26->lock);

	seq_printf(m, "vrr %d\nrefresh %u\ntimed %d\ntimestamps %d\nflip_timestamp %d\nswaps %llu\n",
		   snap->vrr, READ_ONCE(v26->refresh), READ_ONCE(v26->timed),
		   READ_ONCE(j613_25g83_swap_timestamps), READ_ONCE(j613_25g83_flip_timestamp),
		   snap->history_count);
	seq_puts(m, "# id surface timed start_ns submitted_ns replied_ns completed_ns\n");
	first = snap->history_count > DCP_V26_HISTORY ? snap->history_count - DCP_V26_HISTORY : 0;
	for (i = first; i < snap->history_count; i++) {
		struct dcp_v26_present_record *r = &snap->history[i % DCP_V26_HISTORY];

		seq_printf(m, "%u %u %u %lld %lld %lld %lld\n", r->id, r->surface, r->timed,
			   ktime_to_ns(r->start), dcp_v26_since(r->submitted, r->start),
			   dcp_v26_since(r->replied, r->start),
			   dcp_v26_since(r->completed, r->start));
	}
	first = snap->d590_count > 2 ? snap->d590_count - 2 : 0;
	for (i = first; i < snap->d590_count; i++) {
		seq_printf(m, "# swap-complete %llu received_ns %lld\n", i + 1,
			   ktime_to_ns(snap->d590_time[i % 2]));
		seq_hex_dump(m, "", DUMP_PREFIX_OFFSET, 16, 1, snap->d590[i % 2],
			     DCP_V26_D590_SIZE, false);
	}
	kvfree(snap);
	return 0;
}
DEFINE_SHOW_ATTRIBUTE(dcp_v26_present);

/*
 * The modes: the native timing of the boot panel, less the hidden notch rows,
 * and on the 120 Hz ProMotion panel also a locked 60 Hz mode.
 *
 * The locked mode is the same firmware timing with half the pixel clock, so
 * it reports 60 Hz. The firmware is never asked for a timing (see
 * iomfb_v26_6_modeset()): what differs is only that its swaps carry no
 * timing words, and the firmware shows untimed swaps at 60 Hz. Switching
 * between the two is a modeset in DRM terms and nothing more for the panel.
 * appledrm.j613_25g83_refresh picks which one is preferred.
 */
static int dcp_v26_mode(struct apple_dcp *dcp, struct apple_dcp_v26 *v26)
{
	struct dcp_display_mode *modes, *best = NULL, *preferred, *selected;
	struct dcp_parse_ctx ctx;
	unsigned int count, i;
	void *blob = NULL;
	u32 size = 0;
	int ret;

	for (i = 0; i < v26->raw_count; i++) {
		if (!v26->raw[i].service && !strcmp(v26->raw[i].key, "PreferredTimingElements")) {
			size = v26->raw[i].size;
			blob = v26->raw[i].data;
			break;
		}
	}
	if (!blob)
		return -ENODATA;
	ret = parse(blob, size, &ctx);
	if (ret)
		return ret;
	ctx.dcp = dcp;
	modes = enumerate_modes(&ctx, &count, dcp->panel.width_mm, dcp->panel.height_mm,
				dcp->notch_height, true, false);
	if (IS_ERR(modes))
		return PTR_ERR(modes);
	for (i = 0; i < count; i++) {
		struct drm_display_mode *m = &modes[i].mode;
		int hz = drm_mode_vrefresh(m);

		dev_dbg(dcp->dev, "timing %u/%u: " DRM_MODE_FMT "%s\n", i + 1, count,
			DRM_MODE_ARG(m), m->type & DRM_MODE_TYPE_PREFERRED ? " best" : "");
		if (m->hdisplay != v26->panel_width ||
		    m->vdisplay != v26->panel_height - dcp->notch_height ||
		    (hz != 60 && (hz != 120 || !v26->board->promotion)))
			continue;
		if (!best || (m->type & DRM_MODE_TYPE_PREFERRED))
			best = &modes[i];
	}
	if (!best) {
		dev_err(dcp->dev, "no %ux%u timing at 60%s Hz among %u\n", v26->panel_width,
			v26->panel_height - dcp->notch_height,
			v26->board->promotion ? " or 120" : "", count);
		kfree(modes);
		return -EINVAL;
	}
	/*
	 * The parser marks a ProMotion mode by its size and a sync rate of
	 * exactly 120 Hz; any 120 Hz mode of this built-in panel is one.
	 */
	v26->vrr = best->vrr || drm_mode_vrefresh(&best->mode) == 120;
	best->vrr = v26->vrr;
	count = v26->vrr ? 2 : 1;
	selected = kcalloc(count, sizeof(*best), GFP_KERNEL);
	if (!selected) {
		kfree(modes);
		return -ENOMEM;
	}
	selected[0] = *best;
	kfree(modes);
	preferred = &selected[0];
	if (v26->vrr) {
		struct dcp_display_mode *locked = &selected[1];

		*locked = selected[0];
		locked->vrr = false;
		locked->mode.clock = DIV_ROUND_CLOSEST(locked->mode.clock, 2);
		drm_mode_set_name(&locked->mode);
		if (j613_25g83_refresh == 60)
			preferred = locked;
		else if (j613_25g83_refresh != 120)
			dev_warn(dcp->dev, "appledrm.j613_25g83_refresh=%u is neither 60 nor 120; preferring 120 Hz\n",
				 j613_25g83_refresh);
	} else if (j613_25g83_refresh == 120 && v26->board->promotion) {
		dev_warn(dcp->dev, "the panel has no 120 Hz timing; only %d Hz is available\n",
			 drm_mode_vrefresh(&preferred->mode));
	}
	for (i = 0; i < count; i++)
		selected[i].mode.type &= ~DRM_MODE_TYPE_PREFERRED;
	preferred->mode.type |= DRM_MODE_TYPE_PREFERRED;
	mutex_lock(&dcp->modes_lock);
	kfree(dcp->modes);
	dcp->modes = selected;
	dcp->nr_modes = count;
	dcp->modes_admitted = true;
	mutex_unlock(&dcp->modes_lock);
	dev_info(dcp->dev, "modes: %s; preferred %d Hz (appledrm.j613_25g83_refresh)\n",
		 v26->vrr ? "120 Hz variable refresh and 60 Hz locked" : "one fixed timing",
		 drm_mode_vrefresh(&preferred->mode));
	return 0;
}

/*
 * External display: every timing the firmware offers, the preferred one first
 * choice. TimingElements is the display's whole catalog (all refresh rates);
 * PreferredTimingElements only the few the firmware prefers, all at 60 Hz on
 * the monitors seen so far. No timing means no display is attached (or the link
 * is not up yet).
 */
static int dcp_v26_mode_ext(struct apple_dcp *dcp, struct apple_dcp_v26 *v26)
{
	static const char * const catalogs[] = { "TimingElements", "PreferredTimingElements" };
	struct dcp_display_mode *modes, *best = NULL;
	struct dcp_parse_ctx ctx;
	unsigned int count = 0, i, c, max_hz = 0;
	const char *used = NULL;
	void *blob;
	u32 size;
	int ret = -ENODATA;

	modes = ERR_PTR(-ENODATA);
	for (c = 0; c < ARRAY_SIZE(catalogs); c++) {
		blob = NULL;
		size = 0;
		for (i = 0; i < v26->raw_count; i++) {
			if (!v26->raw[i].service && !strcmp(v26->raw[i].key, catalogs[c])) {
				size = v26->raw[i].size;
				blob = v26->raw[i].data;
				break;
			}
		}
		if (!blob)
			continue;
		ret = parse(blob, size, &ctx);
		if (ret)
			continue;
		ctx.dcp = dcp;
		/* Not the built-in panel: no ProMotion assumption for 120 Hz timings. */
		modes = enumerate_modes(&ctx, &count, 0, 0, 0, false, false);
		if (!IS_ERR(modes) && count) {
			used = catalogs[c];
			break;
		}
		if (!IS_ERR(modes))
			kfree(modes);
		ret = IS_ERR(modes) ? PTR_ERR(modes) : -ENODATA;
		modes = ERR_PTR(ret);
	}
	if (IS_ERR(modes))
		return PTR_ERR(modes);
	for (i = 0; i < count; i++) {
		struct drm_display_mode *m = &modes[i].mode;

		/*
		 * The swaps carry no ProMotion timing words for an external display,
		 * and adaptive sync is not programmed: every timing is fixed.
		 */
		modes[i].vrr = false;
		modes[i].min_vrr = 0;
		modes[i].max_vrr = 0;
		max_hz = max_t(unsigned int, max_hz, drm_mode_vrefresh(m));
		dev_dbg(dcp->dev, "ext timing %u/%u: " DRM_MODE_FMT " timing %u color %u%s\n",
			i + 1, count, DRM_MODE_ARG(m), modes[i].timing_mode_id,
			modes[i].color_mode_id,
			m->type & DRM_MODE_TYPE_PREFERRED ? " preferred" : "");
		if (!best || (m->type & DRM_MODE_TYPE_PREFERRED && !(best->mode.type & DRM_MODE_TYPE_PREFERRED)))
			best = &modes[i];
	}
	if (best)
		dev_info(dcp->dev, "ext timings: %u from %s, up to %u Hz; preferred " DRM_MODE_FMT "\n",
			 count, used, max_hz, DRM_MODE_ARG(&best->mode));
	if (!best) {
		kfree(modes);
		return -ENODATA;
	}
	v26->panel_width = best->mode.hdisplay;
	v26->panel_height = best->mode.vdisplay;
	mutex_lock(&dcp->modes_lock);
	kfree(dcp->modes);
	dcp->modes = modes;
	dcp->nr_modes = count;
	dcp->modes_admitted = true;
	mutex_unlock(&dcp->modes_lock);
	return 0;
}

/*
 * J704 external: the firmware reports the attached display with D575 after it
 * has published its TimingElements/PreferredTimingElements. Take the modes then,
 * and tell DRM, as the internal start does for the panel at session start.
 */
static void dcp_v26_ext_hotplug(struct work_struct *work)
{
	struct apple_dcp_v26 *v26 = container_of(work, struct apple_dcp_v26, ext_hotplug_work);
	struct apple_dcp *dcp = v26->dcp;
	struct apple_connector *conn;
	bool connected = READ_ONCE(v26->ext_hotplug_connected);
	int ret = 0;

	if (!dcp)
		return;
	if (connected) {
		mutex_lock(&v26->lock);
		ret = dcp_v26_mode_ext(dcp, v26);
		if (!ret && dcp->nr_modes) {
			const struct dcp_display_mode *m = &dcp->modes[0];
			struct {
				__le32 color_mode_id;
				__le32 timing_mode_id;
			} req;
			__le32 status = 0;
			unsigned int i;
			int dret, pret, mret;

			for (i = 0; i < dcp->nr_modes; i++) {
				if (dcp->modes[i].mode.type & DRM_MODE_TYPE_PREFERRED) {
					m = &dcp->modes[i];
					break;
				}
			}
			/*
			 * SDK 27 external power-on, as the Asahi template powers a
			 * non-main display: set_display_device(2) (H17P A411, same
			 * number in 27), set_power_state {1} (H17P A473 -> SDK 27 A484,
			 * 0x10 in, 0x8 out), then set_digital_out_mode A413 {color,
			 * timing}. (A478 is flush_supports_power, already sent at boot.)
			 */
			{
				__le32 dev_in = cpu_to_le32(2), dev_out = 0;
				struct {
					__le64 unklong;
					__le32 unkint;
					u8 unkint_null;
					u8 pad[3];
				} __packed pwr = { .unklong = cpu_to_le64(1) };
				u8 pwr_out[8] = {};

				dret = dcp_v26_call(v26, DCP_V26_TAG('A', 411), &dev_in, 4,
						    &dev_out, 4, 0);
				pret = dret ? dret : dcp_v26_call(v26, DCP_V26_TAG('A', 484), &pwr,
								  sizeof(pwr), pwr_out,
								  sizeof(pwr_out), 0);
				dev_info(dcp->dev, "J704 ext power: A411(2) %d -> %#x, A484(1) %d -> %*ph\n",
					 dret, le32_to_cpu(dev_out), pret, 8, pwr_out);
			}
			req.color_mode_id = cpu_to_le32(m->color_mode_id);
			req.timing_mode_id = cpu_to_le32(m->timing_mode_id);
			mret = pret ? pret : dcp_v26_call(v26, DCP_V26_TAG('A', 413), &req, sizeof(req),
							  &status, sizeof(status), 0);
			dev_info(dcp->dev, "J704 ext mode " DRM_MODE_FMT ": A413(color %u timing %u) %d status %#x\n",
				 DRM_MODE_ARG(&m->mode), m->color_mode_id, m->timing_mode_id, mret,
				 le32_to_cpu(status));
			if (!mret && !le32_to_cpu(status) && READ_ONCE(v26->ext_hotplug_connected)) {
				v26->ext_timing_id = m->timing_mode_id;
				v26->ext_color_id = m->color_mode_id;
				WRITE_ONCE(v26->ext_pipe_on, true);
			}
		}
		mutex_unlock(&v26->lock);
		if (!ret) {
			dcp_set_dimensions(dcp, dcp_modes_transfer_begin(dcp));
			dcp->active = true;
		}
	}
	/*
	 * A display whose mode could not be set (the session may even have
	 * failed) is not offered to userspace: a compositor would retry its
	 * commits on it for ever.
	 */
	if (connected && !ret && !READ_ONCE(v26->ext_pipe_on))
		ret = -EIO;
	conn = dcp->typec_connector ?: dcp->connector;
	dev_info(dcp->dev, "J704 ext hotplug: connected %d, modes %u (%d), connector %s\n",
		 connected, dcp->nr_modes, ret, conn ? conn->base.name : "none");
	if (!conn)
		return;
	WRITE_ONCE(conn->connected, connected && !ret);
	schedule_work(&conn->hotplug_wq);
	/* The HDMI display (not a Type-C one) went away: the pipeline is free. */
	if (!connected && conn == dcp->fixed_connector)
		dcp_v26_ext_hdmi_gone(dcp);
}

int iomfb_v26_6_start(struct apple_dcp *dcp)
{
	struct apple_dcp_v26 *v26 = dcp->v26;
	const struct drm_display_mode *mode;
	const char *step;
	int ret;

	if (!v26 || !v26->rtk || v26->failed || v26->started)
		return -ENODEV;

	if (v26->board->external) {
		/* AFK endpoints share this RTKit session; start them before IOMFB. */
		dcp->rtk = v26->rtk;
		ret = dcp_v26_ext_endpoints(dcp);
		dev_info(dcp->dev, "%s: AFK endpoints started before IOMFB: %d\n",
			 v26->board->name, ret);
	}

	step = "DCPLink";
	ret = dcp_v26_link_start(&v26->link);
	if (ret)
		goto fail;

	mutex_lock(&v26->lock);
	step = "start signal";
	ret = dcp_v26_simple_call(v26, A(401), false, true, true);
	if (!ret && v26->board->sdk27) {
		/* 25G83 A457 first_client_open (0/0) is A466 on SDK 27. */
		step = "first client open";
		if (READ_ONCE(j704_dcp_client_open))
			ret = dcp_v27_call(v26, 466, false, false, NULL);
	} else if (!ret) {
		step = "first client open";
		ret = dcp_v26_simple_call(v26, A(457), false, false, false);
	}
	mutex_unlock(&v26->lock);
	if (ret)
		goto fail;
	dcp_v26_link_set_idle_work(&v26->link, &v26->idle_work);

	if (v26->board->external) {
		dcp_v26_ext_wait_ports(dcp);
		dcp_v26_ext_connect_debugfs(dcp);
	}

	step = "panel mode";
	mutex_lock(&v26->lock);
	ret = v26->board->external ? dcp_v26_mode_ext(dcp, v26) : dcp_v26_mode(dcp, v26);
	mutex_unlock(&v26->lock);
	if (ret == -ENODATA && v26->board->external) {
		/* The session runs; the display (if any) has not been reported. */
		v26->started = true;
		debugfs_create_file("dcp-j704-ext", 0400, NULL, v26, &dcp_v26_status_fops);
		if (dcp->connector)
			dcp->connector->connected = false;
		complete(&dcp->start_done);
		dev_info(dcp->dev, "%s session started, no display timing reported\n",
			 v26->board->name);
		dcp_v26_ext_hdmi_start(dcp);
		return 0;
	}
	if (ret)
		goto fail;

	/* Nits; the backlight device appears with the first frame. */
	v26->bl_max = min_t(u32, dcp->brightness.maximum, v26->board->max_nits);
	v26->bl_nits = min_t(u32, DCP_V26_DEFAULT_NITS, v26->bl_max);
	v26->started = true;
	/* Never removed, like the session it describes. */
	debugfs_create_file(v26->board->external ? "dcp-j704-ext" :
			    v26->board->sdk27 ? "dcp-j704" : "dcp-j613_25g83", 0400, NULL, v26,
			    &dcp_v26_status_fops);
	debugfs_create_file(v26->board->external ? "dcp-j704-ext-present" :
			    v26->board->sdk27 ? "dcp-j704-present" : "dcp-j613_25g83-present",
			    0400, NULL, v26, &dcp_v26_present_fops);
	dcp->connector->connected = true;
	dcp_set_dimensions(dcp, dcp_modes_transfer_begin(dcp));
	dcp_mode_set_valid(&dcp->mode_state, true);
	dcp->active = true;
	complete(&dcp->start_done);
	if (v26->board->external)
		dcp_v26_ext_hdmi_start(dcp);

	mode = &dcp->modes[0].mode;
	dev_info(dcp->dev, "%s display started: %ux%u@%d, %u notch rows %s, %ux%u mm\n",
		 v26->board->name, mode->hdisplay, mode->vdisplay, drm_mode_vrefresh(mode),
		 v26->notch_rows,
		 dcp->notch_height ? "hidden" : "shown", mode->width_mm, mode->height_mm);
	return 0;
fail:
	v26->failed = true;
	dev_err(dcp->dev,
		"%s display not started: %s failed: %d; the boot framebuffer stays (reboot to retry)\n",
		v26->board->name, step, ret);
	return ret;
}

/*
 * Panel brightness travels in the swap record, in nits.
 *
 * Until the first request the firmware keeps the brightness iBoot set, and
 * no swap touches it. From then on the driver sets it: a request, the CRTC
 * going off (0 nits) and the CRTC coming back (the requested nits) each make
 * the target pending, and the next swap carries it. A request waits a frame
 * for a flip to carry it, then makes a swap of its own (see
 * dcp_v26_brightness_commit()).
 */

/* Called with the lock held. */
static void dcp_v26_brightness_changed(struct apple_dcp_v26 *v26)
{
	lockdep_assert_held(&v26->lock);
	if (!v26->bl_owned)
		return;
	WRITE_ONCE(v26->bl_pending, true);
	v26->bl_seq++;
}

/* Adds a pending target to @swap; returns the token dcp_v26_brightness_done() takes. */
static u64 dcp_v26_brightness_encode(struct apple_dcp_v26 *v26, u8 *swap)
{
	lockdep_assert_held(&v26->lock);
	if (!v26->bl_pending)
		return 0;
	if (v26->board->sdk27)
		dcp_v27_encode_brightness(swap, v26->bl_off ? 0 : v26->bl_nits, v26->bl_max);
	else
		dcp_v26_encode_brightness(swap, v26->bl_off ? 0 : v26->bl_nits);
	return v26->bl_seq;
}

/* A swap completed; @token is what it carried. Called with the lock held. */
static void dcp_v26_brightness_done(struct apple_dcp_v26 *v26, u64 token)
{
	lockdep_assert_held(&v26->lock);
	/* The first frame is on the panel: KMS is up, so is the backlight now. */
	if (!v26->bl_queued && v26->bl_max) {
		v26->bl_queued = true;
		schedule_work(&v26->bl_register_work);
	}
	if (!token)
		return;
	v26->bl_swaps++;
	/* A change made since then is still pending. */
	if (token != v26->bl_seq)
		return;
	v26->bl_sent = v26->bl_off ? 0 : v26->bl_nits;
	WRITE_ONCE(v26->bl_pending, false);
	wake_up_all(&v26->bl_wait);
}

/*
 * Swap start, then the swap; returns once the firmware completed that swap,
 * with @completed set to when its completion came in.
 */
/* Matched UUID ABI: location 9, packed 80-byte request, signed Q32. */
static int dcp_v26_ctm_locked(struct apple_dcp_v26 *v26,
			  const struct drm_crtc_state *state)
{
	u8 request[80] = {}, readback[76] = {};
	__le32 status = 0;
	const struct drm_color_ctm *ctm;
	int i, ret;

	if (!state || (v26->ctm_valid && !state->color_mgmt_changed &&
		       !state->mode_changed && !state->active_changed))
		return 0;
	ctm = state->ctm ? state->ctm->data : NULL;
	put_unaligned_le32(9, request);
	for (i = 0; i < 9; i++) {
		u64 coefficient = ctm ? ctm->matrix[i] :
				  ((i == 0 || i == 4 || i == 8) ? 1ULL << 32 : 0);
		u64 magnitude = coefficient & ~(1ULL << 63);
		u64 signed_q32 = coefficient & (1ULL << 63) ? -magnitude : magnitude;

		put_unaligned_le64(signed_q32, request + 4 + 8 * i);
	}
	ret = dcp_v26_call(v26, A(423), request, sizeof(request),
		       &status, sizeof(status), 0);
	v26->ctm_calls++;
	v26->ctm_status = le32_to_cpu(status);
	v26->ctm_valid = false;
	if (ret || v26->ctm_status) {
		dev_err(v26->dev, "CTM setter transport=%d status=%#x\n",
			ret, v26->ctm_status);
		return ret ? ret : -EIO;
	}
	/* Independent firmware getter, before the swap and on the same lock. */
	memset(request + 4, 0, 76);
	ret = dcp_v26_call(v26, A(422), request, sizeof(request),
		       readback, sizeof(readback), 0);
	v26->ctm_get_status = get_unaligned_le32(readback + 72);
	for (i = 0; i < 9; i++)
		v26->ctm_readback[i] = get_unaligned_le64(readback + i * 8);
	dev_info(v26->dev, "CTM call %llu setter=%#x getter_transport=%d getter=%#x diagonal=%#llx,%#llx,%#llx\n",
		 v26->ctm_calls, v26->ctm_status, ret, v26->ctm_get_status,
		 v26->ctm_readback[0], v26->ctm_readback[4], v26->ctm_readback[8]);
	if (ret || v26->ctm_get_status)
		return ret ? ret : -EIO;
	v26->ctm_valid = true;
	return 0;
}

static int dcp_v26_swap(struct apple_dcp_v26 *v26, const u8 *surface, u64 iova,
			u32 width, u32 height, u32 dst_y, ktime_t *completed,
			const struct drm_crtc_state *ctm_state)
{
	struct dcp_v26_present_record rec = {};
	__le32 start[4] = {}, started[2] = {}, result[3];
	u64 brightness = 0;
	u8 *swap;
	u32 id;
	int ret;

	if (surface && (!iova || !width || width > v26->panel_width || !height ||
			dst_y > v26->panel_height || height > v26->panel_height - dst_y))
		return -EINVAL;
	swap = kmalloc(max(DCP_V26_SWAP_SIZE, DCP_V27_SUBMIT_SIZE), GFP_KERNEL);
	if (!swap)
		return -ENOMEM;

	mutex_lock(&v26->lock);
	/* An external swap is only sent, and only waited for, while the pipe is on. */
	if (v26->board->external)
		v26->link.submit_gate = &v26->ext_pipe_on;
	rec.surface = !!surface;
	rec.start = ktime_get();
	/* The SDK-27 colour-matrix methods are not qualified: identity stays. */
	ret = v26->board->sdk27 ? 0 : dcp_v26_ctm_locked(v26, ctm_state);
	if (!ret)
	ret = dcp_v26_call(v26, A(406), start, sizeof(start), started, sizeof(started), 0);
	if (!ret && le32_to_cpu(started[1]))
		ret = -EIO;
	id = le32_to_cpu(started[0]);
	if (!ret && !id)
		ret = -EPROTO;
	if (!ret) {
		if (v26->board->sdk27)
			dcp_v27_encode_swap(swap, id, DCP_V26_BLACK, surface, iova, width, height,
					    dst_y, min_t(u32, READ_ONCE(j704_dcp_layer),
							 DCP_V27_LAYERS - 1));
		else
			dcp_v26_encode_swap(swap, id, DCP_V26_BLACK, surface, iova, width,
					    height, dst_y, DCP_V26_LAYER);
		/* A target long past: show it at the next refresh, as on M1/M2. */
		if (surface && READ_ONCE(v26->timed) && READ_ONCE(j613_25g83_swap_timestamps)) {
			dcp_v26_swap_set_timing(swap, 120);
			rec.timed = 1;
		}
		rec.id = id;
		rec.submitted = ktime_get();
		brightness = dcp_v26_brightness_encode(v26, swap);
		if (v26->board->sdk27)
			ret = dcp_v26_call(v26, A(408), swap, DCP_V27_SUBMIT_SIZE, result,
					   sizeof(result), id);
		else
			ret = dcp_v26_call(v26, A(407), swap, DCP_V26_SWAP_SIZE, result,
					   sizeof(result), id);
		if (v26->board->sdk27 && v26->swaps < 3)
			dev_info(v26->dev, "IOMFB27 swap %u A408 ret=%d status=%#x\n", id, ret,
				 get_unaligned_le32((u8 *)result + 5));
		/* The swap status sits at byte 5 of the reply. */
		if (!ret && get_unaligned_le32((u8 *)result + 5))
			ret = -EIO;
	}
	if (ret == -EREMOTEIO && brightness && v26->board->sdk27) {
		/*
		 * The firmware refused a swap that carried a brightness change: the SDK-27
		 * brightness block is not qualified, so stop sending it and keep the session.
		 */
		dev_warn(v26->dev, "IOMFB27 swap %u with brightness %u refused (status %#x); brightness changes disabled\n",
			 id, v26->bl_nits, get_unaligned_le32((u8 *)result + 5));
		v26->bl_refused = true;
		v26->bl_owned = false;
		WRITE_ONCE(v26->bl_pending, false);
		wake_up_all(&v26->bl_wait);
	} else if (ret == -ENOLINK && v26->board->external) {
		/*
		 * Unplugged while in flight (answered but never completed, swallowed
		 * without an answer, or not sent at all): the frame is dropped and the
		 * session goes on.
		 */
		dev_info(v26->dev, "IOMFB27 swap %u dropped (display gone)\n", id);
		WRITE_ONCE(v26->ext_pipe_on, false);
		*completed = ktime_get();
		ret = 0;
	} else if (ret) {
		v26->failed = true;
	} else {
		dcp_v26_brightness_done(v26, brightness);
		rec.replied = v26->link.reply_time;
		rec.completed = v26->link.completion_time;
		*completed = rec.completed;
	}
	v26->history[v26->history_count++ % DCP_V26_HISTORY] = rec;
	v26->link.submit_gate = NULL;
	mutex_unlock(&v26->lock);

	kfree(swap);
	return ret;
}

/*
 * Shows @fb (or only the black background) and keeps it until the next swap.
 * @presented, if set, gets the time the firmware reported the swap complete.
 */
static bool dcp_v26_present(struct apple_dcp_v26 *v26, struct drm_framebuffer *fb, u32 dst_y,
			    ktime_t *presented, const struct drm_crtc_state *ctm_state)
{
	u8 surface[DCP_V26_SURFACE_SIZE];
	struct drm_framebuffer *old;
	u64 iova = 0, start, elapsed, swaps;
	ktime_t completed;
	int ret;

	if (READ_ONCE(v26->failed))
		return false;
	if (v26->board->external && !READ_ONCE(v26->ext_pipe_on)) {
		/* No display on the pipe: keep the frame in software only. */
		if (fb)
			drm_framebuffer_get(fb);
		mutex_lock(&v26->lock);
		old = v26->active_fb;
		v26->active_fb = fb;
		mutex_unlock(&v26->lock);
		if (old)
			drm_framebuffer_put(old);
		if (presented)
			*presented = ktime_get();
		return true;
	}
	if (fb) {
		drm_framebuffer_get(fb);
		dcp_v26_encode_surface(surface, fb->pitches[0], fb->width, fb->height,
				       fb->format->format == DRM_FORMAT_XRGB8888);
		iova = drm_fb_dma_get_gem_obj(fb, 0)->dma_addr;
	}
	start = ktime_get_ns();
	ret = dcp_v26_swap(v26, fb ? surface : NULL, iova, fb ? fb->width : 0,
			   fb ? fb->height : 0, dst_y, &completed, ctm_state);
	elapsed = ktime_get_ns() - start;
	if (ret) {
		/* Either framebuffer may still be scanned out: keep both. */
		dev_err(v26->dev, "native DCP flip failed %d; buffers pinned until reboot\n", ret);
		return false;
	}
	mutex_lock(&v26->lock);
	old = v26->active_fb;
	v26->active_fb = fb;
	swaps = ++v26->swaps;
	v26->swap_ns_max = max(v26->swap_ns_max, elapsed);
	mutex_unlock(&v26->lock);
	if (old)
		drm_framebuffer_put(old);
	if (swaps <= 3)
		dev_info(v26->dev, "swap %llu complete: surface %d, imported %d, %llu us\n",
			 swaps, !!fb, fb && fb->obj[0]->import_attach, elapsed / NSEC_PER_USEC);
	if (presented)
		*presented = completed;
	return true;
}

/* Releases helper waiters without reporting a completed flip. */
static void dcp_v26_cancel_event(struct apple_dcp *dcp)
{
	struct apple_crtc *crtc = dcp->crtc;
	struct drm_pending_vblank_event *event;
	unsigned long flags;

	spin_lock_irqsave(&crtc->base.dev->event_lock, flags);
	event = crtc->event;
	crtc->event = NULL;
	spin_unlock_irqrestore(&crtc->base.dev->event_lock, flags);
	if (!event)
		return;
	if (event->base.fence) {
		dma_fence_set_error(event->base.fence, -EIO);
		dma_fence_signal(event->base.fence);
	}
	if (event->base.completion) {
		complete_all(event->base.completion);
		if (event->base.completion_release)
			event->base.completion_release(event->base.completion);
		event->base.completion = NULL;
	}
	drm_event_cancel_free(crtc->base.dev, &event->base);
}

/* Only the layout the firmware was qualified with: one full-mode linear plane. */
int iomfb_v26_6_atomic_check(struct apple_dcp *dcp, struct drm_crtc *crtc,
			     struct drm_atomic_commit *state)
{
	struct apple_dcp_v26 *v26 = dcp->v26;
	struct drm_crtc_state *crtc_state = drm_atomic_get_new_crtc_state(state, crtc);
	struct drm_plane_state *p = drm_atomic_get_new_plane_state(state, crtc->primary);
	struct drm_gem_dma_object *obj;
	struct drm_framebuffer *fb;
	u32 width, height;

	/* Turning the CRTC off needs nothing from the DCP (system suspend does it). */
	if (crtc_state && !crtc_state->active)
		return 0;
	if (dcp->crashed || !v26 || READ_ONCE(v26->failed))
		return -EIO;
	if (!crtc_state || !p || p->crtc != crtc || !p->visible)
		return 0;

	fb = p->fb;
	width = crtc_state->mode.hdisplay;
	height = crtc_state->mode.vdisplay;
	if (fb->format->format != DRM_FORMAT_XRGB8888 &&
	    fb->format->format != DRM_FORMAT_ARGB8888)
		return -EINVAL;
	if (fb->modifier != DRM_FORMAT_MOD_LINEAR || fb->offsets[0] ||
	    fb->pitches[0] < width * 4 || (fb->pitches[0] & 63))
		return -EINVAL;
	if (p->src_x || p->src_y || p->crtc_x || p->crtc_y ||
	    p->src_w != width << 16 || p->src_h != height << 16 ||
	    p->crtc_w != width || p->crtc_h != height ||
	    fb->width != width || fb->height != height)
		return -EINVAL;
	obj = drm_fb_dma_get_gem_obj(fb, 0);
	if (!obj || !obj->dma_addr || (u64)fb->pitches[0] * height > obj->base.size)
		return -EINVAL;
	return 0;
}

/* J704 external: switch the powered pipe to @mode's timing/color pair. */
static int dcp_v26_ext_set_mode(struct apple_dcp_v26 *v26, const struct dcp_display_mode *mode)
{
	struct {
		__le32 color_mode_id;
		__le32 timing_mode_id;
	} req = {
		.color_mode_id = cpu_to_le32(mode->color_mode_id),
		.timing_mode_id = cpu_to_le32(mode->timing_mode_id),
	};
	__le32 status = 0;
	int ret = 0;

	mutex_lock(&v26->lock);
	/* Nothing to do while unplugged; the next hotplug sets the preferred timing. */
	if (!READ_ONCE(v26->ext_pipe_on) ||
	    (v26->ext_timing_id == mode->timing_mode_id &&
	     v26->ext_color_id == mode->color_mode_id))
		goto out;
	ret = dcp_v26_call(v26, DCP_V26_TAG('A', 413), &req, sizeof(req), &status,
			   sizeof(status), 0);
	dev_info(v26->dev, "J704 ext mode " DRM_MODE_FMT ": A413(color %u timing %u) %d status %#x\n",
		 DRM_MODE_ARG(&mode->mode), mode->color_mode_id, mode->timing_mode_id, ret,
		 le32_to_cpu(status));
	if (!ret && le32_to_cpu(status))
		ret = -EIO;
	if (!ret) {
		v26->ext_timing_id = mode->timing_mode_id;
		v26->ext_color_id = mode->color_mode_id;
		v26->panel_width = mode->mode.hdisplay;
		v26->panel_height = mode->mode.vdisplay;
	}
out:
	mutex_unlock(&v26->lock);
	return ret;
}

int iomfb_v26_6_modeset(struct apple_dcp *dcp, struct drm_crtc_state *crtc_state)
{
	struct dcp_display_mode mode;

	if (!lookup_mode(dcp, &crtc_state->mode, &mode))
		return -EINVAL;
	/*
	 * The firmware keeps the timing it booted with, which every mode
	 * shares: nothing is sent to it. The mode only decides whether the
	 * swaps that follow carry timing words (120 Hz ProMotion) or not
	 * (locked 60 Hz).
	 */
	if (dcp->v26) {
		WRITE_ONCE(dcp->v26->timed, mode.vrr);
		WRITE_ONCE(dcp->v26->refresh, drm_mode_vrefresh(&mode.mode));
	}
	/*
	 * An external display does change timing: the hotplug set the preferred
	 * one with A413; a different mode chosen by userspace (another refresh
	 * rate or size) is set the same way, on the pipe that is already on.
	 */
	if (dcp->v26 && dcp->v26->board->external) {
		int ret = dcp_v26_ext_set_mode(dcp->v26, &mode);

		if (ret)
			return ret;
	}
	dcp_mode_set_valid(&dcp->mode_state, true);
	return 0;
}

void iomfb_v26_6_flush(struct apple_dcp *dcp, struct drm_crtc *crtc,
		       struct drm_atomic_commit *state)
{
	struct drm_plane_state *p = drm_atomic_get_new_plane_state(state, crtc->primary);
	struct drm_framebuffer *fb;
	ktime_t presented;
	struct drm_crtc_state *cs = drm_atomic_get_new_crtc_state(state, crtc);

	if (!cs)
		cs = crtc->state;
	if (!p)
		p = crtc->primary->state;
	fb = p && p->visible ? p->fb : NULL;
	dcp->swap_start = ktime_get();
	if (!dcp->v26 || !dcp_v26_present(dcp->v26, fb, dcp->notch_height, &presented, cs)) {
		dcp_v26_cancel_event(dcp);
		return;
	}
	/*
	 * This client holds the framebuffer it shows itself (active_fb) and the swap above
	 * completed, so nothing displaced by this or an earlier commit is scanned out any
	 * more. Nothing here arms the retained-framebuffer list the commit tail fills, so
	 * release it now: J704 kept one framebuffer reference per flip, 154708 on one
	 * framebuffer after 309k swaps (asahi-pw57-boot).
	 */
	dcp_release_all_retained_framebuffers(dcp);
	if (READ_ONCE(j613_25g83_flip_timestamp)) {
		/*
		 * The flip completed when the firmware's swap-complete message
		 * came in, not when this worker got to run: stamp the event with
		 * that time as it is (no swap_start correction).
		 */
		dcp->swap_start = KTIME_MIN;
		dcp_drm_crtc_page_flip(dcp, presented);
	} else {
		dcp_drm_crtc_page_flip(dcp, ktime_get());
	}
}

/*
 * Makes a swap for a pending brightness: a commit of the CRTC alone, which
 * shows the plane as it is. Nothing is done while the CRTC is off (enabling
 * it sends the brightness) or before its first modeset (the first frame
 * sends it), nor when a flip carried it in the meantime.
 */
static int dcp_v26_brightness_commit(struct apple_dcp_v26 *v26)
{
	struct apple_dcp *dcp = READ_ONCE(v26->dcp);
	struct drm_modeset_acquire_ctx ctx;
	struct drm_crtc_state *crtc_state;
	struct drm_atomic_commit *state;
	struct drm_crtc *crtc;
	int ret;

	if (!dcp || !dcp->crtc)
		return 0;
	crtc = &dcp->crtc->base;
	state = drm_atomic_commit_alloc(crtc->dev);
	if (!state)
		return -ENOMEM;
	drm_modeset_acquire_init(&ctx, DRM_MODESET_ACQUIRE_INTERRUPTIBLE);
	state->acquire_ctx = &ctx;
retry:
	ret = drm_modeset_lock(&crtc->mutex, &ctx);
	if (ret)
		goto out;
	if (!crtc->state || !crtc->state->active || !READ_ONCE(v26->bl_pending))
		goto out;
	crtc_state = drm_atomic_get_crtc_state(state, crtc);
	if (IS_ERR(crtc_state)) {
		ret = PTR_ERR(crtc_state);
		goto out;
	}
	ret = drm_atomic_commit(state);
out:
	if (ret == -EDEADLK) {
		drm_atomic_commit_clear(state);
		ret = drm_modeset_backoff(&ctx);
		if (!ret)
			goto retry;
	}
	drm_atomic_commit_put(state);
	drm_modeset_drop_locks(&ctx);
	drm_modeset_acquire_fini(&ctx);
	return ret;
}

static int dcp_v26_backlight_update(struct backlight_device *bd)
{
	struct apple_dcp_v26 *v26 = bl_get_data(bd);
	int ret = 0;

	mutex_lock(&v26->lock);
	if (v26->failed || v26->bl_refused) {
		ret = -EIO;
	} else {
		/* Sent even when unchanged: it also repairs the firmware's value. */
		v26->bl_nits = backlight_get_brightness(bd);
		v26->bl_owned = true;
		dcp_v26_brightness_changed(v26);
	}
	mutex_unlock(&v26->lock);
	if (ret)
		return ret;

	if (wait_event_timeout(v26->bl_wait, !READ_ONCE(v26->bl_pending), DCP_V26_BL_FLIP_WAIT))
		return 0;
	return dcp_v26_brightness_commit(v26);
}

static const struct backlight_ops dcp_v26_backlight_ops = {
	.update_status = dcp_v26_backlight_update,
};

/* Nits, like the M1/M2 panels: 0 to the DT maximum, linear in luminance. */
static void dcp_v26_backlight_register(struct work_struct *work)
{
	struct apple_dcp_v26 *v26 = container_of(work, struct apple_dcp_v26, bl_register_work);
	struct backlight_properties props = {
		.type = BACKLIGHT_PLATFORM,
		.scale = BACKLIGHT_SCALE_LINEAR,
		.max_brightness = v26->bl_max,
		.brightness = min_t(u32, DCP_V26_DEFAULT_NITS, v26->bl_max),
	};
	struct backlight_device *bd;

	if (v26->board->external)
		return;
	mutex_lock(&v26->bl_lock);
	if (!v26->bl_detached && !v26->bl_dev) {
		bd = backlight_device_register("apple-panel-bl", v26->dev, v26,
					       &dcp_v26_backlight_ops, &props);
		if (IS_ERR(bd))
			dev_err(v26->dev, "no panel backlight device: %pe\n", bd);
		else
			WRITE_ONCE(v26->bl_dev, bd);
	}
	mutex_unlock(&v26->bl_lock);
}

/* KMS goes away: so does the backlight device, before the CRTC it commits to. */
static void dcp_v26_backlight_detach(struct apple_dcp_v26 *v26)
{
	struct backlight_device *bd;

	mutex_lock(&v26->bl_lock);
	v26->bl_detached = true;
	bd = v26->bl_dev;
	WRITE_ONCE(v26->bl_dev, NULL);
	mutex_unlock(&v26->bl_lock);
	cancel_work_sync(&v26->bl_register_work);
	backlight_device_unregister(bd);
}

/* 0 nits while the CRTC is off, the requested nits once it is on again. */
static void dcp_v26_brightness_power(struct apple_dcp_v26 *v26, bool on)
{
	mutex_lock(&v26->lock);
	v26->bl_off = !on;
	dcp_v26_brightness_changed(v26);
	mutex_unlock(&v26->lock);
}

void iomfb_v26_6_poweron(struct apple_dcp *dcp)
{
	/* The panel stays on; the next swap shows the plane again. */
	if (dcp->v26 && dcp->v26->started)
		dcp_v26_brightness_power(dcp->v26, true);
}

void iomfb_v26_6_poweroff(struct apple_dcp *dcp)
{
	/* Blank to black; the panel and the DCP stay powered. */
	if (dcp->v26 && dcp->v26->started) {
		dcp_v26_brightness_power(dcp->v26, false);
		if (dcp_v26_present(dcp->v26, NULL, 0, NULL, NULL))
			dcp_release_all_retained_framebuffers(dcp);
	}
}
