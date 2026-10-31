// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * T8142 (M5, HAL300) helpers that have no Rust bindings: static inlines,
 * THIS_MODULE and the OF reserved-memory lookup of nodes that a device-tree
 * overlay added after boot. All admission policy stays in Rust.
 */
#include <crypto/sha2.h>
#include <linux/device.h>
#include <linux/ioport.h>
#include <linux/mm.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/of_address.h>

void asahi_hal300_sha256(const u8 *data, size_t len, u8 *out);
void asahi_hal300_module_get(void);
int asahi_hal300_region(struct device *dev, const char *name, u64 *start,
			u64 *size);

void asahi_hal300_sha256(const u8 *data, size_t len, u8 *out)
{
	sha256(data, len, out);
}

/* Pin this module until reboot: the started firmware calls back into it. */
void asahi_hal300_module_get(void)
{
	__module_get(THIS_MODULE);
}

/*
 * Resolve a named no-map memory-region phandle that lies outside System RAM.
 * Unlike of_reserved_mem_region_to_resource_byname(), this also accepts
 * reserved-memory nodes from a boot-bound overlay, which the early
 * reserved-memory scan never saw.
 */
int asahi_hal300_region(struct device *dev, const char *name, u64 *start,
			u64 *size)
{
	struct device_node *np;
	struct resource res;
	int index, ret;

	index = of_property_match_string(dev->of_node, "memory-region-names",
					 name);
	if (index < 0)
		return -EINVAL;
	np = of_parse_phandle(dev->of_node, "memory-region", index);
	if (!np)
		return -EINVAL;
	ret = !of_property_read_bool(np, "no-map") ||
			      of_property_read_bool(np, "reusable") ||
			      !of_device_is_available(np) ?
		      -EINVAL :
		      of_address_to_resource(np, 0, &res);
	of_node_put(np);
	if (ret)
		return ret;
	if (!resource_size(&res) ||
	    region_intersects(res.start, resource_size(&res),
			      IORESOURCE_SYSTEM_RAM,
			      IORES_DESC_NONE) != REGION_DISJOINT)
		return -EBUSY;
	*start = res.start;
	*size = resource_size(&res);
	return 0;
}

/* Diagnostic stack for the HAL300 lower-arena watch in mmu.rs. */
void asahi_hal300_dump_stack(void)
{
	dump_stack();
}
