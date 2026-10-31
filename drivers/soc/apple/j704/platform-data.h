/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef _APPLE_J704_PLATFORM_DATA_H
#define _APPLE_J704_PLATFORM_DATA_H

#include <linux/module.h>
#include <linux/of.h>
#include <linux/slab.h>

struct j704_tunable {
	u32 off;
	u32 mask;
	u32 val;
};

static inline bool j704_platform_ready(void)
{
	struct device_node *node;
	u32 version;
	bool ready;

	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return false;
	node = of_find_node_by_path("/j704-platform-data");
	if (!node)
		return false;
	ready = of_device_is_compatible(node, "apple,j704-platform-data") &&
		of_property_read_bool(node, "apple,pcie-tunables-ready") &&
		!of_property_read_u32(node, "apple,format-version", &version) &&
		version == 1;
	of_node_put(node);
	return ready;
}

/* Values are supplied by the loader from this machine's ADT, never a dump. */
static inline int j704_load_tunables(const char *role, const char *property,
				     u32 span, struct j704_tunable **out,
				     unsigned int *count)
{
	struct device_node *parent, *node;
	struct j704_tunable *table;
	const __be32 *words;
	int length, ret = -EINVAL;
	u32 version;
	unsigned int i, n;

	*out = NULL;
	*count = 0;
	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	parent = of_find_node_by_path("/j704-platform-data");
	if (!parent)
		return -ENODATA;
	if (!of_device_is_compatible(parent, "apple,j704-platform-data") ||
	    !of_property_read_bool(parent, "apple,pcie-tunables-ready") ||
	    of_property_read_u32(parent, "apple,format-version", &version) ||
	    version != 1)
		goto put_parent;
	node = of_get_child_by_name(parent, role);
	if (!node) {
		ret = -ENODATA;
		goto put_parent;
	}
	words = of_get_property(node, property, &length);
	if (!words || length <= 0 || length % (3 * sizeof(u32)) ||
	    length > 4096 * 3 * sizeof(u32) || span < sizeof(u32))
		goto put_node;
	n = length / (3 * sizeof(u32));
	table = kcalloc(n, sizeof(*table), GFP_KERNEL);
	if (!table) {
		ret = -ENOMEM;
		goto put_node;
	}
	for (i = 0; i < n; i++) {
		table[i].off = be32_to_cpup(&words[3 * i]);
		table[i].mask = be32_to_cpup(&words[3 * i + 1]);
		table[i].val = be32_to_cpup(&words[3 * i + 2]);
		if (table[i].off % sizeof(u32) ||
		    table[i].off > span - sizeof(u32)) {
			kfree(table);
			goto put_node;
		}
	}
	*out = table;
	*count = n;
	ret = 0;
put_node:
	of_node_put(node);
put_parent:
	of_node_put(parent);
	if (ret)
		pr_err("J704: missing or invalid %s/%s; no hardware access\n",
		       role, property);
	return ret;
}

#endif
