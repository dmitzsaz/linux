// SPDX-License-Identifier: GPL-2.0
/* Configure measured root DBI/mux/power and train link0 before enumeration. */
#include <linux/bitops.h>
#include <linux/mfd/macsmc.h>
#include <linux/delay.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include "platform-data.h"
#include "pcie.h"
static struct j704_tunable *root_tunables;
static unsigned int root_tunables_count;
static struct j704_tunable *gen3_tunables;
static unsigned int gen3_tunables_count;
static struct j704_tunable *gen4_tunables;
static unsigned int gen4_tunables_count;
static void __iomem *cfg, *port, *gpio, *dart;
static void change(void __iomem *p, u32 clear, u32 set)
{
	writel((readl(p) & ~clear) | set, p);
}
static void table(const struct j704_tunable *t, unsigned int n)
{
	unsigned int i;
	for (i = 0; i < n; i++)
		change(cfg + t[i].off, t[i].mask, t[i].val);
}
static int start(void)
{
	struct device_node *np;
	struct platform_device *pdev, *smcdev;
	struct apple_smc *smc;
	struct apple_smc_key_info info;
	unsigned int sid;
	u32 id, cap, status;
	int ret;
	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	np = of_find_node_by_path("/soc/iommu@488000000");
	if (!np)
		return -ENODEV;
	pdev = of_find_device_by_node(np);
	of_node_put(np);
	if (!pdev)
		return -ENODEV;
	ret = device_is_bound(&pdev->dev);
	put_device(&pdev->dev);
	if (!ret)
		return -ENODEV;
	np = of_find_node_by_path("/soc/smc@38c600000");
	if (!np)
		return -ENODEV;
	smcdev = of_find_device_by_node(np);
	of_node_put(np);
	if (!smcdev)
		return -ENODEV;
	if (!device_is_bound(&smcdev->dev))
		return -ENODEV;
	smc = dev_get_drvdata(&smcdev->dev);
	if (!smc)
		return -ENODEV;
	ret = apple_smc_get_key_info(smc, SMC_KEY(gP13), &info);
	if (ret || info.size != 4 || !(info.flags & APPLE_SMC_WRITABLE))
		return ret ?: -EOPNOTSUPP;

	port = ioremap_np(0x488028000ULL, 0x8000);
	gpio = ioremap_np(0x39a000000ULL, 0x100000);
	dart = ioremap_np(0x488000000ULL, 0x20000);
	if (!port || !gpio || !dart)
		return -ENOMEM;
	if ((readl(gpio + 4 * 192) & 0x7f) != 2 || !(readl(port + 0x804) & 1) ||
	    readl(port + 0x208) & 1 || readl(dart + 0x100) & BIT(31))
		return -EBUSY;
	for (sid = 0; sid < 19; sid++)
		if (readl(dart + 0x1000 + 4 * sid) ||
		    readl(dart + 0x1400 + 4 * sid) & 1)
			return -EBUSY;
	cfg = ioremap_np(0x1cb0000000ULL, 0x4000);
	if (!cfg)
		return -ENOMEM;
	pr_debug("J704_RADIO_LINK before root ECAM identity read\n");
	id = readl(cfg);
	cap = readl(cfg + 0x7c);
	pr_debug("J704_RADIO_LINK root-id=%#x class=%#x linkcap=%#x command=%#x\n",
		id, readl(cfg + 8), cap, readl(cfg + 4));
	if (id != 0x100c106b || (readl(cfg + 8) >> 16) != 0x604 ||
	    !(cap & 0x3f0))
		return -ENODEV;
	change(cfg + 0x8bc, 0, 1);
	table(root_tunables, root_tunables_count);
	table(gen3_tunables, gen3_tunables_count);
	table(gen4_tunables, gen4_tunables_count);
	change(cfg + 0x7c, 0xf, 2);
	change(cfg + 0x9c, 0x7e, 6);
	writew((readw(cfg + 0xa0) & ~0xf) | 2, cfg + 0xa0);
	change(cfg + 0x710, 0x3f0000, 0x10000);
	change(cfg + 0x80c, 0x1f00, 0x100);
	change(cfg + 0x7c, 0x3f0, 0x10);
	change(cfg + 0x80c, 0, BIT(17));
	change(cfg + 0x8bc, 1, 0);
	/* The board's CLKREQ peripheral mux and Apple APPCLK policy. */
	change(gpio + 4 * 164, 0x60, 0x40 | BIT(9));
	change(port + 0x800, BIT(8), 0);
	/* AMFM's independent REG_ON is necessary: PERST alone never trains. */
	ret = apple_smc_write_u32(smc, SMC_KEY(gP13), 0x00800001);
	pr_debug("J704_RADIO_LINK REG_ON result=%d\n", ret);
	if (ret)
		return ret;
	msleep(150);
	change(gpio + 4 * 192, 0, 1);
	msleep(100);
	writel(1, port + 0x80);
	ret = readl_poll_timeout(port + 0x208, status, status & 1, 1000,
				 1000000);
	pr_debug("J704_RADIO_LINK link=%#x result=%d root-lnkctlsta=%#x; no bus enumeration or DMA\n",
		status, ret, readl(cfg + 0x80));
	return ret;
}
int j704_pcie_radio(void)
{
	int ret;
	ret = j704_load_tunables("port0", "apple,tunable-root", 0x4000,
				 &root_tunables, &root_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("port0", "apple,tunable-gen3", 0x4000,
				 &gen3_tunables, &gen3_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("port0", "apple,tunable-gen4", 0x4000,
				 &gen4_tunables, &gen4_tunables_count);
	if (ret)
		goto out;
	ret = start();
out:
	kfree(root_tunables);
	kfree(gen3_tunables);
	kfree(gen4_tunables);
	return ret;
}
