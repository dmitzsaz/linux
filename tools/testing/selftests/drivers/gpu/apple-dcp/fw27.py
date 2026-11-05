# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Firmware-27 converter ownership and local RemotePort units."""
from common import block


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
struct apple_connector { bool connected; };
struct dptx_port { bool enabled,connected; void *service; };
struct apple_dcp {
 int fw_compat,fixed_connector_type,hdmi_presence_lock,hpd_mutex,hdmi_hpd_irq;
 int *hdmi_hpd;
 void *active_typec_route,*fixed_phy;
 bool fixed_sink_known,fixed_sink_present,fixed_link_held,hdmi_yielded,external,quiescing,typec_work_enabled;
 struct apple_connector *fixed_connector;
 struct { enum dcp_fabric_presence_state state; u64 generation; } hdmi_presence;
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
    production = '\n'.join(block(fabric, x) for x in (
        'bool dcp_hdmi_port_attached(',
        'static bool dcp_typec_route_fixed_output_busy(',
        'static enum dcp_fabric_presence_state dcp_hdmi_presence(',
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
