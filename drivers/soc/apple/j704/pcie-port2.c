// SPDX-License-Identifier: GPL-2.0
/*
 * J704 Apple PCIe port 2 (the built-in SD card reader's root port) clock/RUN stage, modelled
 * exactly on the qualified port-0 stage in the qualified J704 sequence No ECAM, enumeration,
 * DMA or DART access, and the external PERST# of the reader stays asserted.
 *
 * Port 2 is powered down at boot: its register bank answers only after this port's lane clocks
 * are requested through the PHY lane window, which is in the already-powered PHY glue region.
 * Mapping 0x48a028000 before that raises an asynchronous SError (asahi-render67-boot).
 *
 * Addresses are the ADT windows plus 0x210000000 (the qualified J704 sequence):
 * port2 0x48a028000, lane2 0x48f018000, shared PHY control 0x48f020000, RC common 0x48c000000,
 * gpio0 0x39a000000 with PERST# on pin 194 and CLKREQ# on pin 166.
 */
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

static struct j704_tunable *config;
static unsigned int config_count;


#define PORT2 0x48a028000ULL
#define LANE2 0x48f018000ULL
#define LANE0 0x48f010000ULL
#define PHY_CTRL 0x48f020000ULL
#define RC_COMMON 0x48c000000ULL
#define GPIO0 0x39a000000ULL
#define PERST_PIN 194
#define CLKREQ_PIN 166

static void change(void __iomem *p, u32 clear, u32 set)
{
	writel((readl(p) & ~clear) | set, p);
}

static int stage(void)
{
	/* The port-0 stage's traced reset defaults; the per-port banks share this layout. */
	static const struct {
		u32 off, val;
	} reset[] = {
		{ 0x88, 0x110 },      { 0x80, 0 },
		{ 0x84, 0 },	      { 0x104, 0xfffffff0 },
		{ 0x124, 0x100 },     { 0x16c, 0 },
		{ 0x13c, 0 },	      { 0x800, 0x100100 },
		{ 0x808, 0x1001ff },  { 0x82c, 0x10000 },
		{ 0x130, 0x3020000 }, { 0x140, 0x10 },
		{ 0x144, 0x253770 },  { 0x21c, 0 },
		{ 0x834, 0 },	      { 0x83c, 0 },
	};
	static const u32 gates[] = { 0x280, 0x300, 0x2f0, 0x408, 0x410,
				     0x520, 0x560, 0x578, 0x580 };
	void __iomem *common, *phy, *lane, *lane0, *gpio, *port;
	struct device_node *np;
	struct regmap *pmgr;
	unsigned int i;
	u32 v;
	int ret;

	if (!j704_platform_ready())
		return -ENODATA;
	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	np = of_find_node_by_path("/soc/power-management@380700000");
	if (!np)
		return -ENODEV;
	pmgr = syscon_node_to_regmap(np);
	of_node_put(np);
	if (IS_ERR(pmgr))
		return PTR_ERR(pmgr);
	for (i = 0; i < ARRAY_SIZE(gates); i++) {
		ret = regmap_read(pmgr, gates[i], &v);
		if (ret)
			return ret;
		pr_debug("J704_SD_STAGE power offset=%#x value=%#x\n", gates[i],
			v);
		if ((v & 0xff) != 0xff || (i && v & BIT(28)))
			return -EAGAIN;
	}

	common = ioremap_np(RC_COMMON, 0x4000);
	phy = ioremap_np(PHY_CTRL, 0x4000);
	lane = ioremap_np(LANE2, 0x4000);
	lane0 = ioremap_np(LANE0, 0x4000);
	gpio = ioremap_np(GPIO0, 0x100000);
	if (!common || !phy || !lane || !lane0 || !gpio)
		return -ENOMEM;

	pr_debug("J704_SD_STAGE common=%#x phy-ready=%#x ctrl=%#x aux=%#x lane2=%#x lane0=%#x\n",
		readl(common + 0x58), readl(phy + 8), readl(phy),
		readl(phy + 4), readl(lane), readl(lane0));
	pr_debug("J704_SD_STAGE gpio perst194=%#x clkreq166=%#x (port0 perst192=%#x clkreq164=%#x)\n",
		readl(gpio + 4 * PERST_PIN), readl(gpio + 4 * CLKREQ_PIN),
		readl(gpio + 4 * 192), readl(gpio + 4 * 164));

	/* Same admission as the port-0 stage: the shared PHY must be initialized and running. */
	if (!(readl(common + 0x58) & 1) || (readl(phy) & 0x1f) != 0xf ||
	    (readl(phy + 4) & 0x11) != 0x11)
		return -EAGAIN;

	pr_debug("J704_SD_STAGE requesting port2 lane clocks; external PERST# untouched\n");
	change(lane, 0, BIT(0));
	ret = readl_poll_timeout(lane, v, v & BIT(2), 10, 50000);
	pr_debug("J704_SD_STAGE clk0=%#x result=%d\n", v, ret);
	if (ret)
		return ret;
	change(lane, 0, BIT(1));
	ret = readl_poll_timeout(lane, v, v & BIT(3), 10, 50000);
	pr_debug("J704_SD_STAGE clk1=%#x result=%d\n", v, ret);
	if (ret)
		return ret;
	change(lane, BIT(4), 0);
	udelay(1);
	change(lane, 0, BIT(9) | BIT(10));

	port = ioremap_np(PORT2, 0x8000);
	if (!port)
		return -ENOMEM;
	pr_debug("J704_SD_STAGE before APPCLK read\n");
	v = readl(port + 0x800);
	pr_debug("J704_SD_STAGE appclk=%#x lane2=%#x\n", v, readl(lane));
	/*
	 * The SD-booting U-Boot leaves port 2 running and linked for the host
	 * driver to adopt; keep that state. Otherwise nothing may be linked yet.
	 */
	if (readl(port + 0x208) & 1) {
		if (readl(port + 0x804) & 1) {
			pr_debug("J704_SD_STAGE port2 running and linked by the bootloader; left as is\n");
			return 0;
		}
		pr_debug("J704_SD_STAGE port2 already reports link; leaving it untouched\n");
		return -EBUSY;
	}
	for (i = 0; i < 16; i++) {
		if (readl(port + 0x3000 + 4 * i)) {
			pr_debug("J704_SD_STAGE port2 rid2sid%u is populated; refusing\n",
				i);
			return -EBUSY;
		}
	}
	pr_debug("J704_SD_STAGE staging port2 register bank\n");
	for (i = 0; i < ARRAY_SIZE(reset); i++)
		writel(reset[i].val, port + reset[i].off);
	/* This port's values are supplied from its own live ADT child. */
	for (i = 0; i < config_count; i++)
		change(port + config[i].off, config[i].mask, config[i].val);
	change(port + 0x800, 0, BIT(0));
	change(port + 0x82c, 0, BIT(0));
	ret = readl_poll_timeout(port + 0x804, v, v & BIT(0), 100, 250000);
	pr_debug("J704_SD_STAGE RUN=%#x result=%d; PERST# still asserted, no DART/ECAM/DMA\n",
		v, ret);
	return ret;
}

int j704_pcie_port2(void)
{
	int ret;

	ret = j704_load_tunables("port2", "apple,tunable-config", 0x8000,
				 &config, &config_count);
	if (ret)
		return ret;
	ret = stage();
	kfree(config);
	return ret;
}

/* A partially staged lane must stay pinned until reboot. */
