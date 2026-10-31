// SPDX-License-Identifier: GPL-2.0
/* Power the shared J704 PCIe domains before accessing their register banks. */
#include <linux/init.h>
#include <linux/bitops.h>
#include <linux/iopoll.h>
#include <linux/io.h>
#include <linux/mfd/syscon.h>
#include <linux/module.h>
#include <linux/of.h>
#include <linux/regmap.h>

#include "platform-data.h"
#include "pcie.h"

int j704_pcie_power(void)
{
	static const struct {
		const char *name;
		unsigned int offset;
	} gates[] = {
		{ "ANS", 0x300 },	   { "APCIE_GP", 0x2f0 },
		{ "APCIE_SYS_GP", 0x408 }, { "APCIE_ST", 0x410 },
		{ "APCIE_SYS_ST", 0x520 }, { "APCIE_PHY_SW", 0x560 },
		{ "ST0_AUS5_AON", 0x578 }, { "ST0_AUS5", 0x580 },
	};
	struct device_node *node;
	struct regmap *pmgr;
	bool ready = true;
	unsigned int value;
	unsigned int i;
	int ret;

	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	if (!j704_platform_ready())
		return -ENODATA;
	node = of_find_node_by_path("/soc/power-management@380700000");
	if (!node)
		return -ENODEV;
	pmgr = syscon_node_to_regmap(node);
	of_node_put(node);
	if (IS_ERR(pmgr))
		return PTR_ERR(pmgr);

	for (i = 0; i < ARRAY_SIZE(gates); i++) {
		ret = regmap_read(pmgr, gates[i].offset, &value);
		if (ret)
			return ret;
		if (((value >> 4) & 0xf) != 0xf || value & BIT(28)) {
			/* Same mask and bounded transition as Aura pmgr_set_mode(). */
			ret = regmap_update_bits(
				pmgr, gates[i].offset,
				BIT(28) | BIT(9) | BIT(8) | GENMASK(3, 0), 0xf);
			if (!ret)
				ret = regmap_read_poll_timeout(
					pmgr, gates[i].offset, value,
					((value >> 4) & 0xf) == 0xf, 10, 10000);
			if (ret) {
				pr_err("J704_PCIE_POWER gate %s refused active: %d; not reading PCIe MMIO\n",
				       gates[i].name, ret);
				return ret;
			}
		}
		pr_debug("J704_PCIE_POWER_PD %s offset=%#x value=%#x target=%x actual=%x\n",
			gates[i].name, gates[i].offset, value, value & 0xf,
			(value >> 4) & 0xf);
		ready &= ((value >> 4) & 0xf) == 0xf;
	}
	return ready ? 0 : -ETIMEDOUT;
}

/* Active gates remain retained until reboot, including partial startup. */
