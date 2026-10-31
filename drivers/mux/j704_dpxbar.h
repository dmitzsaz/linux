/* SPDX-License-Identifier: GPL-2.0-only */
#ifndef J704_DPXBAR_H
#define J704_DPXBAR_H

#include <linux/types.h>

struct mux_control;

/* pclk is the crossbar pixel clock 1..4 the PHY supplies; uhbr its coding. */
int j704_dpxbar_link_configure(struct mux_control *mux, unsigned int pclk, bool uhbr);
int j704_dpxbar_link_up(struct mux_control *mux);
int j704_dpxbar_link_down(struct mux_control *mux);

#endif
