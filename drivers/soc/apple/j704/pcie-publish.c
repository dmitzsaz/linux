// SPDX-License-Identifier: GPL-2.0
/*
 * Publish one PCIe host for BOTH preinitialized J704 ports: the radio on port 0
 * (Wi-Fi + Bluetooth, with its DART) and the built-in SD card reader on port 2.
 * Both ports are staged by the ordered platform driver before this one, and this stage
 * refuses unless the current boot's radio DART, interrupt controller and both
 * port links are exactly as expected. It publishes a description only; no
 * endpoint driver is requested here.
 */
#include <linux/bitops.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/slab.h>
#include <linux/string.h>
#include "enable-node.h"

#include "platform-data.h"
#include "pcie.h"

int j704_pcie_publish(void)
{
	struct device_node *np;
	struct platform_device *pdev;
	void __iomem *port0 = NULL, *port2 = NULL, *dart = NULL, *dart2 = NULL;
	unsigned int sid;
	phandle dart_phandle;
	struct device_node *enable[2];
	u32 map[12];
	int ret;

	if (!j704_platform_ready())
		return -ENODATA;
	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	np = of_find_node_by_path("/soc/pcie@1cb0000000");
	if (!np)
		return -ENODATA;
	if (of_device_is_available(np)) {
		of_node_put(np);
		return -EEXIST;
	}
	of_node_put(np);

	np = of_find_node_by_path("/soc/interrupt-controller@381000000");
	if (!np)
		return -ENODEV;
	ret = np->phandle && of_device_is_compatible(np, "apple,t8142-aic3");
	of_node_put(np);
	if (!ret)
		return -EINVAL;
	/* The radio DART must already be bound; its phandle comes from this tree. */
	np = of_find_node_by_path("/soc/iommu@488000000");
	if (!np)
		return -ENODEV;
	dart_phandle = np->phandle;
	if (!dart_phandle) {
		of_node_put(np);
		return -EINVAL;
	}
	pdev = of_find_device_by_node(np);
	of_node_put(np);
	if (!pdev)
		return -ENODEV;
	ret = device_is_bound(&pdev->dev);
	put_device(&pdev->dev);
	if (!ret)
		return -ENODEV;

	np = of_find_node_by_path("/soc/iommu@48a000000");
	if (!np)
		return -ENODATA;
	if (of_device_is_available(np)) {
		of_node_put(np);
		return -EEXIST;
	}
	of_node_put(np);
	port0 = ioremap_np(0x488028000ULL, 0x8000);
	port2 = ioremap_np(0x48a028000ULL, 0x8000);
	dart = ioremap_np(0x488000000ULL, 0x20000);
	dart2 = ioremap_np(0x48a000000ULL, 0x20000);
	if (!port0 || !port2 || !dart || !dart2) {
		ret = -ENOMEM;
		goto out;
	}
	/* Both ports must already run and carry a live link, and the radio DART
	 * must not be locked. Port 2's own staging leaves no stream routing. */
	if (!(readl(port0 + 0x804) & 1) || !(readl(port0 + 0x208) & 1) ||
	    readl(dart + 0x100) & BIT(31)) {
		pr_debug("J704_PCIE_BOTH port 0 is not staged and linked; refusing\n");
		ret = -EBUSY;
		goto out;
	}
	if (!(readl(port2 + 0x804) & 1) || !(readl(port2 + 0x208) & 1)) {
		pr_debug("J704_PCIE_BOTH port 2 is not staged and linked; refusing\n");
		ret = -EBUSY;
		goto out;
	}
	/*
	 * Port 2's own DART is published here too, so the reader gets an ordinary
	 * stream instead of untranslated DMA. Attach it only while it is dormant,
	 * with the same checks the radio DART uses: expected identity, unlocked, no
	 * inherited valid roots, every stream disabled, no errors or active DMA.
	 * Nothing can DMA through it yet: no bus master is enabled before the host
	 * driver binds. The ADT's reserved streams are left exactly as they are; the
	 * radio's static bypass writes are deliberately not copied here.
	 */
	pr_debug("J704_PCIE_BOTH dart2 id=%#x sids=%u lock=%#x err=%#x enable=%#x\n",
		readl(dart2 + 8), readl(dart2 + 12) & 0x1ff,
		readl(dart2 + 0x200), readl(dart2 + 0x100),
		readl(dart2 + 0xc00));
	if (readl(dart2 + 8) != 0x2a2a0204 ||
	    (readl(dart2 + 12) & 0x1ff) != 19 || readl(dart2 + 0x200) & 1 ||
	    readl(dart2 + 0x208) || readl(dart2 + 0xc00) ||
	    readl(dart2 + 0x100) & BIT(31) || readl(dart2 + 0x104) ||
	    readl(dart2 + 0x1c0) || readl(dart2 + 0x4000) ||
	    readl(dart2 + 0x704) & 0x777) {
		pr_debug("J704_PCIE_BOTH dart2 is not dormant; refusing\n");
		ret = -EBUSY;
		goto out;
	}
	for (sid = 0; sid < 19; sid++) {
		if (readl(dart2 + 0x1400 + 4 * sid) & 1 ||
		    (readl(dart2 + 0x1000 + 4 * sid) & ~1u)) {
			pr_debug("J704_PCIE_BOTH dart2 stream %u is live; refusing\n",
				sid);
			ret = -EBUSY;
			goto out;
		}
	}
	enable[0] = of_find_node_by_path("/soc/iommu@48a000000");
	enable[1] = of_find_node_by_path("/soc/pcie@1cb0000000");
	if (!enable[0] || !enable[1] || !enable[0]->phandle ||
	    of_property_count_u32_elems(enable[1], "iommu-map") !=
		    ARRAY_SIZE(map) ||
	    of_property_read_u32_array(enable[1], "iommu-map", map,
				       ARRAY_SIZE(map)) ||
	    map[0] != 0x100 || map[1] != dart_phandle || map[2] != 1 ||
	    map[3] != 1 || map[4] != 0x101 || map[5] != dart_phandle ||
	    map[6] != 2 || map[7] != 1 || map[8] != 0x200 ||
	    map[9] != enable[0]->phandle || map[10] != 1 || map[11] != 1) {
		ret = -EINVAL;
	} else {
		pr_debug("J704_PCIE_BOTH enabling the described port0+port2 host after dormant-DART checks\n");
		ret = j704_enable_nodes(enable, ARRAY_SIZE(enable));
	}
	of_node_put(enable[0]);
	of_node_put(enable[1]);
out:
	if (port0)
		iounmap(port0);
	if (port2)
		iounmap(port2);
	if (dart)
		iounmap(dart);
	if (dart2)
		iounmap(dart2);
	return ret;
}

/* The enabled hierarchy and staging state are retained until reboot. */
