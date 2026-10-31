// SPDX-License-Identifier: GPL-2.0
/*
 * J704 PCIe port 2 (built-in SD card reader) root configuration and link training, modelled on
 * the qualified port-0 sequence in the qualified J704 sequence Run after the port-2 clock/RUN
 * stage. It powers the reader through the board's own SMC key and releases its PERST#, then
 * starts LTSSM. No bus enumeration, no DART, no DMA, no media access.
 *
 * Board wiring from the J704 board description:
 *   pci-bridge2 apcie-port 2, root2 ECAM 0x1cb0010000, port2 bank 0x48a028000,
 *   function-perst gpio0 pin 194, function-clkreq gpio0 pin 166,
 *   function-sd_pwr_en smc-pmu pKW4("gP19", 0) - the SD power rail.
 */
#include <linux/bitops.h>
#include <linux/delay.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/mfd/macsmc.h>
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


#define PORT2 0x48a028000ULL
#define ECAM_ROOT2 0x1cb0010000ULL
#define GPIO0 0x39a000000ULL
#define PERST_PIN 194
#define CLKREQ_PIN 166

static void __iomem *cfg, *port, *gpio;

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
	struct platform_device *smcdev;
	struct apple_smc_key_info info;
	struct apple_smc *smc;
	u32 id, class, cap, status;
	int ret;

	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	np = of_find_node_by_path("/soc/smc@38c600000");
	if (!np)
		return -ENODEV;
	smcdev = of_find_device_by_node(np);
	of_node_put(np);
	if (!smcdev || !device_is_bound(&smcdev->dev))
		return -ENODEV;
	smc = dev_get_drvdata(&smcdev->dev);
	if (!smc)
		return -ENODEV;
	/* The board's own SD power-enable key must exist and be writable. */
	ret = apple_smc_get_key_info(smc, SMC_KEY(gP19), &info);
	if (ret || info.size != 4 || !(info.flags & APPLE_SMC_WRITABLE)) {
		pr_debug("J704_SD_LINK gP19 info result=%d size=%u flags=%#x\n",
			ret, ret ? 0 : info.size, ret ? 0 : info.flags);
		return ret ?: -EOPNOTSUPP;
	}

	port = ioremap_np(PORT2, 0x8000);
	gpio = ioremap_np(GPIO0, 0x100000);
	if (!port || !gpio)
		return -ENOMEM;
	/* The SD-booting U-Boot powered the reader, released PERST# and trained the link. */
	if ((readl(port + 0x804) & 1) && (readl(port + 0x208) & 1) &&
	    (readl(gpio + 4 * PERST_PIN) & 0x7f) == 3) {
		pr_debug("J704_SD_LINK port2 linked by the bootloader link=%#x; left as is\n",
			readl(port + 0x208));
		return 0;
	}
	/* The clock/RUN stage must have run, nothing may be linked yet, PERST# stays asserted. */
	if (!(readl(port + 0x804) & 1) || (readl(port + 0x208) & 1) ||
	    (readl(gpio + 4 * PERST_PIN) & 0x7f) != 2) {
		pr_debug("J704_SD_LINK port2 run=%#x link=%#x perst=%#x: refusing\n",
			readl(port + 0x804), readl(port + 0x208),
			readl(gpio + 4 * PERST_PIN));
		return -EBUSY;
	}

	cfg = ioremap_np(ECAM_ROOT2, 0x4000);
	if (!cfg)
		return -ENOMEM;
	pr_debug("J704_SD_LINK before root2 ECAM identity read\n");
	id = readl(cfg);
	class = readl(cfg + 8);
	cap = readl(cfg + 0x7c);
	pr_debug("J704_SD_LINK root2-id=%#x class=%#x linkcap=%#x command=%#x\n",
		id, class, cap, readl(cfg + 4));
	if ((id & 0xffff) != 0x106b || (class >> 16) != 0x604 || !(cap & 0x3f0))
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
	/* The reader's ADT initial-link-speed is 1; keep the first training at Gen1. */
	change(cfg + 0x7c, 0x3f0, 0x10);
	change(cfg + 0x80c, 0, BIT(17));
	change(cfg + 0x8bc, 1, 0);
	/* Board CLKREQ peripheral mux and Apple APPCLK policy, as on port 0. */
	change(gpio + 4 * CLKREQ_PIN, 0x60, 0x40 | BIT(9));
	change(port + 0x800, BIT(8), 0);

	ret = apple_smc_write_u32(smc, SMC_KEY(gP19), 1);
	pr_debug("J704_SD_LINK sd_pwr_en result=%d\n", ret);
	if (ret)
		return ret;
	msleep(150);
	change(gpio + 4 * PERST_PIN, 0, 1);
	msleep(100);
	writel(1, port + 0x80);
	ret = readl_poll_timeout(port + 0x208, status, status & 1, 1000,
				 1000000);
	pr_debug("J704_SD_LINK link=%#x result=%d root-lnkctlsta=%#x; no enumeration or DMA\n",
		status, ret, readl(cfg + 0x80));
	return ret;
}

/* The platform owner retains hardware state until reboot. */
int j704_pcie_sd(void)
{
	int ret;
	ret = j704_load_tunables("port2", "apple,tunable-root", 0x4000,
				 &root_tunables, &root_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("port2", "apple,tunable-gen3", 0x4000,
				 &gen3_tunables, &gen3_tunables_count);
	if (ret)
		goto out;
	ret = j704_load_tunables("port2", "apple,tunable-gen4", 0x4000,
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
