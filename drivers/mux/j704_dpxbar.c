// SPDX-License-Identifier: GPL-2.0-only
/*
 * J704 (T8142) display crossbar: the T8142 part of Aurora's apif-display
 * apple-display-crossbar.c (four pixel clocks, five outputs), as a separate
 * module so the stock mux-apple-display-crossbar stays untouched.
 *
 * Selecting a source only programs the two source nibbles (native connect
 * 0xfffffe00093f5238). The clocks are switched on by j704_dpxbar_link_up()
 * once the PHY supplies its pixel clock (native 0xfffffe00093f5600), and off
 * by j704_dpxbar_link_down() (native 0xfffffe00093f5d70).
 */

#include <linux/bits.h>
#include <linux/delay.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/module.h>
#include <linux/mux/driver.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/spinlock.h>

#include "j704_dpxbar.h"

#define T8142_MUX_MAX 5
#define T8142_N_UFP 8
#define T8142_MUX_DPPHY 0

struct j704_dpxbar {
	struct device *dev;
	void __iomem *regs;
	int selected[T8142_MUX_MAX];
	u8 pclk[T8142_MUX_MAX];
	bool uhbr[T8142_MUX_MAX];
	bool enabled[T8142_MUX_MAX];
	bool faulted;
	spinlock_t lock;
};

static const u8 dest_shift[T8142_MUX_MAX] = { 8, 0, 4, 12, 16 };
static const u8 select_shift[T8142_MUX_MAX] = { 8, 0, 4, 24, 0 };
static const u8 select_shift2[T8142_MUX_MAX] = { 20, 12, 16, 28, 4 };

static void xb_mask32(struct j704_dpxbar *xb, u32 reg, u32 mask, u32 set)
{
	u32 value = readl(xb->regs + reg);

	writel((value & ~mask) | set, xb->regs + reg);
}

static void xb_set32(struct j704_dpxbar *xb, u32 reg, u32 set)
{
	xb_mask32(xb, reg, 0, set);
}

static void xb_clear32(struct j704_dpxbar *xb, u32 reg, u32 clear)
{
	xb_mask32(xb, reg, clear, 0);
}

static int xb_wait_clear(struct j704_dpxbar *xb, u32 reg, u32 bits)
{
	u32 value;
	int ret;

	ret = readl_poll_timeout_atomic(xb->regs + reg, value, !(value & bits), 1, 1000);
	if (ret) {
		xb->faulted = true;
		dev_err(xb->dev, "status +0x%x still 0x%x (bits 0x%x)\n", reg, value, bits);
	}
	return ret;
}

static int t8142_link_down(struct j704_dpxbar *xb, unsigned int index)
{
	u32 src, dst, clock;
	int ret;

	if (!xb->enabled[index])
		return 0;
	src = BIT(xb->selected[index]);
	dst = BIT(dest_shift[index]);
	clock = xb->pclk[index] - 1;

	xb_clear32(xb, 0x3c, src);
	xb_clear32(xb, 0x00, src);
	xb_clear32(xb, 0x0c + 4 * clock, src);
	xb_clear32(xb, 0x24 + 4 * clock, dst);
	udelay(1);
	ret = xb_wait_clear(xb, 0x800, src);
	if (!ret)
		ret = xb_wait_clear(xb, 0x808 + 4 * clock, src);
	if (!ret)
		ret = xb_wait_clear(xb, 0x81c + 4 * clock, dst);
	if (ret)
		return ret;
	xb_clear32(xb, 0x08, src);
	xb_clear32(xb, 0x20, 7 << (3 * xb->selected[index]));
	xb_clear32(xb, 0x38, 7 << dest_shift[index]);
	xb_set32(xb, 0x04, src);
	xb_set32(xb, 0x1c, src);
	xb_set32(xb, 0x34, dst);
	xb->enabled[index] = false;
	return 0;
}

static int t8142_link_up(struct j704_dpxbar *xb, unsigned int index)
{
	int state = xb->selected[index];
	u32 clock, select, src, dst;
	int ret;

	if (state < 0)
		return -ENODEV;
	if (!xb->pclk[index])
		return -EINVAL;
	if (xb->enabled[index])
		return 0;

	/* The coding bit is shared by every output using this pixel clock. */
	for (unsigned int i = 0; i < T8142_MUX_MAX; i++)
		if (i != index && xb->enabled[i] && xb->pclk[i] == xb->pclk[index] &&
		    xb->uhbr[i] != xb->uhbr[index])
			return -EBUSY;

	clock = xb->pclk[index] - 1;
	select = 2 * clock + 1;
	src = BIT(state);
	dst = BIT(dest_shift[index]);
	xb_clear32(xb, 0x04, src);
	xb_clear32(xb, 0x1c, src);
	xb_clear32(xb, 0x34, dst);
	udelay(1);
	ret = xb_wait_clear(xb, 0x804, src);
	if (!ret)
		ret = xb_wait_clear(xb, 0x818, src);
	if (!ret)
		ret = xb_wait_clear(xb, 0x82c, dst);
	if (ret)
		return ret;
	xb_set32(xb, 0x08, src);
	xb_mask32(xb, 0x20, 7 << (3 * state), select << (3 * state));
	xb_mask32(xb, 0x38, 7 << dest_shift[index], select << dest_shift[index]);
	xb_mask32(xb, 0x48, BIT(clock), xb->uhbr[index] ? BIT(clock) : 0);
	xb_set32(xb, 0x00, src);
	xb_set32(xb, 0x0c + 4 * clock, src);
	xb_set32(xb, 0x24 + 4 * clock, dst);
	xb_set32(xb, 0x4c, dst);
	xb_set32(xb, 0x3c, src);
	xb->enabled[index] = true;
	return 0;
}

static int j704_dpxbar_set(struct mux_control *mux, int state)
{
	struct j704_dpxbar *xb = mux_chip_priv(mux->chip);
	unsigned int index = mux_control_get_index(mux);
	unsigned long flags;
	int ret = 0;

	if (state != MUX_IDLE_DISCONNECT &&
	    (state < 0 || state >= T8142_N_UFP ||
	     (index == T8142_MUX_DPPHY && (state & 1))))
		return -EINVAL;

	spin_lock_irqsave(&xb->lock, flags);
	if (xb->faulted) {
		ret = -EIO;
		goto out;
	}
	if (state >= 0) {
		for (unsigned int i = 0; i < T8142_MUX_MAX; i++) {
			if (i != index && xb->selected[i] == state) {
				ret = -EBUSY;
				goto out;
			}
		}
	}
	if (xb->selected[index] == state)
		goto out;
	if (xb->selected[index] >= 0) {
		ret = t8142_link_down(xb, index);
		if (ret)
			goto out;
	}

	/* No implicit clock choice or activation when selecting a new source. */
	xb->pclk[index] = 0;
	xb->selected[index] = state;
	if (state >= 0)
		xb_mask32(xb, index == 4 ? 0x40 : 0x44,
			  (0xfU << select_shift[index]) | (0xfU << select_shift2[index]),
			  ((u32)state << select_shift[index]) |
				  ((u32)state << select_shift2[index]));
out:
	spin_unlock_irqrestore(&xb->lock, flags);
	if (!ret)
		dev_info(xb->dev, "output %u: source %d\n", index, state);
	return ret;
}

static const struct mux_control_ops j704_dpxbar_ops = {
	.set = j704_dpxbar_set,
};

/* Caller holds the mux selection throughout configuration and activation. */
int j704_dpxbar_link_configure(struct mux_control *mux, unsigned int pclk, bool uhbr)
{
	struct j704_dpxbar *xb;
	unsigned int index;
	unsigned long flags;
	int ret = 0;

	if (!mux || mux->chip->ops != &j704_dpxbar_ops)
		return -EOPNOTSUPP;
	if (pclk < 1 || pclk > 4)
		return -EINVAL;
	xb = mux_chip_priv(mux->chip);
	index = mux_control_get_index(mux);
	spin_lock_irqsave(&xb->lock, flags);
	if (xb->faulted)
		ret = -EIO;
	else if (xb->selected[index] < 0)
		ret = -ENODEV;
	else if (xb->enabled[index])
		ret = -EBUSY;
	else {
		xb->pclk[index] = pclk;
		xb->uhbr[index] = uhbr;
	}
	spin_unlock_irqrestore(&xb->lock, flags);
	return ret;
}
EXPORT_SYMBOL_GPL(j704_dpxbar_link_configure);

static int j704_dpxbar_link(struct mux_control *mux, bool up)
{
	struct j704_dpxbar *xb;
	unsigned int index;
	unsigned long flags;
	int ret;

	if (!mux || mux->chip->ops != &j704_dpxbar_ops)
		return -EOPNOTSUPP;
	xb = mux_chip_priv(mux->chip);
	index = mux_control_get_index(mux);
	spin_lock_irqsave(&xb->lock, flags);
	if (xb->faulted)
		ret = -EIO;
	else if (xb->selected[index] < 0)
		ret = -ENODEV;
	else
		ret = up ? t8142_link_up(xb, index) : t8142_link_down(xb, index);
	spin_unlock_irqrestore(&xb->lock, flags);
	dev_info(xb->dev, "output %u: link %s pclk %u%s: %d\n", index, up ? "up" : "down",
		 xb->pclk[index], xb->uhbr[index] ? " uhbr" : "", ret);
	return ret;
}

int j704_dpxbar_link_up(struct mux_control *mux)
{
	return j704_dpxbar_link(mux, true);
}
EXPORT_SYMBOL_GPL(j704_dpxbar_link_up);

int j704_dpxbar_link_down(struct mux_control *mux)
{
	return j704_dpxbar_link(mux, false);
}
EXPORT_SYMBOL_GPL(j704_dpxbar_link_down);

static int j704_dpxbar_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct mux_chip *mux_chip;
	struct j704_dpxbar *xb;
	struct resource *res;

	if (!of_machine_is_compatible("apple,j704"))
		return -ENODEV;
	res = platform_get_resource(pdev, IORESOURCE_MEM, 0);
	if (!res || resource_size(res) < 0x830)
		return -EINVAL;

	mux_chip = devm_mux_chip_alloc(dev, T8142_MUX_MAX, sizeof(*xb));
	if (IS_ERR(mux_chip))
		return PTR_ERR(mux_chip);
	xb = mux_chip_priv(mux_chip);
	xb->dev = dev;
	spin_lock_init(&xb->lock);
	mux_chip->ops = &j704_dpxbar_ops;
	xb->regs = devm_ioremap_resource(dev, res);
	if (IS_ERR(xb->regs))
		return PTR_ERR(xb->regs);

	for (unsigned int i = 0; i < T8142_MUX_MAX; i++) {
		mux_chip->mux[i].states = T8142_N_UFP;
		mux_chip->mux[i].idle_state = MUX_IDLE_DISCONNECT;
		xb->selected[i] = -1;
	}
	dev_info(dev, "T8142 crossbar: select 0x%08x/0x%08x\n", readl(xb->regs + 0x40),
		 readl(xb->regs + 0x44));
	return devm_mux_chip_register(dev, mux_chip);
}

static const struct of_device_id j704_dpxbar_ids[] = {
	{ .compatible = "apple,t8142-display-crossbar" },
	{}
};
MODULE_DEVICE_TABLE(of, j704_dpxbar_ids);

static struct platform_driver j704_dpxbar_driver = {
	.driver = {
		.name = "j704-display-crossbar",
		.of_match_table = j704_dpxbar_ids,
	},
	.probe = j704_dpxbar_probe,
};
static int __init j704_dpxbar_init(void)
{
	if (!of_machine_is_compatible("apple,j704"))
		return -ENODEV;
	return platform_driver_register(&j704_dpxbar_driver);
}
module_init(j704_dpxbar_init);

static void __exit j704_dpxbar_exit(void)
{
	platform_driver_unregister(&j704_dpxbar_driver);
}
module_exit(j704_dpxbar_exit);

MODULE_DESCRIPTION("J704 (T8142) display crossbar");
MODULE_LICENSE("GPL v2");
