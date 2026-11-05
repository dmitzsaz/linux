// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* Included by pmp-report.c: a crashed PMP can leave ready/ACK SRAM set. */

#include <kunit/test.h>

struct pmp_owner_test {
	struct apple_pmp_report rep;
	struct apple_pmp_report_owner owner;
	struct device dev;
	struct device_node node;
	u64 regs[4];
};

static const struct apple_pmp_report_offsets pmp_owner_test_offsets = {
	.tgt_read = 0,
	.tgt_write = 8,
	.actual = 16,
	.status = 24,
};

static struct pmp_owner_test *pmp_owner_test_new(struct kunit *test)
{
	struct pmp_owner_test *t = kunit_kzalloc(test, sizeof(*t), GFP_KERNEL);
	unsigned long flags;

	KUNIT_ASSERT_NOT_NULL(test, t);
	t->dev.of_node = &t->node;
	t->owner.dev = &t->dev;
	t->rep.pmp = &t->node;
	t->rep.base = (__force void __iomem *)t->regs;
	t->rep.offsets = &pmp_owner_test_offsets;
	spin_lock_init(&t->rep.lock);
	INIT_DELAYED_WORK(&t->rep.ready_work, apple_pmp_report_ready_work);
	t->regs[2] = BIT_ULL(3);
	t->regs[3] = PMP_REPORT_READY;
	spin_lock_irqsave(&apple_pmp_report_owners_lock, flags);
	list_add_tail(&t->owner.list, &apple_pmp_report_owners);
	spin_unlock_irqrestore(&apple_pmp_report_owners_lock, flags);
	KUNIT_ASSERT_EQ(test, kunit_add_action_or_reset(test,
						     apple_pmp_report_remove_owner, &t->owner), 0);
	return t;
}

static void pmp_owner_crash_invalidates_stale_ack(struct kunit *test)
{
	struct pmp_owner_test *t = pmp_owner_test_new(test);
	u64 request;

	KUNIT_ASSERT_EQ(test, apple_pmp_report_set_state(&t->rep, 3, true, false), 0);
	KUNIT_ASSERT_TRUE(test, apple_pmp_report_running(&t->rep));
	request = t->regs[1];
	apple_pmp_report_fail_owner(&t->dev);
	KUNIT_EXPECT_FALSE(test, apple_pmp_report_running(&t->rep));
	KUNIT_EXPECT_EQ(test, apple_pmp_report_set_state(&t->rep, 3, true, false), -EIO);
	KUNIT_EXPECT_EQ(test, apple_pmp_report_set_state(&t->rep, 3, false, true), -EIO);
	KUNIT_EXPECT_EQ(test, t->regs[1], request);
	KUNIT_EXPECT_EQ(test, t->regs[3], (u64)PMP_REPORT_READY);
}

static void pmp_owner_crash_stops_deferred_writes(struct kunit *test)
{
	struct pmp_owner_test *t = pmp_owner_test_new(test);

	t->rep.defer = true;
	t->rep.known = BIT_ULL(3);
	t->rep.shadow = BIT_ULL(3);
	t->regs[1] = 0x1234;
	apple_pmp_report_fail_owner(&t->dev);
	apple_pmp_report_ready_work(&t->rep.ready_work.work);
	KUNIT_EXPECT_TRUE(test, t->rep.defer);
	KUNIT_EXPECT_FALSE(test, delayed_work_pending(&t->rep.ready_work));
	KUNIT_EXPECT_EQ(test, t->regs[1], 0x1234ULL);
}

static void pmp_owner_removal_does_not_restore_stale_ack(struct kunit *test)
{
	struct pmp_owner_test *t = pmp_owner_test_new(test);

	apple_pmp_report_remove_owner(&t->owner);
	KUNIT_EXPECT_EQ(test, apple_pmp_report_set_state(&t->rep, 3, true, false), -ENODEV);
	KUNIT_EXPECT_FALSE(test, apple_pmp_report_running(&t->rep));
	KUNIT_EXPECT_EQ(test, t->regs[1], 0ULL);
}

static void pmp_owner_failure_is_scoped_to_supplier(struct kunit *test)
{
	struct pmp_owner_test *t = pmp_owner_test_new(test);
	struct pmp_owner_test *other = pmp_owner_test_new(test);

	apple_pmp_report_fail_owner(&other->dev);
	KUNIT_EXPECT_TRUE(test, apple_pmp_report_running(&t->rep));
	KUNIT_EXPECT_EQ(test, apple_pmp_report_set_state(&t->rep, 3, true, false), 0);
	KUNIT_EXPECT_FALSE(test, apple_pmp_report_running(&other->rep));
}

static struct kunit_case pmp_owner_test_cases[] = {
	KUNIT_CASE(pmp_owner_crash_invalidates_stale_ack),
	KUNIT_CASE(pmp_owner_crash_stops_deferred_writes),
	KUNIT_CASE(pmp_owner_removal_does_not_restore_stale_ack),
	KUNIT_CASE(pmp_owner_failure_is_scoped_to_supplier),
	{}
};

static struct kunit_suite pmp_owner_test_suite = {
	.name = "apple-pmp-report-owner",
	.test_cases = pmp_owner_test_cases,
};

kunit_test_suite(pmp_owner_test_suite);
