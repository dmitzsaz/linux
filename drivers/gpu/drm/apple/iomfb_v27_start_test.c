// SPDX-License-Identifier: GPL-2.0-only OR MIT

#include <kunit/device.h>
#include <kunit/test.h>
#include <linux/module.h>
#include <linux/unaligned.h>

#include <drm/drm_device.h>
#include <drm/drm_fourcc.h>
#include <drm/drm_framebuffer.h>

#include "dcp.h"
#include "dcp-internal.h"
#include "iomfb_v27.h"

struct v27_start_test {
	struct apple_dcp dcp;
	struct kunit *test;
	u8 result;
	bool fail_start, fail_open;
	unsigned int calls;
	u64 expected_deadline;
};

enum v27_external_event {
	V27_EXT_NONE, V27_EXT_BOUNCE, V27_EXT_LOW, V27_EXT_DETACH,
	V27_EXT_CATALOG, V27_EXT_SUSPEND,
};

struct v27_external_test {
	struct apple_dcp dcp;
	struct apple_connector connector;
	struct dcp_display_mode modes[3];
	struct kunit *test;
	enum v27_external_event event;
	u32 status;
	unsigned int calls;
	atomic_t stops;
};

static void v27_external_noop(struct work_struct *work)
{
}

static void v27_external_stop(struct work_struct *work)
{
	struct apple_dcp *dcp = container_of(work, struct apple_dcp, external_stop_work);
	struct v27_external_test *t = container_of(dcp, struct v27_external_test, dcp);

	atomic_inc(&t->stops);
}

static void v27_external_reply(struct apple_dcp *dcp,
			       const struct dcp_method_entry *call,
			       dcp_callback_t callback, void *cookie)
{
	struct v27_external_test *t = container_of(dcp, struct v27_external_test, dcp);

	KUNIT_ASSERT_EQ(t->test, memcmp(call->tag, "A413", 4), 0);
	KUNIT_EXPECT_TRUE(t->test, dcp->mode_state.changing);
	t->calls++;
	scoped_guard(mutex, &dcp->iomfb.lock) {
		if (t->event == V27_EXT_BOUNCE || t->event == V27_EXT_LOW) {
			/* The fixed-sink trampoline calls this before hotplug. */
			if (dcp->fixed_phy)
				dcp_modes_begin_attachment(dcp);
			KUNIT_EXPECT_TRUE(t->test, dcp_external_hotplug(dcp, false));
			KUNIT_EXPECT_FALSE(t->test, work_pending(&dcp->external_stop_work));
			if (t->event == V27_EXT_BOUNCE)
				KUNIT_EXPECT_TRUE(t->test, dcp_external_hotplug(dcp, true));
		}
		if (t->event == V27_EXT_DETACH)
			WRITE_ONCE(dcp->external_physically_absent, true);
		if (t->event == V27_EXT_CATALOG) {
			scoped_guard(mutex, &dcp->modes_lock)
				dcp->modes_generation++;
		}
		if (t->event == V27_EXT_SUSPEND)
			WRITE_ONCE(dcp->typec_work_enabled, false);
	}
	callback(dcp, &t->status, cookie);
}

static void v27_external_cleanup(void *data)
{
	struct v27_external_test *t = data;

	cancel_work_sync(&t->dcp.external_stop_work);
	cancel_work_sync(&t->dcp.vblank_wq);
	cancel_work_sync(&t->connector.hotplug_wq);
}

static struct v27_external_test *v27_external_setup(struct kunit *test, bool fixed)
{
	struct v27_external_test *t = kunit_kzalloc(test, sizeof(*t), GFP_KERNEL);
	struct apple_dcp *dcp;
	unsigned int i;
	char name[48];

	KUNIT_ASSERT_NOT_NULL(test, t);
	t->test = test;
	dcp = &t->dcp;
	snprintf(name, sizeof(name), "v27-external-%lx", (unsigned long)t);
	dcp->dev = kunit_device_register(test, name);
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, dcp->dev);
	dcp->fw_compat = DCP_FIRMWARE_V_27_0;
	dcp->hw.iomfb_method_profile = DCP_IOMFB_METHODS_H17P_EXTERNAL;
	dcp->test_push = v27_external_reply;
	dcp->iomfb.owner = current;
	mutex_init(&dcp->iomfb.lock);
	mutex_init(&dcp->external_lock);
	mutex_init(&dcp->hpd_mutex);
	spin_lock_init(&dcp->metadata.lock);
	spin_lock_init(&dcp->mode_state.lock);
	dcp_modes_init(dcp);
	INIT_WORK(&dcp->external_stop_work, v27_external_stop);
	INIT_WORK(&dcp->vblank_wq, v27_external_noop);
	INIT_WORK(&t->connector.hotplug_wq, v27_external_noop);
	KUNIT_ASSERT_EQ(test, kunit_add_action_or_reset(test, v27_external_cleanup, t), 0);
	dcp->connector = &t->connector;
	dcp->external_cable = true;
	dcp->external_running = true;
	dcp->external_generation = dcp->external_rx_generation = 7;
	dcp->external_hotplug_generation = 7;
	dcp->external_fw_connected = true;
	dcp->typec_work_enabled = true;
	dcp->typec_cable_connected = true;
	dcp->active_typec_route = fixed ? NULL : (void *)t;
	dcp->fixed_phy = fixed ? (void *)t : NULL;
	dcp->iomfb.presentation_paused = true;
	dcp->modes_generation = 11;
	dcp->modes_admitted = true;
	dcp->modes = t->modes;
	dcp->nr_modes = ARRAY_SIZE(t->modes);
	for (i = 0; i < ARRAY_SIZE(t->modes); i++) {
		t->modes[i].mode = (struct drm_display_mode) {
			.clock = 241500, .hdisplay = 2560, .hsync_start = 2608,
			.hsync_end = 2640, .htotal = 2720, .vdisplay = 1440,
			.vsync_start = 1443, .vsync_end = 1448, .vtotal = 1481,
		};
		t->modes[i].sdr_usable = true;
		t->modes[i].pq.score = -1;
	}
	t->modes[0].firmware_preferred = true;
	t->modes[0].timing_mode_id = 46;
	t->modes[0].color_mode_id = 109;
	t->modes[1].mode.clock = 580080;
	t->modes[1].timing_mode_id = 100;
	t->modes[1].color_mode_id = 103;
	t->modes[2].mode.hdisplay = 3840;
	t->modes[2].mode.vdisplay = 2160;
	t->modes[2].mode.clock = 594000;
	return t;
}

static int v27_external_apply(struct v27_external_test *t, unsigned int index)
{
	struct drm_crtc_state state = { .mode = t->modes[index].mode };

	guard(mutex)(&t->dcp.external_lock);
	return dcp_external_modeset(&t->dcp, &state, t->dcp.modes_generation);
}

static void v27_external_hpd_reconciliation(struct kunit *test)
{
	enum v27_external_event event;
	unsigned int fixed;

	for (fixed = 0; fixed < 2; fixed++) {
		for (event = V27_EXT_BOUNCE; event <= V27_EXT_SUSPEND; event++) {
			struct v27_external_test *t = v27_external_setup(test, fixed);
			int ret;

			t->event = event;
			ret = v27_external_apply(t, 0);
			KUNIT_EXPECT_EQ(test, ret == 0, event == V27_EXT_BOUNCE);
			KUNIT_EXPECT_EQ(test, t->connector.connected, ret == 0);
			KUNIT_EXPECT_EQ(test, t->connector.edid_live, ret == 0);
			KUNIT_EXPECT_EQ(test, t->dcp.external_mode_valid, ret == 0);
			KUNIT_EXPECT_EQ(test, t->dcp.mode_state.valid, ret == 0);
			KUNIT_EXPECT_FALSE(test, t->dcp.mode_state.changing);
			KUNIT_EXPECT_FALSE(test, t->dcp.fixed_link_held);
			flush_work(&t->dcp.external_stop_work);
			if (fixed || event == V27_EXT_BOUNCE)
				KUNIT_EXPECT_EQ(test, atomic_read(&t->stops), 0);
		}
	}
}

static void v27_external_mode_cache(struct kunit *test)
{
	struct v27_external_test *t = v27_external_setup(test, false);

	KUNIT_ASSERT_EQ(test, v27_external_apply(t, 0), 0);
	KUNIT_ASSERT_EQ(test, v27_external_apply(t, 0), 0);
	KUNIT_EXPECT_EQ(test, t->calls, 1U);
	KUNIT_ASSERT_EQ(test, v27_external_apply(t, 1), 0);
	KUNIT_EXPECT_EQ(test, t->calls, 2U);
	t->status = 6;
	KUNIT_EXPECT_EQ(test, v27_external_apply(t, 0), -EREMOTEIO);
	KUNIT_EXPECT_FALSE(test, t->dcp.external_mode_valid);
	t->status = 0;
	KUNIT_ASSERT_EQ(test, v27_external_apply(t, 1), 0);
	KUNIT_EXPECT_EQ(test, t->calls, 4U);
	KUNIT_EXPECT_EQ(test, t->dcp.external_mode.timing_mode_id, 100U);
}

static void v27_external_initial_selection(struct kunit *test)
{
	struct v27_external_test *t = v27_external_setup(test, false);
	struct drm_crtc_state state = {};
	u64 generation;

	KUNIT_ASSERT_EQ(test, dcp_external_initial_mode(&t->dcp, &state, &generation), 0);
	KUNIT_EXPECT_EQ(test, generation, 11ULL);
	KUNIT_EXPECT_EQ(test, state.mode.clock, 241500);
	KUNIT_EXPECT_EQ(test, state.mode.hdisplay, 2560);
	t->modes[0].mode.hdisplay = t->modes[1].mode.hdisplay = 3440;
	KUNIT_ASSERT_EQ(test, dcp_external_initial_mode(&t->dcp, &state, &generation), 0);
	KUNIT_EXPECT_EQ(test, state.mode.hdisplay, 3440);
	t->dcp.modes_admitted = false;
	KUNIT_EXPECT_EQ(test, dcp_external_initial_mode(&t->dcp, &state, &generation), -ENODATA);
}

static void v27_start_reply(struct apple_dcp *dcp,
			    const struct dcp_method_entry *call,
			    dcp_callback_t callback, void *cookie)
{
	struct v27_start_test *t = container_of(dcp, struct v27_start_test, dcp);
	u8 reply[4] = { 1, 0xff, 0xff, 0xff };
	static const char * const methods[] = { "A401", "A466", "A412" };

	KUNIT_ASSERT_LT(t->test, t->calls, ARRAY_SIZE(methods));
	KUNIT_ASSERT_EQ(t->test, memcmp(call->tag, methods[t->calls], 4), 0);
	switch (t->calls++) {
	case 0:
		/* A401 is a boolean; the other bytes retain the AP's poison. */
		reply[0] = t->result;
		callback(dcp, t->fail_start ? NULL : reply, cookie);
		break;
	case 1:
		/* A466's zero-length output points at unrelated shared bytes. */
		memset(reply, 0xff, sizeof(reply));
		callback(dcp, t->fail_open ? NULL : reply, cookie);
		break;
	case 2:
		callback(dcp, reply, cookie);
		break;
	}
}

static struct v27_start_test *v27_start_setup(struct kunit *test)
{
	struct v27_start_test *t = kunit_kzalloc(test, sizeof(*t), GFP_KERNEL);

	KUNIT_ASSERT_NOT_NULL(test, t);
	t->test = test;
	t->result = 1;
	t->dcp.dev = kunit_device_register(test, "v27-start");
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, t->dcp.dev);
	t->dcp.fw_compat = DCP_FIRMWARE_V_27_0;
	t->dcp.test_push = v27_start_reply;
	/* Emulate the serialized receive context that runs the reply chain. */
	t->dcp.iomfb.owner = current;
	init_completion(&t->dcp.start_done);
	return t;
}

static void v27_start_success(struct kunit *test)
{
	struct v27_start_test *t = v27_start_setup(test);

	iomfb_start_v27(&t->dcp);
	KUNIT_EXPECT_EQ(test, t->calls, 3U);
	KUNIT_EXPECT_FALSE(test, t->dcp.crashed);
	KUNIT_EXPECT_TRUE(test, t->dcp.active);
	KUNIT_EXPECT_TRUE(test, t->dcp.main_display);
	KUNIT_EXPECT_TRUE(test, completion_done(&t->dcp.start_done));
}

static void v27_start_refused(struct kunit *test)
{
	struct v27_start_test *t = v27_start_setup(test);

	t->result = 0;
	iomfb_start_v27(&t->dcp);
	KUNIT_EXPECT_EQ(test, t->calls, 1U);
	KUNIT_EXPECT_TRUE(test, t->dcp.crashed);
	KUNIT_EXPECT_FALSE(test, t->dcp.active);
	KUNIT_EXPECT_TRUE(test, completion_done(&t->dcp.start_done));
}

static void v27_start_missing_ack(struct kunit *test)
{
	struct v27_start_test *t = v27_start_setup(test);

	t->fail_start = true;
	iomfb_start_v27(&t->dcp);
	KUNIT_EXPECT_EQ(test, t->calls, 1U);
	KUNIT_EXPECT_TRUE(test, t->dcp.crashed);
	KUNIT_EXPECT_FALSE(test, t->dcp.active);
	KUNIT_EXPECT_TRUE(test, completion_done(&t->dcp.start_done));
}

static void v27_open_missing_ack(struct kunit *test)
{
	struct v27_start_test *t = v27_start_setup(test);

	t->fail_open = true;
	iomfb_start_v27(&t->dcp);
	KUNIT_EXPECT_EQ(test, t->calls, 2U);
	KUNIT_EXPECT_TRUE(test, t->dcp.crashed);
	KUNIT_EXPECT_FALSE(test, t->dcp.active);
	KUNIT_EXPECT_TRUE(test, completion_done(&t->dcp.start_done));
}

static void v27_refresh_reply(struct apple_dcp *dcp,
			      const struct dcp_method_entry *call,
			      dcp_callback_t callback, void *cookie)
{
	struct v27_start_test *t = container_of(dcp, struct v27_start_test, dcp);
	struct dcp_swap_start_resp_h17p start = { .swap_id = 7 };
	static const char * const methods[] = { "A413", "A406", "A408" };
	u32 status = 0;
	unsigned int i;

	KUNIT_ASSERT_LT(t->test, t->calls, ARRAY_SIZE(methods));
	KUNIT_ASSERT_EQ(t->test, memcmp(call->tag, methods[t->calls], 4), 0);
	switch (t->calls++) {
	case 0:
		callback(dcp, &status, cookie);
		break;
	case 1:
		callback(dcp, &start, cookie);
		break;
	case 2:
		/* Inspect the actual A408 wire record, before accepting the swap. */
		for (i = 0; i < 7; i++)
			KUNIT_EXPECT_EQ(t->test,
				get_unaligned_le64(&dcp->present_v27.swap.timestamp[i]),
				(i == 0 || i == 1 || i == 6) ? t->expected_deadline : 0);
		break;
	}
}

/* A fixed 120 Hz mode with no admitted VRR range must still be timed. */
static void v27_fixed_promotion(struct kunit *test)
{
	struct v27_start_test *t = v27_start_setup(test);
	struct apple_dcp *dcp = &t->dcp;
	struct v27_frame {
		struct drm_device drm;
		struct drm_crtc crtc;
		struct drm_crtc_state state;
		struct apple_connector connector;
		struct apple_plane plane;
		struct apple_plane_state plane_state;
		struct drm_framebuffer fb;
		struct drm_atomic_commit commit;
		struct __drm_crtcs_state crtc_entry;
		struct __drm_planes_state plane_entry;
		struct dcp_display_mode mode;
	} *f = kunit_kzalloc(test, sizeof(*f), GFP_KERNEL);
	unsigned int i;

	KUNIT_ASSERT_NOT_NULL(test, f);
	dcp->test_push = v27_refresh_reply;
	dcp_modes_init(dcp);
	spin_lock_init(&dcp->metadata.lock);
	mutex_init(&dcp->iomfb.lock);
	mutex_init(&dcp->swapped_out_fbs_lock);
	INIT_LIST_HEAD(&dcp->swapped_out_fbs);
	dcp->connector = &f->connector;
	f->connector.connected = true;
	dcp->mode_state.valid = true;
	dcp->modes_admitted = true;
	dcp->modes = &f->mode;
	dcp->nr_modes = 1;
	f->mode.timing_mode_id = 2;
	f->mode.color_mode_id = 1;
	f->mode.mode = (struct drm_display_mode) {
		.clock = 804560, .hdisplay = 3024, .hsync_start = 3032,
		.hsync_end = 3064, .htotal = 3104, .vdisplay = 1890,
		.vsync_start = 1992, .vsync_end = 2074, .vtotal = 2160,
	};
	f->drm.mode_config.num_total_plane = 1;
	f->fb.format = drm_format_info(DRM_FORMAT_XRGB8888);
	f->plane_state.base.fb = &f->fb;
	f->plane_state.base.visible = true;
	f->plane_state.base.crtc = &f->crtc;
	f->crtc_entry.new_state = &f->state;
	f->plane_entry.ptr = &f->plane.base;
	f->plane_entry.old_state = f->plane_entry.new_state = &f->plane_state.base;
	f->commit.dev = &f->drm;
	f->commit.crtcs = &f->crtc_entry;
	f->commit.planes = &f->plane_entry;

	/* Fixed ProMotion, fixed 60 Hz, and an external fixed 120 Hz timing. */
	for (i = 0; i < 3; i++) {
		dcp->main_display = i != 2;
		dcp->connector_type = i == 2 ? DRM_MODE_CONNECTOR_DisplayPort :
			DRM_MODE_CONNECTOR_eDP;
		f->mode.mode.clock = i == 1 ? 402280 : 804560;
		f->mode.needs_timestamps = i != 1;
		f->state.mode = f->mode.mode;
		t->expected_deadline = i == 0 ? 120 : 0;
		t->calls = 0;
		dcp->present_state_h17p.pending = false;
		KUNIT_ASSERT_EQ(test, iomfb_modeset_v27(dcp, &f->state), 0);
		KUNIT_EXPECT_EQ(test, dcp->use_timestamps, i == 0);
		KUNIT_EXPECT_FALSE(test, dcp->vrr_enabled);
		/* A previous AP submission time must not become a fw27 deadline. */
		dcp->swap_submit_timestamp = 0x123456789ULL;
		mutex_lock(&dcp->iomfb.lock);
		iomfb_flush_v27(dcp, &f->crtc, &f->commit);
		mutex_unlock(&dcp->iomfb.lock);
		KUNIT_EXPECT_EQ(test, t->calls, 3U);
	}
}

static struct kunit_case v27_start_cases[] = {
	KUNIT_CASE(v27_start_success),
	KUNIT_CASE(v27_start_refused),
	KUNIT_CASE(v27_start_missing_ack),
	KUNIT_CASE(v27_open_missing_ack),
	KUNIT_CASE(v27_fixed_promotion),
	KUNIT_CASE(v27_external_hpd_reconciliation),
	KUNIT_CASE(v27_external_mode_cache),
	KUNIT_CASE(v27_external_initial_selection),
	{}
};

static struct kunit_suite v27_start_suite = {
	.name = "apple-dcp-v27-start",
	.test_cases = v27_start_cases,
};

kunit_test_suite(v27_start_suite);

MODULE_LICENSE("Dual MIT/GPL");
