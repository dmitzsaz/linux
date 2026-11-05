// SPDX-License-Identifier: GPL-2.0-only OR MIT

#include <kunit/device.h>
#include <kunit/test.h>
#include <linux/module.h>

#include "dcp.h"
#include "dcp-internal.h"
#include "iomfb_v27.h"

struct v27_start_test {
	struct apple_dcp dcp;
	struct kunit *test;
	u8 result;
	bool fail_start, fail_open;
	unsigned int calls;
};

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

static struct kunit_case v27_start_cases[] = {
	KUNIT_CASE(v27_start_success),
	KUNIT_CASE(v27_start_refused),
	KUNIT_CASE(v27_start_missing_ack),
	KUNIT_CASE(v27_open_missing_ack),
	{}
};

static struct kunit_suite v27_start_suite = {
	.name = "apple-dcp-v27-start",
	.test_cases = v27_start_cases,
};

kunit_test_suite(v27_start_suite);

MODULE_LICENSE("Dual MIT/GPL");
