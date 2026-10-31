// SPDX-License-Identifier: GPL-2.0
/* Enable the described radio DART only while its streams and endpoint are dormant. */
#include <linux/init.h>
#include <linux/io.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include "enable-node.h"
#include "platform-data.h"
#include "pcie.h"

int j704_pcie_dart(void)
{
	struct device_node *np, *enable[1];
	void __iomem *dart, *port, *gpio;
	unsigned int sid;
	int ret;
	if (!j704_platform_ready())
		return -ENODATA;
	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	np = of_find_node_by_path("/soc/interrupt-controller@381000000");
	if (!np)
		return -ENODEV;
	ret = np->phandle && of_device_is_compatible(np, "apple,t8142-aic3");
	of_node_put(np);
	if (!ret)
		return -EINVAL;
	np = of_find_node_by_path("/soc/iommu@488000000");
	if (!np)
		return -ENODATA;
	if (of_device_is_available(np)) {
		of_node_put(np);
		return -EEXIST;
	}
	of_node_put(np);
	gpio = ioremap_np(0x39a000000ULL, 0x100000);
	port = ioremap_np(0x488028000ULL, 0x8000);
	dart = ioremap_np(0x488000000ULL, 0x20000);
	if (!gpio || !port || !dart)
		return -ENOMEM;
	if ((readl(gpio + 4 * 192) & 0x7f) != 2 || !(readl(port + 0x804) & 1) ||
	    readl(port + 0x208) & 1)
		return -EBUSY;
	if (readl(dart + 8) != 0x2a2a0204 || (readl(dart + 12) & 0x1ff) != 19 ||
	    readl(dart + 0x200) & 1 || readl(dart + 0x208) ||
	    readl(dart + 0xc00) || readl(dart + 0x100) & BIT(31) ||
	    readl(dart + 0x104))
		return -EBUSY;
	for (sid = 0; sid < 19; sid++) {
		if (readl(dart + 0x1400 + 4 * sid) & 1 ||
		    (readl(dart + 0x1000 + 4 * sid) & ~1))
			return -EBUSY;
	}
	if (readl(dart + 0x1c0) || readl(dart + 0x4000) ||
	    readl(dart + 0x704) & 0x777)
		return -EBUSY;
	pr_debug("J704_RADIO_DART attaching dormant unlocked DART v2.4; no valid roots, streams disabled, external reset held\n");
	enable[0] = of_find_node_by_path("/soc/iommu@488000000");
	ret = j704_enable_nodes(enable, 1);
	of_node_put(enable[0]);
	if (ret)
		return ret;
	pr_debug("J704_RADIO_DART enabled; driver TCR16=%#x TCR18=%#x enable=%#x\n",
		readl(dart + 0x1040), readl(dart + 0x1048),
		readl(dart + 0xc00));
	/* Reserve the ADT's static bypass roles, independent of normal SIDs1/2.
  * No endpoint DMA can begin: GPIO192 remains asserted, PCI host absent.
  */
	if (readl(dart + 0x200) & 1 || !(readl(dart + 0xc00) & BIT(16)) ||
	    readl(dart + 0x1040) || readl(dart + 0x1048))
		return -EBUSY;
	writel(6, dart + 0x1040);
	writel(6, dart + 0x1048);
	pr_debug("J704_RADIO_DART static bypass16=%#x bypass18=%#x; endpoint stays reset\n",
		readl(dart + 0x1040), readl(dart + 0x1048));
	return 0;
}
