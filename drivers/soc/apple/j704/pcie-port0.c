// SPDX-License-Identifier: GPL-2.0
/* J704 native port0 clock/RUN startup. No ECAM, enumeration or DMA writes.
 * This is not a PCI host driver. It deliberately leaves external PERST# low.
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
static struct j704_tunable *port_config;
static unsigned int port_config_count;
static void __iomem *port, *lane, *gpio, *common, *phy;
static void change(void __iomem *p, u32 clear, u32 set)
{
	writel((readl(p) & ~clear) | set, p);
}
static int stage(void)
{
	/* AFISOCNI4 (0x280) is a no_ps software parent in the ADT. Its
	 * register image is not a power-state acknowledgement. Only check
	 * the physical gates enabled by j704_pcie_power(). */
	static const u32 gates[] = { 0x280, 0x300, 0x2f0, 0x408, 0x410,
				     0x520, 0x560, 0x578, 0x580 };
	/* Reset defaults traced from AppleT8142PCIePort's inherited Gen4 method.
  * RID/SID, MSI tables and interrupt acknowledgements are excluded here. */
	static const struct {
		u32 off, val;
	} reset[] = { { 0x88, 0x110 },	    { 0x80, 0 },
		      { 0x84, 0 },	    { 0x104, 0xfffffff0 },
		      { 0x124, 0x100 },	    { 0x16c, 0 },
		      { 0x13c, 0 },	    { 0x800, 0x100100 },
		      { 0x808, 0x1001ff },  { 0x82c, 0x10000 },
		      { 0x130, 0x3020000 }, { 0x140, 0x10 },
		      { 0x144, 0x253770 },  { 0x21c, 0 },
		      { 0x834, 0 },	    { 0x83c, 0 } };
	struct device_node *np;
	struct regmap *pmgr;
	u32 v;
	unsigned int i;
	int ret;
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
	/* Skip the no_ps parent at index 0. */
	for (i = 1; i < ARRAY_SIZE(gates); i++) {
		ret = regmap_read(pmgr, gates[i], &v);
		if (ret)
			return ret;
		pr_debug("J704_PORT_STAGE power offset=%#x value=%#x\n",
			gates[i], v);
		if ((v & 0xff) != 0xff || v & BIT(28)) {
			pr_err("J704_PORT_STAGE power not ready: offset=%#x value=%#x\n",
			       gates[i], v);
			return -EAGAIN;
		}
	}
	common = ioremap_np(0x48c000000ULL, 0x4000);
	phy = ioremap_np(0x48f020000ULL, 0x4000);
	lane = ioremap_np(0x48f010000ULL, 0x4000);
	gpio = ioremap_np(0x39a000000ULL, 0x100000);
	port = ioremap_np(0x488028000ULL, 0x8000);
	if (!common || !lane || !gpio || !port || !phy)
		return -ENOMEM;
	pr_debug("J704_PORT_STAGE common=%#x phy-ready=%#x ctrl=%#x aux=%#x lane=%#x\n",
		readl(common + 0x58), readl(phy + 8), readl(phy),
		readl(phy + 4), readl(lane));
	/* PHY ready is transient after the common dynamic clock handoff.
  * Its requested clocks/ACKs and released reset remain a stable admission.
  * Request this port's lane clocks before accessing its local register bank.
  */
	if (!(readl(common + 0x58) & 1) || (readl(phy) & 0x1f) != 0xf ||
	    (readl(phy + 4) & 0x11) != 0x11) {
		pr_err("J704_PORT_STAGE shared PHY not ready: RUN=%#x ACK=%#x ctrl=%#x aux=%#x\n",
		       readl(common + 0x50), readl(common + 0x58),
		       readl(phy), readl(phy + 4));
		return -EAGAIN;
	}
	if ((readl(gpio + 4 * 192) & 0x7f) != 2)
		return -EBUSY;
	pr_debug("J704_PORT_STAGE external PERST# low; requesting port0 lane clocks\n");
	change(lane, 0, BIT(0));
	ret = readl_poll_timeout(lane, v, v & BIT(2), 10, 50000);
	pr_debug("J704_PORT_STAGE clk0=%#x result=%d\n", v, ret);
	if (ret)
		return ret;
	change(lane, 0, BIT(1));
	ret = readl_poll_timeout(lane, v, v & BIT(3), 10, 50000);
	pr_debug("J704_PORT_STAGE clk1=%#x result=%d\n", v, ret);
	if (ret)
		return ret;
	change(lane, BIT(4), 0);
	udelay(1);
	change(lane, 0, BIT(9) | BIT(10));
	pr_debug("J704_PORT_STAGE before APPCLK read\n");
	v = readl(port + 0x800);
	pr_debug("J704_PORT_STAGE appclk=%#x\n", v);
	pr_debug("J704_PORT_STAGE lane=%#x perst-gpio=%#x\n", readl(lane),
		readl(gpio + 4 * 192));
	/* External reset must already be held low, output mode; no live replay. */
	if ((readl(gpio + 4 * 192) & 0x7f) != 2 || (readl(port + 0x208) & 1))
		return -EBUSY;
	/* Refuse inherited routes; reserved SID16..18 are never written. */
	for (i = 0; i < 16; i++)
		if (readl(port + 0x3000 + 4 * i))
			return -EBUSY;
	for (i = 0; i < port_config_count; i++)
		if (port_config[i].off & 3 || port_config[i].off > 0x7ffc)
			return -EINVAL;
	pr_debug("J704_PORT_STAGE holding external PERST# low; starting cold clock/RUN stage\n");
	for (i = 0; i < ARRAY_SIZE(reset); i++)
		writel(reset[i].val, port + reset[i].off);
	for (i = 0; i < port_config_count; i++)
		change(port + port_config[i].off, port_config[i].mask,
		       port_config[i].val);
	change(port + 0x800, 0, BIT(0));
	change(port + 0x82c, 0, BIT(0));
	ret = readl_poll_timeout(port + 0x804, v, v & BIT(0), 100, 250000);
	pr_debug("J704_PORT_STAGE RUN=%#x result=%d; external PERST# remains low; no DART/ECAM/DMA access\n",
		v, ret);
	return ret;
}
/* Retain state even on partial initialization. */
int j704_pcie_port0(void)
{
	int ret;
	ret = j704_load_tunables("port0", "apple,tunable-config", 0x8000,
				 &port_config, &port_config_count);
	if (ret)
		goto out;
	ret = stage();
out:
	kfree(port_config);
	return ret;
}
