// SPDX-License-Identifier: GPL-2.0-only OR MIT
#include <linux/module.h>

/* Firmware-visible session owners outlive unbind until a verified stop exists. */
void asahi_hal300_pin_module(void);
void asahi_hal300_pin_module(void)
{
	__module_get(THIS_MODULE);
}
