/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
/* J704 module copy of Aurora's pmp-report API (renamed: the original is built in). */
#ifndef _J704_PMP_REPORT_H_
#define _J704_PMP_REPORT_H_

#include <linux/types.h>

struct device_node;
struct device;

int j704_pmp_report_wait_supplier_ready(struct device *supplier, unsigned int timeout_ms);
int j704_pmp_report_wait_ready(struct device_node *entry, unsigned long timeout);
int j704_pmp_report_fast_die_effort(const struct device_node *np,
				    unsigned int lane, u8 *effort);

#endif
