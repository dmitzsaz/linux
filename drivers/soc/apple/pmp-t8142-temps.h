/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
#ifndef APPLE_PMP_T8142_TEMPS_H
#define APPLE_PMP_T8142_TEMPS_H

#include <linux/errno.h>
#include <linux/io.h>
#include <linux/kernel.h>
#include <linux/limits.h>
#include <linux/string.h>
#include <linux/unaligned.h>

/* Layout of firmware 5048df36-0612-3766-b0d7-2b9a03982e02, not T8132.
 * Firmware constructs the controller at IOP 0x1061110, installs its vtable
 * at 0x1045c58 and allocates count * 0xa4 bytes for temperature records.
 * The sampling method stores signed Q6 at +0x24 and validity at +0x30.
 */
#define T8142_PMP_IOP_BASE 0x1000000ULL
#define T8142_PMP_THERMAL 0x61110
#define T8142_PMP_RECORD_SIZE 0xa4
#define T8142_PMP_MAX_SENSORS 64
#define T8142_PMP_DT_RECORD_SIZE 0x38

struct apple_pmp_t8142_temps {
	void __iomem *sram;
	size_t size;
	u32 count;
	char names[T8142_PMP_MAX_SENSORS][16];
	u8 metadata[T8142_PMP_MAX_SENSORS][20];
};

static inline bool apple_pmp_t8142_cpu(const char *name)
{
	return name[0] == 't' && (name[1] == 'p' || name[1] == 'e');
}

static inline bool apple_pmp_t8142_identity(void __iomem *sram, size_t size)
{
	static const u8 uuid[] = {
		0x50, 0x48, 0xdf, 0x36, 0x06, 0x12, 0x37, 0x66,
		0xb0, 0xd7, 0x2b, 0x9a, 0x03, 0x98, 0x2e, 0x02,
	};
	u8 actual[sizeof(uuid)];

	if (size < T8142_PMP_THERMAL + 0x298)
		return false;
	memcpy_fromio(actual, sram + 0x214, sizeof(actual));
	return readl(sram + 0x204) == 0x64697575 &&
	       readl(sram + 0x208) == 5 &&
	       !memcmp(actual, uuid, sizeof(uuid));
}

static inline int apple_pmp_t8142_table(const struct apple_pmp_t8142_temps *temps,
				       u32 *offset)
{
	u64 ptr;
	u32 count;

	if (!apple_pmp_t8142_identity(temps->sram, temps->size))
		return -ENODEV;
	if (readq(temps->sram + T8142_PMP_THERMAL) != 0x1045c58)
		return -ENODATA;
	count = readl(temps->sram + T8142_PMP_THERMAL + 0x294);
	ptr = readq(temps->sram + T8142_PMP_THERMAL + 0x1a8);
	if (!count)
		return -ENODATA;
	if (count != temps->count || count > T8142_PMP_MAX_SENSORS || (ptr & 3) ||
	    ptr < T8142_PMP_IOP_BASE || ptr - T8142_PMP_IOP_BASE > temps->size ||
	    count * T8142_PMP_RECORD_SIZE > temps->size - (ptr - T8142_PMP_IOP_BASE))
		return -EINVAL;
	*offset = ptr - T8142_PMP_IOP_BASE;
	return 0;
}

static inline int apple_pmp_t8142_sample(struct apple_pmp_t8142_temps *temps,
					unsigned int index, long *value)
{
	u8 first[T8142_PMP_RECORD_SIZE], second[T8142_PMP_RECORD_SIZE];
	void __iomem *record;
	s16 raw;
	unsigned int attempt;
	u32 offset;
	int ret;

	if (index >= temps->count)
		return -EINVAL;
	/* Firmware can finish its table allocation after the owner probe returns. */
	ret = apple_pmp_t8142_table(temps, &offset);
	if (ret)
		return ret;
	record = temps->sram + offset + index * T8142_PMP_RECORD_SIZE;
	for (attempt = 0; attempt < 8; attempt++) {
		memcpy_fromio(first, record, sizeof(first));
		memcpy_fromio(second, record, sizeof(second));
		if (memcmp(first, second, sizeof(first)))
			continue;
		if (memcmp(first + 0x94, temps->names[index], 16) ||
		    memcmp(first, temps->metadata[index], 8) ||
		    memcmp(first + 0xc, temps->metadata[index] + 0xc, 8))
			return -EIO;
		if (readq(temps->sram + T8142_PMP_THERMAL + 0x1a8) !=
		    T8142_PMP_IOP_BASE + offset)
			return -EAGAIN;
		/* Never substitute the saved last-valid sample at +0x3c. */
		if (first[0x30] != 1)
			return -ENODATA;
		raw = (s16)get_unaligned_le16(first + 0x24);
		if (raw < -40 * 64 || raw > 150 * 64)
			return -ERANGE;
		*value = DIV_ROUND_CLOSEST((s64)raw * 1000, 64);
		return 0;
	}
	return -EAGAIN;
}

static inline int apple_pmp_t8142_hotspot(struct apple_pmp_t8142_temps *temps,
					 long *value)
{
	long hottest = LONG_MIN, sample;
	unsigned int i;
	int ret;

	for (i = 0; i < temps->count; i++) {
		if (!apple_pmp_t8142_cpu(temps->names[i]))
			continue;
		ret = apple_pmp_t8142_sample(temps, i, &sample);
		if (ret == -ENODATA)
			continue;
		if (ret)
			return ret;
		hottest = max(hottest, sample);
	}
	if (hottest == LONG_MIN)
		return -ENODATA;
	*value = hottest;
	return 0;
}

static inline int apple_pmp_t8142_prepare(struct apple_pmp_t8142_temps *temps,
					 const u8 *dt, size_t dt_size)
{
	unsigned int i, j;

	if (!apple_pmp_t8142_identity(temps->sram, temps->size))
		return -ENODEV;
	if (!dt_size || dt_size % T8142_PMP_DT_RECORD_SIZE ||
	    dt_size / T8142_PMP_DT_RECORD_SIZE > T8142_PMP_MAX_SENSORS)
		return -EINVAL;
	temps->count = dt_size / T8142_PMP_DT_RECORD_SIZE;
	for (i = 0; i < temps->count; i++, dt += T8142_PMP_DT_RECORD_SIZE) {
		/* The firmware resolves DT controller ID +8 to an instance index;
		 * compare the unchanged ID/type, group, channel and normalized name.
		 */
		if (get_unaligned_le32(dt + 4) != 1 &&
		    get_unaligned_le32(dt + 4) != 8)
			return -EINVAL;
		memcpy(temps->metadata[i], dt, 20);
		for (j = 0; j < 16; j++) {
			u8 c = dt[0x28 + j];

			if (c >= 'A' && c <= 'Z')
				c += 'a' - 'A';
			if (c && (c < 0x20 || c > 0x7e))
				return -EINVAL;
			temps->names[i][j] = c;
		}
		if (!temps->names[i][0] || !memchr(temps->names[i], 0, 16))
			return -EINVAL;
	}
	return 0;
}

#endif
