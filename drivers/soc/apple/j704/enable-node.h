/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef _APPLE_J704_ENABLE_NODE_H
#define _APPLE_J704_ENABLE_NODE_H

#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/slab.h>

/* Descriptions are in the board DTS. Keep an applied changeset until reboot. */
static int j704_enable_nodes(struct device_node **nodes, unsigned int count)
{
	struct of_changeset *changes;
	const char *status;
	unsigned int i;
	int ret;

	for (i = 0; i < count; i++) {
		if (!nodes[i] ||
		    of_property_read_string(nodes[i], "status", &status) ||
		    strcmp(status, "disabled"))
			return -EINVAL;
	}
	changes = kzalloc(sizeof(*changes), GFP_KERNEL);
	if (!changes)
		return -ENOMEM;
	of_changeset_init(changes);
	for (i = 0; i < count; i++) {
		ret = of_changeset_update_prop_string(changes, nodes[i],
						      "status", "okay");
		if (ret)
			goto free;
	}
	ret = of_changeset_apply(changes);
	if (ret)
		goto free;
	return 0;
free:
	of_changeset_destroy(changes);
	kfree(changes);
	return ret;
}

#endif
