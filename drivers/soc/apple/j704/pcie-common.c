// SPDX-License-Identifier: GPL-2.0
/* Bounded T8142 common/Gen4 PHY sequence, matching the native T8142 startup order. */
#include <linux/bitops.h>
#include <linux/delay.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/mfd/syscon.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/regmap.h>

#include "platform-data.h"
#include "pcie.h"
static struct j704_tunable *axi_tunables;
static unsigned int axi_tunables_count;
static struct j704_tunable *phy_tunables;
static unsigned int phy_tunables_count;
static struct j704_tunable *pll_tunables;
static unsigned int pll_tunables_count;
static struct j704_tunable *auspma_tunables;
static unsigned int auspma_tunables_count;

static void __iomem *rc, *glue, *ip, *axi;

static void change(void __iomem *address, u32 clear, u32 set)
{
	u32 value = readl(address);
	writel((value & ~clear) | set, address);
}

static int table(void __iomem *base, u32 span, const struct j704_tunable *t,
		 unsigned int count, const char *name)
{
	unsigned int i;
	for (i = 0; i < count; i++)
		if (t[i].off & 3 || t[i].off > span - 4)
			return -EINVAL;
	pr_debug("J704_PCIE_COLD tunables=%s entries=%u\n", name, count);
	for (i = 0; i < count; i++)
		change(base + t[i].off, t[i].mask, t[i].val);
	return 0;
}

static int sequence(void)
{
	void __iomem *phy = glue + 0x20000;
	void __iomem *clock = glue + 0x4000;
	u32 value;
	int ret;

	ret = table(axi, 0x1000000, axi_tunables, axi_tunables_count, "axi2af");
	if (ret)
		return ret;
	writel(0, rc + 4);
	ret = table(glue, 0x40000, phy_tunables, phy_tunables_count,
		    "phy-glue");
	if (ret)
		return ret;
	ret = readl_poll_timeout(clock, value, value & BIT(31), 10, 250000);
	pr_debug("J704_PCIE_COLD refclk=%#x status=%d\n", value, ret);
	if (ret)
		return ret;

	change(phy, 0, BIT(0));
	ret = readl_poll_timeout(phy, value, value & BIT(2), 10, 50000);
	pr_debug("J704_PCIE_COLD clk0=%#x status=%d\n", value, ret);
	if (ret)
		return ret;
	udelay(1);
	change(phy, 0, BIT(1));
	ret = readl_poll_timeout(phy, value, value & BIT(3), 10, 50000);
	pr_debug("J704_PCIE_COLD clk1=%#x status=%d\n", value, ret);
	if (ret)
		return ret;
	change(phy, BIT(4), 0);
	udelay(1);
	change(phy + 4, 0, BIT(0));
	ret = table(ip, 0x20000, pll_tunables, pll_tunables_count, "pll");
	if (ret)
		return ret;
	ret = table(ip, 0x20000, auspma_tunables, auspma_tunables_count,
		    "auspma");
	if (ret)
		return ret;
	change(ip + 0x90, BIT(0), 0);
	change(phy + 4, 0, BIT(4));
	ret = readl_poll_timeout(phy + 8, value, value & BIT(0), 10, 250000);
	pr_debug("J704_PCIE_COLD phy-ready=%#x status=%d\n", value, ret);
	if (ret)
		return ret;
	change(clock, BIT(0), BIT(0));
	change(phy, BIT(27), BIT(27));

	writel(0x140, rc + 0x54);
	change(rc + 0x50, 0, BIT(0));
	ret = readl_poll_timeout(rc + 0x58, value, value & BIT(0), 10, 250000);
	pr_debug("J704_PCIE_COLD rc-ready=%#x status=%d\n", value, ret);
	return ret;
}

static int j704_pcie_cold_init(void)
{
	static const unsigned int gates[] = { 0x300, 0x2f0, 0x408, 0x410,
					      0x520, 0x560, 0x578, 0x580 };
	struct device_node *node;
	struct regmap *pmgr;
	unsigned int i, state;
	int ret = 0;

	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	node = of_find_node_by_path("/soc/power-management@380700000");
	if (!node)
		return -ENODEV;
	pmgr = syscon_node_to_regmap(node);
	of_node_put(node);
	if (IS_ERR(pmgr))
		return PTR_ERR(pmgr);
	for (i = 0; i < ARRAY_SIZE(gates); i++) {
		ret = regmap_read(pmgr, gates[i], &state);
		if (ret)
			return ret;
		if (((state >> 4) & 0xf) != 0xf || state & BIT(28))
			return -EAGAIN;
	}
	rc = ioremap_np(0x48c000000ULL, 0x4000);
	glue = ioremap_np(0x48f000000ULL, 0x40000);
	ip = ioremap_np(0x48f040000ULL, 0x20000);
	axi = ioremap_np(0x48e000000ULL, 0x1000000);
	if (!rc || !glue || !ip || !axi) {
		if (rc)
			iounmap(rc);
		if (glue)
			iounmap(glue);
		if (ip)
			iounmap(ip);
		if (axi)
			iounmap(axi);
		return -ENOMEM;
	}
	/* PHY-ready may deassert after its dynamic clock handoff. The clock
	 * requests/ACKs and released reset are the stable inherited state. Do
	 * not reset an already running RC when that state is inconsistent. */
	if (readl(rc + 0x58) & BIT(0)) {
		if ((readl(glue + 0x20000) & 0x1f) != 0xf ||
		    (readl(glue + 0x20004) & 0x11) != 0x11)
			ret = -EBUSY;
		else
			pr_debug("J704 PCIe common PHY already ready\n");
	} else {
		ret = sequence();
	}
	return ret;
}

/* Retain the mappings and state after any partially completed transaction. */
int j704_pcie_common(void)
{
	int ret;
	ret = j704_load_tunables("shared", "apple,tunable-axi2af", 0x1000000,
				 &axi_tunables, &axi_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("shared", "apple,tunable-phy", 0x40000,
				 &phy_tunables, &phy_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("shared", "apple,tunable-pll", 0x20000,
				 &pll_tunables, &pll_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("shared", "apple,tunable-auspma", 0x20000,
				 &auspma_tunables, &auspma_tunables_count);
	if (ret)
		goto out;
	ret = j704_pcie_cold_init();
out:
	kfree(axi_tunables);
	kfree(phy_tunables);
	kfree(pll_tunables);
	kfree(auspma_tunables);
	return ret;
}
