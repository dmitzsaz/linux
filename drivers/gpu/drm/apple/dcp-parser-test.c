// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* Synthetic SDR converter catalogs, without display identities or firmware data. */
#include <kunit/test.h>
#include <linux/err.h>
#include <linux/slab.h>
#include <linux/string.h>
#include <linux/unaligned.h>

#include "parser.h"

enum ycc_change {
	YCC_NORMAL,
	YCC_UNSAFE,
	YCC_DSC,
	YCC_OUTPUT_UNSAFE,
	YCC_OUTPUT_DSC,
	YCC_NO_OUTPUT,
	YCC_OUTPUT_422,
	YCC_OUTPUT_BT601,
	YCC_OUTPUT_DEPTH,
	YCC_OUTPUT_RANGE,
	YCC_OUTPUT_HDR,
	YCC_OUTPUT_VIRTUAL,
	YCC_OUTPUT_SCORE,
	YCC_OUTPUT_SAME_ID,
	YCC_OUTPUT_MISSING,
	YCC_OUTPUT_DUPLICATE,
	YCC_OUTPUT_CHAIN,
	YCC_DUPLICATE_OUTPUT,
	YCC_BT601,
	YCC_DEPTH,
	YCC_RANGE,
	YCC_HDR,
	YCC_VIRTUAL,
	YCC_MISSING,
	YCC_DUPLICATE,
	YCC_NO_ENCODING,
	YCC_WRONG_ENCODING,
	YCC_INTERLACED,
	YCC_REPEAT,
	YCC_RGB_UNSAFE,
};

struct ycc_blob {
	u8 data[65536];
	u32 size;
};

static void ycc_tag(struct ycc_blob *b, u32 type, u32 size)
{
	b->size = ALIGN(b->size, 4);
	put_unaligned_le32(type << 24 | size, b->data + b->size);
	b->size += 4;
}

static void ycc_key(struct ycc_blob *b, const char *key)
{
	u32 len = strlen(key);

	ycc_tag(b, 9, len);
	memcpy(b->data + b->size, key, len);
	b->size += len;
}

static void ycc_int(struct ycc_blob *b, const char *key, s64 value)
{
	ycc_key(b, key);
	ycc_tag(b, 4, 64);
	put_unaligned_le64(value, b->data + b->size);
	b->size += 8;
}

static void ycc_bool(struct ycc_blob *b, const char *key, bool value)
{
	ycc_key(b, key);
	ycc_tag(b, 11, value);
}

static void ycc_colors(struct ycc_blob *b, enum ycc_change change, bool output)
{
	static const char * const keys[] = {
		"ID", "Score", "Depth", "PixelEncoding", "DynamicRange", "EOTF", "Colorimetry",
	};
	s64 values[] = { output ? 186 : 187, 100, 8, output ? 1 : 3, 1, 0, 1 };
	bool virtual = change == (output ? YCC_OUTPUT_VIRTUAL : YCC_VIRTUAL);
	bool missing = change == (output ? YCC_OUTPUT_MISSING : YCC_MISSING);
	bool duplicate = change == (output ? YCC_OUTPUT_DUPLICATE : YCC_DUPLICATE);
	unsigned int outputs = output ? change == YCC_OUTPUT_CHAIN :
		change == YCC_NO_OUTPUT ? 0 : change == YCC_DUPLICATE_OUTPUT ? 2 : 1;
	unsigned int i;

	if (change == (output ? YCC_OUTPUT_DEPTH : YCC_DEPTH))
		values[2] = 10;
	if (change == YCC_OUTPUT_422 && output)
		values[3] = 3;
	if (change == (output ? YCC_OUTPUT_RANGE : YCC_RANGE))
		values[4] = 0;
	if (change == (output ? YCC_OUTPUT_HDR : YCC_HDR))
		values[5] = 2;
	if (change == (output ? YCC_OUTPUT_BT601 : YCC_BT601))
		values[6] = 0;
	if (change == YCC_OUTPUT_SCORE && output)
		values[1] = -1;
	if (change == YCC_OUTPUT_SAME_ID && output)
		values[0] = 187;

	ycc_tag(b, 1, 8 + outputs + duplicate - missing);
	ycc_bool(b, "IsVirtual", virtual);
	for (i = 0; i < ARRAY_SIZE(keys); i++) {
		if (missing && i == 2)
			continue;
		ycc_int(b, keys[i], values[i]);
	}
	if (duplicate)
		ycc_int(b, "Depth", 8);
	for (i = 0; i < outputs; i++) {
		ycc_key(b, "DownstreamFormat");
		if (output)
			ycc_tag(b, 1, 0);
		else
			ycc_colors(b, change, true);
	}
}

static void ycc_rgb(struct ycc_blob *b, bool pq)
{
	ycc_tag(b, 1, 8);
	ycc_int(b, "ID", pq ? 190 : 176);
	ycc_int(b, "Score", 1);
	ycc_int(b, "Depth", pq ? 10 : 8);
	ycc_int(b, "PixelEncoding", 0);
	ycc_int(b, "DynamicRange", 0);
	ycc_int(b, "EOTF", pq ? 2 : 0);
	ycc_int(b, "Colorimetry", pq ? 9 : 10);
	ycc_bool(b, "IsVirtual", false);
}

static void ycc_dimension(struct ycc_blob *b, bool vertical, u32 active, u32 rate)
{
	bool uhd = active == 3840 || active == 4096;
	u32 total = vertical ? (active == 2160 ? 2250 : 1125) :
		uhd ? (rate == 50U << 16 ? 5280 : 4400) : 2200;
	u32 porch = vertical ? (active == 2160 ? 8 : 4) :
		uhd ? total - active - 88 - (active == 3840 ? 296 : 128) : 88;
	u32 sync = vertical ? (active == 2160 ? 10 : 5) : uhd ? 88 : 44;

	if (rate == 11796583) {
		total = vertical ? 1106 : 2080;
		porch = vertical ? 3 : 48;
		sync = vertical ? 5 : 32;
	}
	ycc_key(b, vertical ? "VerticalAttributes" : "HorizontalAttributes");
	ycc_tag(b, 1, 7);
	ycc_int(b, "Active", active);
	ycc_int(b, "Total", total);
	ycc_int(b, "FrontPorch", porch);
	ycc_int(b, "SyncWidth", sync);
	ycc_int(b, "PreciseSyncRate", vertical ? rate : 0);
	ycc_int(b, "SyncPolarity", 1);
	ycc_int(b, "PixelRepetition", 0);
}

static void ycc_timing(struct ycc_blob *b, u32 id, u32 width, u32 rate,
		       enum ycc_change change, bool rgb, bool ycc, bool pq)
{
	bool encoding = change != YCC_NO_ENCODING;
	bool unsafe = change == YCC_UNSAFE || change == YCC_OUTPUT_UNSAFE ||
		change == YCC_RGB_UNSAFE;
	bool dsc = change == YCC_DSC || change == YCC_OUTPUT_DSC;

	ycc_tag(b, 1, 10 + encoding);
	ycc_int(b, "ID", id);
	ycc_int(b, "Score", id);
	ycc_bool(b, "IsVirtual", false);
	ycc_bool(b, "IsPreferred", rgb && rate == 30U << 16);
	ycc_bool(b, "IsInterlaced", change == YCC_INTERLACED);
	ycc_dimension(b, false, width, rate);
	if (change == YCC_REPEAT)
		put_unaligned_le64(1, b->data + b->size - 8);
	ycc_dimension(b, true, width >= 3840 ? 2160 : 1080, rate);
	ycc_key(b, "ColorModes");
	ycc_tag(b, 2, rgb + ycc + pq);
	if (rgb)
		ycc_rgb(b, false);
	if (ycc)
		ycc_colors(b, change, false);
	if (pq)
		ycc_rgb(b, true);
	ycc_key(b, "UnsafeColorElementIDs");
	ycc_tag(b, 2, unsafe);
	if (unsafe) {
		ycc_tag(b, 4, 64);
		put_unaligned_le64(change == YCC_RGB_UNSAFE ? 176 :
				   change == YCC_UNSAFE ? 187 : 186, b->data + b->size);
		b->size += 8;
	}
	ycc_key(b, "DSCRequiredColorElementIDs");
	ycc_tag(b, 2, dsc);
	if (dsc) {
		ycc_tag(b, 4, 64);
		put_unaligned_le64(change == YCC_DSC ? 187 : 186, b->data + b->size);
		b->size += 8;
	}
	if (encoding)
		ycc_int(b, "ValidPixelEncodings", change == YCC_WRONG_ENCODING ? 8 : 2);
}

static struct ycc_blob *ycc_begin(struct kunit *test, unsigned int count)
{
	struct ycc_blob *b = kunit_kzalloc(test, sizeof(*b), GFP_KERNEL);

	if (!b)
		return NULL;
	put_unaligned_le32(0xd3, b->data);
	b->size = 4;
	ycc_tag(b, 2, count);
	return b;
}

static struct dcp_display_mode *ycc_parse(struct kunit *test, struct ycc_blob *b,
					  bool enabled, unsigned int *count)
{
	struct dcp_parse_ctx ctx;
	int ret = parse(b->data, b->size, &ctx);

	if (ret) {
		KUNIT_FAIL(test, "invalid test catalog: %d", ret);
		return ERR_PTR(ret);
	}
	ctx.fw27_hdmi_ycbcr420 = enabled;
	return enumerate_modes(&ctx, count, 0, 0, 0, false, true);
}

static void dcp_ycc_catalog_test(struct kunit *test)
{
	static const u32 rates[] = { 50U << 16, 3928232, 60U << 16 };
	static const u32 ids[] = { 143, 145, 147, 149, 150, 151 };
	struct ycc_blob *b = ycc_begin(test, 46);
	struct dcp_display_mode *m;
	unsigned int count, i;

	KUNIT_ASSERT_NOT_NULL(test, b);
	/* Forty independent RGB commands, plus six standard UHD timings. */
	for (i = 0; i < 40; i++)
		ycc_timing(b, 500 + i, i ? 1920 : 3840, 30U << 16,
			   YCC_NORMAL, true, false, false);
	for (i = 0; i < ARRAY_SIZE(ids); i++)
		ycc_timing(b, ids[i], i < 3 ? 3840 : 4096, rates[i % 3],
			   YCC_NORMAL, false, true, false);
	KUNIT_ASSERT_LT(test, b->size, sizeof(b->data));
	m = ycc_parse(test, b, false, &count);
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, m);
	KUNIT_EXPECT_EQ(test, count, 40U);
	kfree(m);
	m = ycc_parse(test, b, true, &count);
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, m);
	KUNIT_ASSERT_EQ(test, count, 46U);
	for (i = 40; i < count; i++) {
		KUNIT_EXPECT_EQ(test, m[i].timing_mode_id, ids[i - 40]);
		KUNIT_EXPECT_EQ(test, m[i].color_mode_id, 187U);
		KUNIT_EXPECT_EQ(test, m[i].sdr.format, DCP_COLOR_FORMAT_YCBCR422);
		KUNIT_EXPECT_EQ(test, m[i].sdr.range, DCP_COLOR_YCBCR_RANGE_LIMITED);
		KUNIT_EXPECT_EQ(test, m[i].sdr.colorimetry, DCP_COLORIMETRY_BT709);
		KUNIT_EXPECT_LT(test, m[i].sdr_rgb.score, 0);
		KUNIT_EXPECT_TRUE(test, m[i].sdr_usable);
		KUNIT_EXPECT_FALSE(test, m[i].automatic_rgb8_quirk);
		KUNIT_EXPECT_FALSE(test, m[i].vrr);
		KUNIT_EXPECT_EQ(test, !!(m[i].mode.type & DRM_MODE_TYPE_PREFERRED), i == 42);
	}
	kfree(m);
}

static void dcp_ycc_restrictions_test(struct kunit *test)
{
	enum ycc_change change;

	for (change = YCC_UNSAFE; change <= YCC_REPEAT; change++) {
		struct ycc_blob *b = ycc_begin(test, 1);
		struct dcp_display_mode *m;
		unsigned int count;

		KUNIT_ASSERT_NOT_NULL(test, b);
		ycc_timing(b, 147, 3840, 60U << 16, change, false, true, false);
		m = ycc_parse(test, b, true, &count);
		KUNIT_ASSERT_NOT_ERR_OR_NULL(test, m);
		KUNIT_EXPECT_EQ_MSG(test, count, 0U, "change=%d", change);
		kfree(m);
	}
}

static void dcp_ycc_rgb_hdr_test(struct kunit *test)
{
	struct ycc_blob *b = ycc_begin(test, 1);
	struct dcp_display_mode *m;
	unsigned int count;

	KUNIT_ASSERT_NOT_NULL(test, b);
	/* A higher-scored converter color must never replace usable RGB. */
	ycc_timing(b, 147, 3840, 60U << 16, YCC_NORMAL, true, true, true);
	m = ycc_parse(test, b, true, &count);
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, m);
	KUNIT_ASSERT_EQ(test, count, 1U);
	KUNIT_EXPECT_EQ(test, m[0].color_mode_id, 176U);
	KUNIT_EXPECT_EQ(test, m[0].sdr_rgb.format, DCP_COLOR_FORMAT_RGB);
	KUNIT_EXPECT_EQ(test, m[0].sdr.id, 187U);
	KUNIT_EXPECT_EQ(test, m[0].pq.id, 190U);
	KUNIT_EXPECT_EQ(test, m[0].pq.eotf, DCP_EOTF_ST_2084);
	kfree(m);

	b = ycc_begin(test, 1);
	KUNIT_ASSERT_NOT_NULL(test, b);
	/* PQ alone still cannot provide an external SDR timing. */
	ycc_timing(b, 147, 3840, 60U << 16, YCC_NORMAL, false, false, true);
	m = ycc_parse(test, b, true, &count);
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, m);
	KUNIT_EXPECT_EQ(test, count, 0U);
	kfree(m);
}

static void dcp_ycc_unsafe_rgb_test(struct kunit *test)
{
	struct ycc_blob *b = ycc_begin(test, 1);
	struct dcp_display_mode *m;
	unsigned int count;

	KUNIT_ASSERT_NOT_NULL(test, b);
	/* FW27 must not admit the unsafe RGB8 1080p180 exception. */
	ycc_timing(b, 78, 1920, 11796583, YCC_RGB_UNSAFE, true, false, false);
	m = ycc_parse(test, b, true, &count);
	KUNIT_ASSERT_NOT_ERR_OR_NULL(test, m);
	KUNIT_EXPECT_EQ(test, count, 0U);
	kfree(m);
}

static void dcp_ycc_truncated_test(struct kunit *test)
{
	struct ycc_blob *b = ycc_begin(test, 1);
	u32 size, cut;

	KUNIT_ASSERT_NOT_NULL(test, b);
	ycc_timing(b, 147, 3840, 60U << 16, YCC_NORMAL, false, true, false);
	size = b->size;
	for (cut = 4; cut < size; cut++) {
		struct dcp_display_mode *m;
		unsigned int count;

		b->size = cut;
		m = ycc_parse(test, b, true, &count);
		if (IS_ERR(m))
			continue;
		KUNIT_EXPECT_EQ_MSG(test, count, 0U, "cut=%u", cut);
		kfree(m);
	}
}

static struct kunit_case dcp_ycc_cases[] = {
	KUNIT_CASE(dcp_ycc_catalog_test),
	KUNIT_CASE(dcp_ycc_restrictions_test),
	KUNIT_CASE(dcp_ycc_rgb_hdr_test),
	KUNIT_CASE(dcp_ycc_unsafe_rgb_test),
	KUNIT_CASE(dcp_ycc_truncated_test),
	{}
};

static struct kunit_suite dcp_ycc_suite = {
	.name = "apple-dcp-hdmi-color",
	.test_cases = dcp_ycc_cases,
};

kunit_test_suite(dcp_ycc_suite);
