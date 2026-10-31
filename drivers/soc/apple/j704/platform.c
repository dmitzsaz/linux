// SPDX-License-Identifier: GPL-2.0-only
/* J704 startup is one provider-bound transaction, not a userspace sequence.
 * Once hardware is touched, keep both this driver and partial state until
 * reboot. Suppliers may defer the probe only before that point. */
#include <linux/mfd/macsmc.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include "platform-data.h"
#include "pcie.h"

static DEFINE_MUTEX(j704_start_lock);
static bool j704_start_consumed;

static int j704_validate_tables(void)
{
	static const struct {
		const char *role, *property;
		u32 span;
	} tables[] = {
		{ "shared", "apple,tunable-axi2af", 0x1000000 },
		{ "shared", "apple,tunable-phy", 0x40000 },
		{ "shared", "apple,tunable-pll", 0x20000 },
		{ "shared", "apple,tunable-auspma", 0x20000 },
		{ "port0", "apple,tunable-config", 0x8000 },
		{ "port2", "apple,tunable-config", 0x8000 },
		{ "port0", "apple,tunable-root", 0x4000 },
		{ "port0", "apple,tunable-gen3", 0x4000 },
		{ "port0", "apple,tunable-gen4", 0x4000 },
		{ "port2", "apple,tunable-root", 0x4000 },
		{ "port2", "apple,tunable-gen3", 0x4000 },
		{ "port2", "apple,tunable-gen4", 0x4000 },
	};
	unsigned int i, count;
	struct j704_tunable *values;
	int ret;
	for (i = 0; i < ARRAY_SIZE(tables); i++) {
		ret = j704_load_tunables(tables[i].role, tables[i].property,
					 tables[i].span, &values, &count);
		if (ret)
			return ret;
		kfree(values);
	}
	return 0;
}

static int j704_platform_probe(struct platform_device *pdev)
{
	static const struct {
		const char *name;
		int (*run)(void);
	} stages[] = {
		{ "power", j704_pcie_power },
		{ "common PHY", j704_pcie_common },
		{ "radio port", j704_pcie_port0 },
		{ "radio DART", j704_pcie_dart },
		{ "radio link", j704_pcie_radio },
		{ "SD port", j704_pcie_port2 },
		{ "SD link", j704_pcie_sd },
		{ "PCI host", j704_pcie_publish },
	};
	struct device_node *node;
	struct platform_device *smc;
	unsigned int i;
	int ret;
	if (!j704_platform_ready())
		return dev_err_probe(&pdev->dev, -ENODATA,
				     "missing loader PCIe input contract\n");
	node = of_parse_phandle(pdev->dev.of_node, "apple,smc", 0);
	if (!node)
		return -EINVAL;
	smc = of_find_device_by_node(node);
	of_node_put(node);
	if (!smc)
		return -EPROBE_DEFER;
	if (!device_is_bound(&smc->dev) || !dev_get_drvdata(&smc->dev)) {
		put_device(&smc->dev);
		return -EPROBE_DEFER;
	}
	if (!device_link_add(&pdev->dev, &smc->dev,
			     DL_FLAG_AUTOREMOVE_CONSUMER)) {
		put_device(&smc->dev);
		return -ENOMEM;
	}
	put_device(&smc->dev);
	ret = j704_validate_tables();
	if (ret)
		return ret;
	mutex_lock(&j704_start_lock);
	if (j704_start_consumed) {
		ret = -EBUSY;
		goto out;
	}
	if (THIS_MODULE && !try_module_get(THIS_MODULE)) {
		ret = -ENODEV;
		goto out;
	}
	j704_start_consumed = true;
	for (i = 0; i < ARRAY_SIZE(stages); i++) {
		ret = stages[i].run();
		if (ret) {
			dev_err(&pdev->dev,
				"%s startup failed: %d; partial state pinned until reboot\n",
				stages[i].name, ret);
			goto out;
		}
	}
	dev_info(&pdev->dev, "radio and SD PCIe startup complete\n");
out:
	mutex_unlock(&j704_start_lock);
	return ret;
}
static const struct of_device_id j704_platform_match[] = {
	{ .compatible = "apple,j704-platform" },
	{},
};
MODULE_DEVICE_TABLE(of, j704_platform_match);
static struct platform_driver j704_platform_driver = {
	.probe = j704_platform_probe,
	.driver = { .name = "apple-j704-platform",
		    .of_match_table = j704_platform_match,
		    .suppress_bind_attrs = true },
};
module_platform_driver(j704_platform_driver);
MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("Apple J704 ordered PCIe platform startup");
