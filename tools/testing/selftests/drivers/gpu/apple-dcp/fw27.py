# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Firmware-27 converter ownership and local RemotePort units."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from common import block, run_unit


def ownership(root):
    apple = root / 'drivers/gpu/drm/apple'
    fabric = (apple / 'dcp-fabric.c').read_text()
    dcp = (apple / 'dcp.c').read_text()
    prefix = r'''
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include <errno.h>
#include <stdio.h>
typedef uint32_t u32;
typedef uint64_t u64;
#define READ_ONCE(x) (x)
#define WRITE_ONCE(x,y) ((x)=(y))
#define KUNIT_STATIC_STUB_REDIRECT(...) ((void)0)
#define lockdep_assert_held(p) assert(*(p))
#define guard(k) (void)
#define container_of(p,t,m) ((t *)((char *)(p)-offsetof(t,m)))
#define spin_lock_irqsave(p,f) ((void)(p),(f)=0)
#define spin_unlock_irqrestore(p,f) ((void)(p),(void)(f))
#define DCP_FIRMWARE_V_27_0 27
#define DCP_FIRMWARE_V_14_7 14
#define DRM_MODE_CONNECTOR_USB 1
#define DCP_HDMI_HOLD_MS 10000
#define to_delayed_work(p) container_of(p,struct delayed_work,work)
struct work_struct { bool queued; };
struct delayed_work { struct work_struct work; };
enum dcp_fabric_presence_state { DCP_FABRIC_ABSENT, DCP_FABRIC_SETTLING, DCP_FABRIC_PRESENT };
struct dcp_fabric_presence { enum dcp_fabric_presence_state state; u64 generation; unsigned long deadline; };
struct apple_connector { bool connected; };
struct dptx_port { bool enabled,connected; void *service; };
struct apple_dcp {
 int fw_compat,fixed_connector_type,hdmi_presence_lock,hpd_mutex,hdmi_hpd_irq;
 int *hdmi_hpd;
 void *active_typec_route,*fixed_phy;
 bool fixed_sink_known,fixed_sink_present,fixed_link_held,hdmi_yielded,external,quiescing,typec_work_enabled;
 struct apple_connector *fixed_connector;
 struct dcp_fabric_presence hdmi_presence;
 enum dcp_fabric_presence_state hdmi_presence_prev;
 struct delayed_work hdmi_recheck_wq;
 struct dptx_port dptxport[2];
};
struct apple_dcp_typec_route { struct apple_dcp *dcp; };
static int system_freezable_wq,jiffies,invalidations,capacity,hpd_error,release_error,hpd_calls,release_calls;
static bool dcp_is_typec_output(struct apple_dcp *d) { return !!d->active_typec_route; }
static int gpiod_get_value_cansleep(int *p) { return *p; }
static bool dcp_fabric_fixed_busy(bool a,bool b,bool c,bool d) { return a||b||c||d; }
static bool dcp_hdmi_settle_enabled(struct apple_dcp *d) { (void)d;return false; }
static bool dcp_fabric_presence_recheck(void *p,u64 g,bool h,int j,int t) { (void)p;(void)g;(void)h;(void)j;(void)t;return false; }
static int msecs_to_jiffies(int ms) { return ms; }
static void dcp_hdmi_sample(struct apple_dcp *d,u64 g,int l) { (void)d;(void)g;(void)l; }
static void dcp_hdmi_schedule(struct apple_dcp *d) { (void)d; }
static void dcp_hdmi_update_locked(struct apple_dcp *d) { (void)d; }
static void synchronize_irq(int irq) { (void)irq; }
static int dcp_typec_fabric_lock;
static void dcp_typec_pipeline_freed(void) { capacity++; }
static void dcp_modes_begin_attachment(struct apple_dcp *d) { (void)d;invalidations++; }
static void mod_delayed_work(int q,struct delayed_work *w,int delay) { (void)q;(void)delay;w->work.queued=true; }
static bool dptx_service_available(struct dptx_port *p) { return p->enabled; }
static int dptxport_set_hpd(void *s,bool h) { assert(s&&!h);hpd_calls++;return hpd_error; }
static int dptxport_release_display(void *s) { assert(s);assert(hpd_calls>release_calls);release_calls++;return release_error; }
'''
    production = block((apple / 'dcp-fabric-core.c').read_text(), 'u64 dcp_fabric_presence_edge(') + '\n'
    production += '\n'.join(block(fabric, x) for x in (
        'bool dcp_hdmi_port_attached(',
        'static bool dcp_typec_route_fixed_output_busy(',
        'static enum dcp_fabric_presence_state dcp_hdmi_presence(',
        'static u64 dcp_hdmi_edge(',
        'static void dcp_hdmi_recheck_work(',
        'void dcp_fixed_sink_hotplug('))
    production += '\n' + block(dcp, 'static int dcp_dptx_withdraw_locked(')
    return prefix + production + r'''
static void recheck(struct apple_dcp *d) {
 assert(d->hdmi_recheck_wq.work.queued);
 d->hdmi_recheck_wq.work.queued=false;
 dcp_hdmi_recheck_work(&d->hdmi_recheck_wq.work);
}
int main(void) {
 int gpio=1;
 struct apple_dcp d={.fw_compat=27,.fixed_phy=&gpio,.hdmi_hpd=&gpio,.typec_work_enabled=true,.hpd_mutex=1};
 struct apple_dcp_typec_route route={.dcp=&d};
 d.dptxport[0]=(struct dptx_port){.enabled=true,.connected=true,.service=&d};
 /* Physical FW27 edges have a token even when fabric settling is disabled. */
 assert(dcp_hdmi_edge(&d)==1&&d.hdmi_presence.generation==1);
 assert(dcp_hdmi_edge(&d)==2&&d.hdmi_presence.generation==2);
 d.fw_compat=26;assert(dcp_hdmi_edge(&d)==0&&d.hdmi_presence.generation==2);d.fw_compat=27;
 /* A high converter HPD and no initial downstream report reserve nothing. */
 assert(!dcp_hdmi_port_attached(&d));
 assert(!dcp_typec_route_fixed_output_busy(&route));
 assert(dcp_hdmi_presence(&d)==DCP_FABRIC_ABSENT);
 /* The firmware reports a real monitor; only that sink holds the pipe. */
 dcp_fixed_sink_hotplug(&d,true);
 assert(dcp_hdmi_port_attached(&d)&&dcp_typec_route_fixed_output_busy(&route));
 recheck(&d);assert(capacity==0);
 /* Timeout: a failed HPD withdrawal must neither release nor lend it. */
 hpd_error=-ETIMEDOUT;
 assert(dcp_dptx_withdraw_locked(&d,0)==-ETIMEDOUT);
 assert(d.fixed_link_held&&d.dptxport[0].connected&&release_calls==0);
 dcp_fixed_sink_hotplug(&d,false);recheck(&d);
 assert(dcp_typec_route_fixed_output_busy(&route)&&capacity==0);
 /* Nor may a failed release make capacity available after an unplug. */
 hpd_error=0;release_error=-EIO;
 assert(dcp_dptx_withdraw_locked(&d,0)==-EIO);
 assert(d.fixed_link_held&&d.dptxport[0].connected);
 /* Both replies acknowledged: wake the waiting port without a cable retry. */
 release_error=0;
 assert(!dcp_dptx_withdraw_locked(&d,0));
 assert(!d.fixed_link_held&&!d.dptxport[0].connected);
 assert(!dcp_typec_route_fixed_output_busy(&route));
 recheck(&d);assert(capacity==1&&gpio==1);
 /* A late HDMI withdrawal worker cannot disturb a Type-C borrower. */
 d.active_typec_route=&route;
 d.hdmi_recheck_wq.work.queued=true;recheck(&d);assert(capacity==1);
 dcp_fixed_sink_hotplug(&d,true);assert(!d.fixed_sink_present);
 assert(!dcp_hdmi_port_attached(&d));
 puts("PASS firmware-27 converter HPD, sink ownership, failed/acknowledged withdrawal, capacity wake and borrower isolation");
}
'''


def ports(root):
    apple = root / 'drivers/gpu/drm/apple'
    return r'''
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>
typedef int64_t s64;
typedef uint32_t u32;
#define READ_ONCE(x) (x)
#define DCP_FIRMWARE_V_27_0 27
struct dptx_port { bool bound; s64 unit; };
struct apple_dcp { int fw_compat; u32 index; struct { u32 num_dptx_ports; } hw; struct dptx_port dptxport[2]; };
struct apple_dcp_afkep { struct apple_dcp *dcp; bool compact,wire_v2; };
struct apple_epic_service { struct apple_dcp_afkep *ep; bool defer_start; struct dptx_port *cookie; };
static bool dcp_fw_is_14_7(struct apple_dcp *d) { return d->fw_compat==14; }
static void trace_dptxport_init(struct apple_dcp *d,s64 unit) { (void)d;(void)unit; }
static int dptx_service_bind(struct apple_epic_service *s,struct dptx_port *p,s64 unit) {
 if(p->bound)return -EBUSY;
 p->bound=true;p->unit=unit;s->cookie=p;return 0;
}
''' + block((apple / 'dptx-service.h').read_text(), 'static inline int dptx_service_port(') + '\n' + block((apple / 'dptxep.c').read_text(), 'static int dptxport_prepare(') + r'''
int main(void) {
 const char *name="dcpdptx-port-epic",*class="AppleDCPDPTXRemotePort";
 struct apple_dcp d={.fw_compat=27,.index=0,.hw.num_dptx_ports=2};
 struct apple_dcp_afkep ep={.dcp=&d,.wire_v2=true};
 struct apple_epic_service a={.ep=&ep},b={.ep=&ep},bad={.ep=&ep};
 assert(!dptxport_prepare(&a,name,class,0)&&a.cookie==&d.dptxport[0]);
 assert(!dptxport_prepare(&b,name,class,1)&&b.cookie==&d.dptxport[1]);
 assert(dptxport_prepare(&bad,name,class,2)==-EINVAL);
 assert(dptxport_prepare(&bad,"wrong",class,0)==-EINVAL);
 assert(dptxport_prepare(&bad,name,"wrong",0)==-EINVAL);
 assert(dptxport_prepare(&bad,name,class,0)==-EBUSY);
 /* H17P retains indexed units, independent of AFKv2 transport on 27. */
 d=(struct apple_dcp){.fw_compat=26,.index=3,.hw.num_dptx_ports=1};
 assert(dptxport_prepare(&bad,name,class,0)==-EINVAL);
 assert(!dptxport_prepare(&bad,name,class,3)&&bad.cookie==&d.dptxport[0]);
 puts("PASS firmware-27 local RemotePort units 0/1, identity/duplicate guards and unchanged H17P index admission");
}
'''


HANDBACK_PREFIX = r'''// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* Host-only boundaries; the fabric callbacks are extracted without edits. */
#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

#define READ_ONCE(x) (x)
#define WRITE_ONCE(x, v) ((x) = (v))
#define ERR_PTR(e) ((void *)(intptr_t)(e))
#define PTR_ERR(p) ((int)(intptr_t)(p))
#define IS_ERR(p) ((uintptr_t)(p) >= (uintptr_t)-4095)
#define DCP_FIRMWARE_V_27_0 27
#define DCP_FIRMWARE_V_26_6_DESKTOP 26
#define DCP_FIRMWARE_V_14_7 14
#define DCP_FIRMWARE_V_12_3 12
#define DCP_FIRMWARE_V_13_5 13
#define DCP_FIRMWARE_H17P 25
#define DRM_MODE_CONNECTOR_USB 1
#define DRM_MODE_CONNECTOR_HDMIA 2
#define DCP_IOMFB_METHODS_H17P_EXTERNAL 1
#define USB_TYPEC_DP_SID 0xff01
#define TYPEC_STATE_SAFE 0
#define TYPEC_DP_STATE_A 10
#define TYPEC_DP_STATE_F 15
#define DP_STATUS_HPD_STATE 1
#define DP_STATUS_IRQ_HPD 2
#define MUX_IDLE_DISCONNECT -1
#define BIT(n) (1U << (n))
#define container_of(p, t, m) ((t *)((char *)(p) - offsetof(t, m)))
#define spin_lock_irqsave(p, f) ((void)(p), (f) = 0)
#define spin_unlock_irqrestore(p, f) ((void)(p), (void)(f))

struct host_mutex {
	bool held;
};

struct host_guard {
	struct host_mutex *lock;
};

static struct host_guard host_lock_mutex(struct host_mutex *lock)
{
	assert(!lock->held);
	lock->held = true;
	return (struct host_guard){ .lock = lock };
}

static void host_unlock(struct host_guard *guard)
{
	if (guard->lock) {
		assert(guard->lock->held);
		guard->lock->held = false;
		guard->lock = NULL;
	}
}

#define JOIN1(a, b) a ## b
#define JOIN(a, b) JOIN1(a, b)
#define guard(k) \
	struct host_guard __attribute__((cleanup(host_unlock))) \
	JOIN(held_, __COUNTER__) = JOIN(host_lock_, k)
#define scoped_guard(k, p) \
	for (struct host_guard __attribute__((cleanup(host_unlock))) \
	     held = JOIN(host_lock_, k)(p); held.lock; host_unlock(&held))
#define lockdep_assert_held(p) assert((p)->held)

struct list_head {
	void *entry;
	struct list_head *next;
};

static void host_list_append(struct list_head *head, struct list_head *link, void *entry)
{
	struct list_head *tail = head;

	while (tail->next)
		tail = tail->next;
	tail->next = link;
	link->entry = entry;
	if (!head->entry)
		head->entry = entry;
}

static void *host_list_next(const struct list_head *link)
{
	return link->next ? link->next->entry : NULL;
}

#define list_for_each_entry(p, h, m) \
	for (__typeof__(p) item = (h)->entry; \
	     ((p) = item); item = host_list_next(&item->m))
#define list_empty(h) (!(h)->entry)

struct phy {
	int id;
};

struct mux_control {
	bool held;
	struct host_mux_chip *chip;
};

struct host_mux_chip {
	struct mux_control *mux;
};

struct platform_device {
	int unused;
};

#define to_platform_device(p) (p)

struct encoder {
	u32 possible_crtcs;
};

struct apple_connector {
	bool connected;
	struct platform_device *dcp;
	struct encoder *port_encoder;
	u32 candidate_crtcs;
	int bl_sync_wq;
};

struct typec_altmode {
	u32 svid;
};

struct typec_displayport_data {
	u32 status;
	u32 conf;
};

struct typec_mux_state {
	struct typec_altmode *alt;
	unsigned long mode;
	void *data;
};

struct typec_mux_dev {
	void *data;
};

#define typec_mux_get_drvdata(m) ((m)->data)

struct apple_dcp_typec_route;

struct drm_crtc {
	u32 index;
};

struct apple_crtc {
	struct drm_crtc base;
};

struct apple_dcp {
	struct platform_device *dev;
	struct apple_crtc *crtc;
	struct dcp_fabric_pipeline fabric;
	struct dcp_fabric_presence hdmi_presence;
	int hdmi_presence_lock;
	unsigned int nr_typec_routes;
	bool fixed_sink_present, fixed_link_held, external;
	struct apple_dcp *peer_owner;
	int fw_compat;
	struct host_mutex external_lock, tb_lock, hpd_mutex, modes_lock;
	int hdmi_start_wq;
	int external_error;
	bool external_power_attempted, external_stopped, external_cable;
	bool external_physically_absent;
	u64 external_generation, typec_generation, modes_generation, tb_generation;
	bool modes_admitted, modes_provisional;
	struct { int iomfb_method_profile; } hw;
	struct { bool connected; } dptxport[2];
	int announce;
	bool active, hdmi_yielded, peer_starved, typec_narrow;
	bool ext_backlight, typec_cable_connected, ext_dead, external_native;
	bool fixed_route_selected, dptx_tunnel, tb_clock_ok, tb_clock_refused;
	struct apple_dcp_typec_route *active_typec_route, *typec_routes;
	struct apple_connector *connector, *fixed_connector, *typec_connector;
	struct phy *phy, *fixed_phy;
	u32 dptx_phy, fixed_dptx_phy, fixed_mux_index, dptx_dfp_port;
	struct mux_control *fixed_xbar, *xbar, *direct_xbar_up;
	void *tb_dpin_set_active, *tb_dpin_ctx;
	int *hdmi_hpd;
	u32 connector_type, fixed_connector_type;
};
'''


HANDBACK_MOCKS = r'''// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* Firmware, mux and publication leaves, with lock and ownership assertions. */
static struct host_mutex dcp_typec_fabric_lock;
static struct list_head dcp_typec_ports;
static bool typec_follow_crtc = true;
static unsigned int dcp_hybrid_fails;
static struct apple_dcp *display;
static int select_error, deselect_error, disconnect_error, connect_error;
static int typec_select_error, activate_disconnect_error, yield_error, yields;
static int selects, deselects, disconnects, connects, errors, retired;
static int typec_selects, fixed_deselects, typec_connects;
static int metadata_ends, metadata_starts, external_publications;
static bool selection_under_external_lock;

static void handback_error(const char *message, ...)
{
	assert(message);
	errors++;
}

#define dev_err(d, ...) ((void)(d), handback_error(__VA_ARGS__))
#define dev_warn(d, ...) ((void)(d), handback_error(__VA_ARGS__))
#define dev_info(...) ((void)0)
#define dev_info_once(...) ((void)0)

static int gpiod_get_value_cansleep(int *gpio)
{
	return *gpio;
}

static int mux_control_select(struct mux_control *mux, unsigned int index)
{
	assert(dcp_typec_fabric_lock.held);
	assert(mux == display->fixed_xbar && index == display->fixed_mux_index);
	assert(!mux->held && !display->active_typec_route);
	assert(display->phy == display->fixed_phy);
	assert(display->dptx_phy == display->fixed_dptx_phy);
	selects++;
	selection_under_external_lock = display->external_lock.held;
	if (select_error)
		return select_error;
	mux->held = true;
	return 0;
}

static int mux_control_deselect(struct mux_control *mux)
{
	assert(dcp_typec_fabric_lock.held);
	assert(mux->held);
	mux->held = false;
	if (mux == display->fixed_xbar) {
		fixed_deselects++;
		return 0;
	}
	assert(display->external_lock.held && display->tb_lock.held);
	deselects++;
	return deselect_error;
}

static int mux_control_try_select(struct mux_control *mux, unsigned int index)
{
	assert(dcp_typec_fabric_lock.held && !display->active_typec_route);
	assert(!mux->held && !display->fixed_route_selected && index == 0);
	typec_selects++;
	if (typec_select_error)
		return typec_select_error;
	mux->held = true;
	return 0;
}

static bool dcp_is_external_h17p(struct apple_dcp *dcp)
{
	return dcp->fw_compat == DCP_FIRMWARE_V_27_0;
}

static void dcp_typec_route_phy_off(struct apple_dcp_typec_route *route)
{
	assert(route->dcp->external_lock.held && route->dcp->tb_lock.held);
	assert(!route->phy_powered);
}

static void dcp_peer_release_26_6(struct apple_dcp *dcp)
{
	assert(dcp->fw_compat == DCP_FIRMWARE_V_26_6_DESKTOP);
}

static void dcp_firmware_metadata_connection(struct apple_dcp *dcp,
					     u64 generation, bool live)
{
	assert(generation == dcp->typec_generation);
	if (live)
		metadata_starts++;
	else
		metadata_ends++;
}

static void dcp_modes_invalidate_locked(struct apple_dcp *dcp)
{
	assert(dcp->modes_lock.held);
	dcp->modes_generation++;
	dcp->modes_admitted = false;
	retired++;
}

static void dcp_modes_begin_attachment(struct apple_dcp *dcp)
{
	guard(mutex)(&dcp->modes_lock);
	dcp_modes_invalidate_locked(dcp);
}

static void apple_connector_set_pipeline(struct apple_connector *connector,
					 struct platform_device *device)
{
	connector->dcp = device;
}

static void schedule_work(int *work)
{
	(void)work;
}

static void dpavservep_detach(struct apple_dcp *dcp)
{
	assert(dcp->external_lock.held);
}

static int dcp_dpxbar_preselect(struct mux_control *mux, int state)
{
	(void)mux;
	(void)state;
	assert(!"no tunnel in this fixture");
	return -EINVAL;
}

static bool dcp_typec_route_no_dp_alt(const struct apple_dcp_typec_route *route)
{
	(void)route;
	return false;
}

static int dcp_route_clock_rate(struct apple_dcp_typec_route *route,
				struct phy *phy, unsigned int dpin, u8 rate)
{
	(void)route;
	(void)phy;
	(void)dpin;
	(void)rate;
	assert(!"no tunnel clock in this fixture");
	return -EINVAL;
}

static int dcp_dpxbar_tunnel_select_source(struct mux_control *mux, int state)
{
	return dcp_dpxbar_preselect(mux, state);
}

static int dcp_dptx_disconnect_oob(struct platform_device *device, u32 port)
{
	assert(device == display->dev && !port);
	guard(mutex)(&display->external_lock);
	disconnects++;
	if (disconnect_error || activate_disconnect_error)
		return disconnect_error ?: activate_disconnect_error;
	display->external_stopped = true;
	display->external_cable = false;
	display->typec_cable_connected = false;
	display->dptxport[0].connected = false;
	return 0;
}

static void assert_fixed(struct apple_dcp *dcp)
{
	assert(dcp_typec_fabric_lock.held && !dcp->tb_lock.held);
	assert(!dcp->active_typec_route && !dcp->typec_connector);
	assert(!dcp->hdmi_yielded);
	assert(dcp->connector == dcp->fixed_connector);
	assert(dcp->connector->dcp == dcp->dev);
	assert(dcp->connector_type == dcp->fixed_connector_type);
	assert(dcp->phy == dcp->fixed_phy);
	assert(dcp->dptx_phy == dcp->fixed_dptx_phy);
	assert(!dcp->fixed_xbar ||
	       (dcp->fixed_route_selected && dcp->fixed_xbar->held));
}

static int dcp_external_link_locked(struct apple_dcp *dcp)
{
	assert(dcp->external_lock.held);
	assert_fixed(dcp);
	connects++;
	if (!connect_error)
		dcp->dptxport[0].connected = true;
	return connect_error;
}

static int dcp_desktop_dptx_connect(struct apple_dcp *dcp, u32 port)
{
	assert(!port && dcp->xbar == dcp->fixed_xbar);
	assert_fixed(dcp);
	connects++;
	return connect_error;
}

struct dcp_fabric_session {
	u64 cookie;
};

static bool dcp_typec_sessionless(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(!"no sessionless connection in this fixture");
	return false;
}

static struct dcp_fabric_session dcp_session_locked(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(!"no Type-C connect in this fixture");
	return (struct dcp_fabric_session){};
}

static int dcp_dptx_connect_raw(struct apple_dcp *dcp, u32 port, void *tunnel,
				struct dcp_fabric_session *session, bool cable, bool raw)
{
	(void)dcp;
	(void)port;
	(void)tunnel;
	(void)session;
	(void)cable;
	(void)raw;
	assert(!"no raw connection in this fixture");
	return -EINVAL;
}

static int dcp_dptx_connect_tile(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(!"no tiled connection in this fixture");
	return -EINVAL;
}

static void dcp_external_publish_generation(struct apple_dcp *dcp)
{
	assert(dcp->external_lock.held);
	external_publications++;
}

static void dcp_announce_end(int *announce)
{
	(void)announce;
}



static bool dcp_typec_keep_order(void)
{
	return false;
}

static void dcp_typec_rebalance_locked(void *port, unsigned int mask)
{
	(void)port;
	(void)mask;
	assert(!"no rebalance on this single-pipeline fixture");
}



static struct apple_dcp_typec_route *dcp_typec_free_route(struct apple_dcp_typec_port *port);







static int dcp_dptx_connect_oob(struct platform_device *device, u32 port)
{
	struct apple_dcp_typec_route *route = display->active_typec_route;

	assert(device == display->dev && !port && route && route->selected);
	assert(route->xbar->held && !display->fixed_route_selected);
	assert(display->phy == route->phy && display->dptx_phy == route->dptx_phy);
	assert(display->typec_connector == route->port->connector);
	assert(display->connector == display->typec_connector);
	assert(display->typec_connector->dcp == device && display->typec_cable_connected);
	display->dptxport[0].connected = true;
	typec_connects++;
	return 0;
}

static void dcp_retrain_oob(struct apple_connector *connector)
{
	(void)connector;
	assert(!"no retrain in this fixture");
}

static void dcp_external_sink_irq(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(!"no sink IRQ in this fixture");
}

static bool dcp_typec_sink_relink(struct apple_dcp *dcp)
{
	(void)dcp;
	return false;
}

static int dcp_dptx_recover_irq(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(!"no IRQ recovery in this fixture");
	return -EINVAL;
}

static int dcp_typec_route_apply(struct typec_mux_dev *mux,
				 struct typec_mux_state *state);

static int dcp_dptx_disconnect(struct apple_dcp *dcp, u32 port)
{
	assert(dcp_typec_fabric_lock.held && dcp == display && !port);
	guard(mutex)(&dcp->external_lock);
	return activate_disconnect_error;
}

static void apple_connector_edid_set_live(struct apple_connector *connector, bool live)
{
	assert(connector && !live);
}

static unsigned int drm_crtc_index(struct drm_crtc *crtc)
{
	return crtc->index;
}

static bool dcp_typec_ordered_dual_stream(void)
{
	return false;
}

static bool dcp_typec_hybrid_first(void)
{
	return false;
}

static bool dcp_hdmi_settle_enabled(struct apple_dcp *dcp)
{
	(void)dcp;
	return false;
}

static bool iomfb_v14_7_external_failed(struct apple_dcp *dcp)
{
	(void)dcp;
	return false;
}

static bool dcpext_scanout_terminal(struct apple_dcp *dcp)
{
	(void)dcp;
	return false;
}

static bool dcp_tb_services_ready(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(!"no tunnel service query in this fixture");
	return false;
}

static u8 dcp_route_tunnel_clock_blocked(struct apple_dcp_typec_route *route)
{
	(void)route;
	return 0;
}

static int of_property_read_string(void *node, const char *name, const char **value)
{
	(void)node;
	(void)name;
	*value = NULL;
	return -ENOENT;
}

static void cancel_delayed_work(int *work)
{
	(void)work;
	assert(yield_error);
}

static int dcp_hdmi_yield_link(struct apple_dcp *dcp)
{
	(void)dcp;
	assert(yield_error);
	yields++;
	return yield_error;
}
'''


HANDBACK_ASSERTIONS = r'''// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* All starting states have a real Type-C lease and no fixed-output lease. */
int main(int argc, char **argv)
{
	struct platform_device device = {};
	struct apple_crtc crtc = {};
	struct phy typec_phy = { .id = 1 }, fixed_phy = { .id = 2 };
	struct mux_control typec_mux = { .held = true }, fixed_mux = {};
	struct encoder encoder = { .possible_crtcs = 1 };
	struct apple_connector fixed_connector = {};
	struct apple_connector typec_connector = {
		.connected = true,
		.dcp = &device,
		.port_encoder = &encoder,
		.candidate_crtcs = 3,
	};
	int gpio = 1;
	struct apple_dcp dcp = {
		.dev = &device,
		.crtc = &crtc,
		.nr_typec_routes = 1,
		.fw_compat = DCP_FIRMWARE_V_27_0,
		.hw.iomfb_method_profile = DCP_IOMFB_METHODS_H17P_EXTERNAL,
		.active = true,
		.hdmi_yielded = true,
		.external_cable = true,
		.external_stopped = true,
		.external_generation = 5,
		.typec_generation = 9,
		.modes_generation = 3,
		.modes_admitted = true,
		.typec_cable_connected = true,
		.typec_connector = &typec_connector,
		.connector = &typec_connector,
		.fixed_connector = &fixed_connector,
		.phy = &typec_phy,
		.fixed_phy = &fixed_phy,
		.dptx_phy = 1,
		.fixed_dptx_phy = 2,
		.fixed_mux_index = 4,
		.fixed_xbar = &fixed_mux,
		.hdmi_hpd = &gpio,
		.connector_type = DRM_MODE_CONNECTOR_USB,
		.fixed_connector_type = DRM_MODE_CONNECTOR_HDMIA,
	};
	struct apple_dcp_typec_port port = {
		.connector = &typec_connector,
		.hpd = true,
		.dp_wanted = true,
		.dp_hpd = true,
	};
	struct apple_dcp_typec_route route = {
		.dcp = &dcp,
		.port = &port,
		.phy = &typec_phy,
		.dptx_phy = 1,
		.xbar = &typec_mux,
		.active_xbar = &typec_mux,
		.selected = true,
		.xbar_up = true,
	};
	struct typec_mux_dev mux = { .data = &route };
	struct typec_altmode alt = { .svid = USB_TYPEC_DP_SID };
	struct typec_displayport_data dp = {};
	struct typec_mux_state state = { .mode = TYPEC_STATE_SAFE };
	const char *name;
	bool yielded;
	int ret;

	assert(argc == 2);
	name = argv[1];
	display = &dcp;
	dcp.typec_routes = &route;
	dcp.active_typec_route = &route;
	dcp.dptxport[0].connected = true;
	port.owner = &route;
	host_list_append(&port.routes, &route.port_link, &route);
	host_list_append(&dcp_typec_ports, &port.link, &port);
	if (!strncmp(name, "idle-fixed-", 11)) {
		/* An old HDMI CRTC stopped with its logical attachment retained. */
		gpio = 0;
		dcp.active_typec_route = NULL;
		dcp.typec_connector = NULL;
		dcp.connector = &fixed_connector;
		dcp.hdmi_yielded = false;
		dcp.fixed_sink_present = false;
		dcp.phy = &fixed_phy;
		dcp.dptx_phy = dcp.fixed_dptx_phy;
		dcp.fixed_route_selected = fixed_mux.held = true;
		dcp.external_power_attempted = true;
		dcp.external_cable = dcp.external_stopped = true;
		dcp.dptxport[0].connected = false;
		route.selected = typec_mux.held = false;
		port.owner = NULL;
		if (!strcmp(name, "idle-fixed-stop-failure"))
			activate_disconnect_error = -ETIMEDOUT;
		if (!strcmp(name, "idle-fixed-mux-failure"))
			typec_select_error = -EBUSY;
		scoped_guard(mutex, &dcp_typec_fabric_lock)
			ret = dcp_typec_route_activate(&route, &typec_mux);
		if (activate_disconnect_error) {
			assert(ret == -ETIMEDOUT && !dcp.active_typec_route);
			assert(fixed_mux.held && !typec_mux.held && !route.selected);
		} else {
			/* OOB attach must see a new cable, not the old CRTC's DPMS wake. */
			assert(!dcp.external_cable && !dcp.typec_cable_connected);
			assert(ret == (typec_select_error ?: 0));
			assert(route.selected == !typec_select_error);
			assert(typec_mux.held == !typec_select_error);
			assert(fixed_mux.held == !!typec_select_error);
		}
		assert(disconnects == 1 && !typec_connects);
		printf("PASS idle fixed attachment retired before route selection: %s\n", name);
		return 0;
	}
	if (!strcmp(name, "yield-failure")) {
		/* HDMI is logically active even though its GPIO has gone low. */
		gpio = 0;
		dcp.active_typec_route = NULL;
		dcp.typec_connector = NULL;
		dcp.connector = &fixed_connector;
		dcp.hdmi_yielded = false;
		dcp.fixed_sink_present = true;
		dcp.fixed_route_selected = fixed_mux.held = true;
		route.selected = typec_mux.held = false;
		port.owner = NULL;
		port.hpd = port.dp_hpd = false;
		state.mode = TYPEC_DP_STATE_A;
		state.alt = &alt;
		state.data = &dp;
		dp.status = DP_STATUS_HPD_STATE;
		yield_error = -EIO;
		ret = dcp_typec_route_finish(&mux, &state);
		assert(ret == -EIO && yields == 1 && !typec_connects);
		assert(!port.owner && !dcp.active_typec_route && !route.selected);
		assert(fixed_mux.held && !typec_mux.held && !selects && !disconnects);
		puts("PASS failed HDMI yield propagates before any Type-C activation");
		return 0;
	}

	if (!strncmp(name, "recovery-", 9)) {
		struct phy next_phy = { .id = 3 };
		struct mux_control next_mux = {};
		struct apple_connector next_connector = { .candidate_crtcs = 1 };
		struct apple_dcp_typec_port next_port = {
			.connector = &next_connector,
			.dp_wanted = true,
			.dp_hpd = true,
		};
		struct apple_dcp_typec_route next_route = {
			.dcp = &dcp,
			.port = &next_port,
			.phy = &next_phy,
			.dptx_phy = 3,
			.xbar = &next_mux,
		};
		struct typec_mux_dev next_provider = { .data = &next_route };

		host_list_append(&next_port.routes, &next_route.port_link, &next_route);
		/* Put the retained owner second: reservations must scan every port. */
		dcp_typec_ports = (struct list_head){};
		port.link = (struct list_head){};
		host_list_append(&dcp_typec_ports, &next_port.link, &next_port);
		host_list_append(&dcp_typec_ports, &port.link, &port);
		state = (struct typec_mux_state){
			.mode = TYPEC_DP_STATE_A, .alt = &alt, .data = &dp,
		};
		select_error = -EIO;
		if (!strcmp(name, "recovery-teardown-error"))
			deselect_error = -EIO;
		assert(dcp_typec_route_finish(&mux, &state) == -EIO);
		assert(port.owner == &route && !port.safe_pending && !port.applied_valid);
		assert(dcp.hdmi_yielded && !fixed_mux.held && !typec_mux.held);
		assert(!connects && !typec_connects && !next_port.owner);
		if (deselect_error) {
			assert(dcp.active_typec_route == &route && route.deactivate_error == -EIO);
			dp.status = DP_STATUS_HPD_STATE;
			assert(dcp_typec_route_finish(&mux, &state) == -EIO);
			assert(!port.applied_valid && !typec_selects && !typec_connects);
			puts("PASS an unproven crossbar teardown cannot acknowledge HPD-high");
			return 0;
		}
		assert(!dcp.active_typec_route && !route.selected && !dcp.typec_connector);
		assert(!dcp.fixed_route_selected && retired == 1 && selects == 1);
		if (strcmp(name, "recovery-hpd-high")) {
			guard(mutex)(&dcp_typec_fabric_lock);
			assert(!dcp_typec_route_available(&next_route));
			assert(!dcp_typec_free_route(&next_port));
			assert(dcp.fabric.owned && !dcp.fabric.fixed_busy);
			assert(dcp.fabric.presence == DCP_FABRIC_ABSENT);
			/* Another capacity event must not allocate the reserved pipeline. */
			dcp_typec_pipeline_freed();
			assert(!next_port.owner && !typec_connects && !typec_selects);
		}

		select_error = 0;
		if (!strcmp(name, "recovery-low-to-waiter")) {
			/* A real HPD-low retry completes release and wakes the other port. */
			gpio = 0;
			assert(!dcp_typec_route_finish(&mux, &state));
			assert(!port.owner && port.applied_valid);
			assert(next_port.owner == &next_route && next_route.selected);
			assert(dcp.active_typec_route == &next_route && next_mux.held);
			assert(typec_connects == 1 && !typec_mux.held && !connects);
			dp.status = DP_STATUS_HPD_STATE;
			assert(dcp_typec_route_finish(&mux, &state) == -ENOSPC);
			assert(!port.owner && !port.applied_valid && typec_connects == 1);
			/* The competitor leaves: real capacity wakeup reacquires the original. */
			dp.status = 0;
			assert(!dcp_typec_route_finish(&next_provider, &state));
			assert(!next_port.owner && port.owner == &route && route.selected);
			assert(dcp.active_typec_route == &route && typec_mux.held);
			assert(typec_connects == 2 && !next_mux.held && !connects);
			assert(retired == 4 && dcp.modes_generation == 7);
			puts("PASS production allocation, capacity wakeup and later reacquisition");
			return 0;
		}
		if (!strcmp(name, "recovery-hpd-high-fixed-error"))
			select_error = -EIO;
		if (!strcmp(name, "recovery-hpd-high-mux-error"))
			typec_select_error = -EBUSY;
		if (!strcmp(name, "recovery-hpd-high-disconnect-error"))
			activate_disconnect_error = -ETIMEDOUT;
		dp.status = DP_STATUS_HPD_STATE;
		ret = dcp_typec_route_finish(&mux, &state);
		if (typec_select_error || activate_disconnect_error) {
			assert(ret == (typec_select_error ?: activate_disconnect_error));
			assert(port.owner == &route && !port.applied_valid);
			assert(!route.selected && !dcp.active_typec_route && !typec_connects);
			{
				guard(mutex)(&dcp_typec_fabric_lock);
				assert(!dcp_typec_route_available(&next_route));
				assert(!dcp_typec_free_route(&next_port));
			}
			typec_select_error = activate_disconnect_error = 0;
			/* Only this next mux event retries the failed acquisition. */
			ret = dcp_typec_route_finish(&mux, &state);
		}
		assert(!ret && port.applied_valid && port.hpd);
		assert(port.owner == &route && route.selected && dcp.active_typec_route == &route);
		assert(typec_mux.held && !fixed_mux.held && !dcp.fixed_route_selected);
		assert(dcp.phy == &typec_phy && dcp.dptx_phy == 1);
		assert(dcp.typec_connector == &typec_connector && typec_connector.dcp == &device);
		assert(typec_connects == 1 && !connects && dcp.hdmi_yielded);
		assert(retired == 2 && dcp.modes_generation == 5);
		assert(!route.deactivate_xbar_done && !route.deactivate_error);
		{
			guard(mutex)(&dcp_typec_fabric_lock);
			assert(!dcp_typec_route_available(&next_route));
			assert(!dcp_typec_free_route(&next_port));
		}
		printf("PASS production HPD-high reacquisition: %s\n", name);
		return 0;
	}

	if (!strcmp(name, "not-yielded"))
		dcp.hdmi_yielded = false;
	if (!strcmp(name, "no-fixed-mux"))
		dcp.fixed_xbar = NULL;
	if (!strcmp(name, "inactive"))
		dcp.active = false;
	if (!strcmp(name, "absent") || !strcmp(name, "hpd-rise") ||
	    !strcmp(name, "late-select-failure"))
		gpio = 0;
	if (!strcmp(name, "select-failure") || !strcmp(name, "hpd-select-failure"))
		select_error = -EBUSY;
	if (!strcmp(name, "deactivate-failure"))
		deselect_error = -EIO;
	if (!strcmp(name, "disconnect-failure"))
		disconnect_error = -ETIMEDOUT;
	if (!strcmp(name, "connect-failure") || !strcmp(name, "safe-connect-failure"))
		connect_error = -ETIMEDOUT;
	if (!strcmp(name, "desktop")) {
		dcp.fw_compat = DCP_FIRMWARE_V_26_6_DESKTOP;
		dcp.hw.iomfb_method_profile = 0;
		dcp.hdmi_yielded = false;
	}
	yielded = dcp.hdmi_yielded;

	if (!strcmp(name, "route-move")) {
		guard(mutex)(&dcp_typec_fabric_lock);
		/* Selecting a fixed output must also respect the current borrower. */
		assert(!dcp_fixed_output_select(&dcp));
		assert(dcp.phy == &typec_phy && !selects);
		assert(!dcp_typec_route_deactivate(&route));
		assert(dcp.hdmi_yielded && dcp.phy == &typec_phy);
		assert(!dcp.fixed_route_selected && !selects && !connects);
		puts("PASS route moves retain yielded Type-C parking");
		return 0;
	}

	if (!strcmp(name, "hpd") || !strcmp(name, "hpd-select-failure") ||
	    !strcmp(name, "connect-failure") || !strcmp(name, "desktop") ||
	    !strcmp(name, "exit")) {
		/* FW27's finish callback dispatches the actual non-SAFE apply body. */
		state.mode = TYPEC_DP_STATE_A;
		state.alt = &alt;
		state.data = &dp;
		if (!strcmp(name, "exit"))
			state.alt = NULL;
		ret = dcp.fw_compat == DCP_FIRMWARE_V_27_0 ?
			dcp_typec_route_finish(&mux, &state) : dcp_typec_route_apply(&mux, &state);
		if (select_error) {
			assert(ret == -EBUSY && !connects && errors == 1);
			assert(port.owner == &route && !port.applied_valid);
			assert(!route.selected && !dcp.active_typec_route);
			assert(dcp.hdmi_yielded && !dcp.fixed_route_selected);
			assert(!fixed_mux.held && !typec_mux.held && retired == 1);
			select_error = 0;
			/* Retry only because another identical mux event was delivered. */
			ret = dcp_typec_route_finish(&mux, &state);
			assert(deselects == 1 && retired == 2 && disconnects == 1);
		}
		assert(ret == connect_error);
		assert(!port.owner && !port.safe_pending && !typec_connects);
		assert(connects == 1 && !dcp.hdmi_yielded);
		assert(selects >= 1 && !dcp.typec_connector);
		assert(dcp.phy == &fixed_phy && dcp.dptx_phy == 2);
		assert(dcp.fixed_route_selected && fixed_mux.held);
		if (connect_error)
			assert(errors == 1 && !port.applied_valid);
		else
			assert(port.applied_valid);
		if (!strcmp(name, "desktop"))
			assert(!external_publications && !metadata_starts);
		else
			assert(external_publications == 1 && metadata_starts == 1);
		puts("PASS actual non-SAFE release, selection, connect and error propagation");
		return 0;
	}

	ret = dcp_typec_route_prepare(&mux, &state);
	assert(port.owner == &route && port.safe_pending && !port.applied_valid);
	if (disconnect_error) {
		assert(ret == disconnect_error && route.selected && !connects);
		assert(dcp.active_typec_route == &route && dcp.hdmi_yielded);
		assert(typec_mux.held && !fixed_mux.held && !retired);
		puts("PASS failed firmware withdrawal retains the Type-C owner");
		return 0;
	}
	assert(!ret && disconnects == 1 && !connects && !selects);
	ret = dcp_typec_route_finish(&mux, &state);
	if (deselect_error) {
		assert(ret == -EIO && !connects && errors == 1);
		assert(port.owner == &route && port.safe_pending);
		assert(!port.safe_finish_ready && dcp.hdmi_yielded);
		assert(dcp.active_typec_route == &route && !fixed_mux.held);
		assert(dcp_typec_route_finish(&mux, &state) == -EIO);
		assert(deselects == 1 && !retired && !selects);
		puts("PASS failed crossbar teardown retains the owner without double release");
		return 0;
	}
	if (select_error) {
		assert(ret == -EBUSY && !connects && errors == 1);
		assert(port.owner == &route && port.safe_pending);
		assert(!port.safe_finish_ready && !dcp.fixed_route_selected);
		assert(dcp.hdmi_yielded && !route.selected);
		assert(!typec_mux.held && !fixed_mux.held && retired == 1);
		select_error = 0;
		assert(!dcp_typec_route_prepare(&mux, &state));
		ret = dcp_typec_route_finish(&mux, &state);
		assert(deselects == 1 && retired == 1 && disconnects == 1);
	}
	assert(!ret && port.owner == &route && port.safe_finish_ready);
	assert(!connects && !dcp.active_typec_route && !route.selected);
	assert(dcp.hdmi_yielded == yielded && retired == 1 && !dcp.modes_admitted);
	assert(!typec_connector.dcp && !typec_connector.connected);
	assert(encoder.possible_crtcs == typec_connector.candidate_crtcs);
	assert(dcp.connector == &fixed_connector && fixed_connector.dcp == &device);
	if (gpio) {
		assert(dcp.phy == &fixed_phy && dcp.dptx_phy == 2);
		assert(!dcp.fixed_xbar || (dcp.fixed_route_selected && fixed_mux.held));
		assert(!dcp.fixed_xbar || selection_under_external_lock);
	} else {
		assert(dcp.phy == &typec_phy && !dcp.fixed_route_selected);
		assert(!selects);
	}
	if (!strcmp(name, "hpd-rise") || !strcmp(name, "late-select-failure"))
		gpio = 1;
	if (!strcmp(name, "late-select-failure"))
		select_error = -EBUSY;
	dcp_typec_route_commit(&mux, &state);
	assert(!port.owner && !port.safe_pending && !port.safe_finish_ready);
	assert(port.applied_valid && port.preferred_route == &route);
	assert(!typec_connects && !dcp.hdmi_yielded);
	if (!gpio || !dcp.active || select_error) {
		assert(!connects && retired == 1 && !external_publications);
		assert(!select_error || (errors == 1 && !dcp.fixed_route_selected));
	} else {
		assert(connects == 1 && retired == 2);
		assert(dcp.external_generation == 6 && dcp.typec_generation == 10);
		assert(metadata_starts == 1 && external_publications == 1);
		assert(dcp.phy == &fixed_phy && dcp.dptx_phy == 2);
		assert(!connect_error || errors == 1);
	}
	/* Commit has published the release; repeating it cannot retry the link. */
	int attempts = connects;

	dcp_typec_route_commit(&mux, &state);
	assert(connects == attempts && !typec_connects);
	assert(!dcp.external_lock.held && !dcp.tb_lock.held && !dcp.hpd_mutex.held);
	printf("PASS production SAFE handback: %s\n", name);
	return 0;
}
'''


HANDBACK_CASES = ('safe', 'hpd', 'exit', 'select-failure', 'hpd-select-failure',
         'disconnect-failure', 'deactivate-failure', 'connect-failure',
         'safe-connect-failure', 'absent', 'hpd-rise', 'late-select-failure',
         'inactive', 'no-fixed-mux', 'not-yielded', 'route-move', 'desktop',
         'recovery-hpd-high', 'recovery-hpd-high-fixed-error',
         'recovery-hpd-high-mux-error', 'recovery-hpd-high-disconnect-error',
         'recovery-competing-allocator', 'recovery-low-to-waiter',
         'recovery-teardown-error', 'yield-failure', 'idle-fixed-attach',
         'idle-fixed-stop-failure', 'idle-fixed-mux-failure')


def handback(root, fabric=None):
    apple = root / 'drivers/gpu/drm/apple'
    if fabric is None:
        fabric = (apple / 'dcp-fabric.c').read_text()
    dcp = (apple / 'dcp.c').read_text()
    core = (apple / 'dcp-fabric-core.h').read_text()
    effects = (apple / 'dcp-fabric-effects.h').read_text()
    # The real pipeline type is embedded by the host DCP boundary below.
    prefix, dcp_struct = HANDBACK_PREFIX.split('struct apple_dcp {', 1)
    unit = prefix + '#include <limits.h>\n'
    for marker in ('enum dcp_fabric_presence_state {',
                   'struct dcp_fabric_presence {', 'struct dcp_fabric_pipeline {'):
        unit += block(core, marker) + ';\n'
    unit += 'struct apple_dcp {' + dcp_struct + '\n'
    for source, marker in (
        (core, 'struct dcp_fabric_route {'),
        (core, 'struct dcp_fabric_plan {'),
        (core, 'struct dcp_fabric_port {'),
        (core, 'struct dcp_fabric_policy {'),
        (core, 'enum dcp_fabric_capacity_action {'),
        (core, 'enum dcp_fabric_fixed_step {'),
        (fabric, 'struct apple_dcp_typec_port {'),
        ((apple / 'dcp-fabric.h').read_text(),
         'struct apple_dcp_typec_route {'),
        (effects, 'struct dcp_fabric_fixed_ops {'),
    ):
        unit += block(source, marker) + ';\n'
    unit += HANDBACK_MOCKS + '\n'
    for source, markers in (
        ((apple / 'dcp-fabric-core.c').read_text(),
         ('unsigned int dcp_fabric_fixed_steps(',
          'enum dcp_fabric_capacity_action dcp_fabric_capacity_action(',
          'bool dcp_fabric_fixed_busy(', 'bool dcp_fabric_available(',
          'unsigned int dcp_fabric_score(', 'bool dcp_fabric_waiting(',
          'struct dcp_fabric_route *\ndcp_fabric_free_route(',
          'u32 dcp_fabric_connector_mask(')),
        (effects, ('static inline int dcp_fabric_run_fixed(',)),
        ((apple / 'iomfb.c').read_text(), ('bool dcp_modes_end_typec(',)),
        (fabric, ('bool dcp_is_typec_output(',
                  'bool dcp_typec_follows_crtc(',
                  'static bool dcp_typec_narrows(',
                  'static bool dcp_typec_port_follows(',
                  'static bool dcp_typec_safe_held(',
                  'static bool dcp_typec_route_fixed_output_busy(',
                  'static bool dcp_typec_route_busy(',
                  'static enum dcp_fabric_presence_state dcp_hdmi_presence(',
                  'static bool dcp_typec_route_available(',
                  'static bool dcp_typec_tunnel_held(',
                  'static struct apple_dcp_typec_route *\ndcp_fabric_real_route(',
                  'static void dcp_fabric_snapshot_port(',
                  'static struct apple_dcp_typec_route *\ndcp_typec_free_route(',
                  'static struct apple_dcp_typec_route *\ndcp_typec_take_hdmi_pipeline_locked(',
                  'static int dcp_typec_port_busy(',
                  'static int dcp_typec_route_activate(',
                  'static void dcp_typec_port_attach(',
                  'static void dcp_typec_route_waiting(',
                  'static void dcp_typec_pipeline_freed(',
                  'static void dcp_fixed_restore_phy(',
                  'static int dcp_fixed_select_mux(',)),
    ):
        unit += '\n'.join(block(source, m) for m in markers) + '\n'
    unit += block(fabric, 'static const struct dcp_fabric_fixed_ops dcp_fixed_ops = {')
    unit += ';\n'
    for source, markers in (
        (fabric, ('static int dcp_fixed_output_select(',
                  'static int dcp_typec_route_deactivate(',)),
        (dcp, ('int dcp_dptx_connect(',)),
        (fabric, ('static int dcp_typec_route_release(',
                  'static int dcp_typec_connect_fixed(',
                  'static int dcp_typec_route_prepare(',
                  'static int dcp_typec_route_finish(',
                  'static void dcp_typec_route_commit(',
                  'static bool dcp_typec_route_is_dp(',
                  'static int dcp_typec_route_apply(')),
    ):
        for marker in markers:
            # The historical control predates these two handback adapters.
            if marker in ('static int dcp_typec_route_release(',
                          'static int dcp_typec_connect_fixed(') and marker not in source:
                continue
            unit += block(source, marker) + '\n'
    return unit + HANDBACK_ASSERTIONS, tuple((c,) for c in HANDBACK_CASES)


def handback_killed(name, unit, output, cases):
    """A negative control must compile and then fail a runtime assertion."""
    try:
        run_unit(name, unit, output, cases)
    except subprocess.CalledProcessError as error:
        assert error.cmd[0] == str(output / name / 'host'), name
        assert error.returncode == -6, (name, 'not an assertion kill', error)
        logs = sorted((output / name).glob('case-*.log'))
        failures = [p.read_text() for p in logs if 'Assertion' in p.read_text()]
        assert failures, (name, 'missing assertion log')
        print(f'PASS negative control caught: {name}')
        return {'name': name, 'assertion': failures[-1].strip()}
    raise AssertionError(f'negative control survived: {name}')


def handback_mutations(unit, output, cases):
    release = block(unit, 'static int dcp_typec_route_release(')
    finish = block(unit, 'static int dcp_typec_route_finish(')
    apply = block(unit, 'static int dcp_typec_route_apply(')
    deactivate = block(unit, 'static int dcp_typec_route_deactivate(')
    retire = block(unit, 'bool dcp_modes_end_typec(')
    held = block(unit, 'static bool dcp_typec_safe_held(')
    changes = (
        ('keep-yielded', release, release.replace(
            'WRITE_ONCE(dcp->hdmi_yielded, false);', '(void)0;')),
        ('lose-yield-reservation', release, release.replace(
            'WRITE_ONCE(dcp->hdmi_yielded, true);', '(void)0;')),
        ('drop-owner-before-finish', finish, finish.replace(
            'ret = port->owner ?', 'port->owner = NULL;\n\tret = port->owner ?')),
        ('hpd-bypass-release', apply, apply.replace(
            'dcp_typec_route_release(port->owner)',
            'dcp_typec_route_deactivate(port->owner)')),
        ('omit-final-select', 'ret = dcp_fixed_output_select(dcp);', 'ret = 0;'),
        ('ignore-select-error', deactivate, deactivate.replace(
            'if (ret)\n\t\t\t\t\treturn ret;',
            'if (ret)\n\t\t\t\t\tret = 0;')),
        ('ignore-connect-error', apply, apply.replace(
            'dcp_typec_pipeline_freed();\n\t\t\tif (ret)\n\t\t\t\treturn ret;',
            'dcp_typec_pipeline_freed();')),
        ('lose-typec-retirement', retire, retire.replace(
            'dcp_modes_invalidate_locked(dcp);', '(void)0;')),
        ('require-old-fixed-lease', 'if (released->fixed_phy)',
         'if (released->fixed_phy && released->fixed_route_selected)'),
        ('connect-before-commit', finish, finish.replace(
            'port->safe_finish_ready = true;',
            'port->safe_finish_ready = true;\n'
            '\tdcp_typec_connect_fixed(port->owner->dcp);')),
        ('omit-final-unyield', 'WRITE_ONCE(released->hdmi_yielded, false);',
         '(void)0;'),
        ('invisible-retained-owner', held, held.replace(
            'port->safe_pending || dcp->fw_compat == DCP_FIRMWARE_V_27_0',
            'port->safe_pending')),
        ('skip-hpd-high-reactivation',
         'ret = dcp_typec_route_activate(owner, owner->xbar);', 'ret = 0;'),
        ('ignore-reactivation-error', apply, apply.replace(
            'ret = dcp_typec_route_activate(owner, owner->xbar);\n'
            '\t\t\tif (ret)\n\t\t\t\treturn ret;',
            'ret = dcp_typec_route_activate(owner, owner->xbar);\n\t\t\tret = 0;')),
        ('never-recover-retained-owner',
         'ret = dcp_typec_route_activate(owner, owner->xbar);',
         'ret = -EAGAIN;'),
        ('accept-unproven-teardown', apply, apply.replace(
            'if (owner->deactivate_error)\n\t\t\treturn owner->deactivate_error;',
            'if (false)\n\t\t\treturn owner->deactivate_error;')),
        ('lose-yield-error', 'return PTR_ERR(best);', 'return -ENOSPC;'),
    )
    results = []
    for name, old, new in changes:
        assert old != new and unit.count(old) == 1, name
        results.append(handback_killed(name, unit.replace(old, new, 1), output, cases))
    (output / 'mutations.json').write_text(json.dumps(results, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--mutations', action='store_true')
    parser.add_argument('--old-ref', help='negative control: only the fabric from this local ref')
    parser.add_argument('--review-ref', help='negative controls: the reviewed stale-owner fabric')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[6]
    output = args.output or Path(tempfile.mkdtemp(prefix='dcp-handback-'))
    unit, cases = handback(root)
    run_unit('hdmi-handback', unit, output, cases)
    print(f'PASS {len(cases)} production handback cases; artifacts: {output}')
    if args.mutations:
        handback_mutations(unit, output, cases)
    if args.old_ref:
        fabric = subprocess.check_output(
            ['git', 'show', args.old_ref + ':drivers/gpu/drm/apple/dcp-fabric.c'],
            cwd=root, text=True)
        old, _ = handback(root, fabric)
        handback_killed('old-handback', old, output, (('safe',),))
    if args.review_ref:
        fabric = subprocess.check_output(
            ['git', 'show', args.review_ref + ':drivers/gpu/drm/apple/dcp-fabric.c'],
            cwd=root, text=True)
        old, _ = handback(root, fabric)
        for case in ('recovery-hpd-high', 'recovery-competing-allocator'):
            handback_killed('review-' + case, old, output, ((case,),))


if __name__ == '__main__':
    main()
