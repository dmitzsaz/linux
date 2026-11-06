#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Extracted firmware-27 initial-mode transaction; no kernel or hardware use."""
from pathlib import Path
import argparse
import tempfile

from common import block, run_unit


def transaction(root):
    apple = root / 'drivers/gpu/drm/apple'
    dcp = (apple / 'dcp.c').read_text()
    detach = block(dcp, 'static int dcp_external_disconnect(')
    assert detach.index('external_physically_absent, true') < detach.index('guard(mutex)(&dcp->external_lock)')
    enable = block(dcp, 'VISIBLE_IF_KUNIT void dcp_enable_typec_work(')
    assert enable.index('typec_work_enabled = true') < enable.rindex('dcp_external_queue_initial_mode(dcp)')
    stop = block(dcp, 'VISIBLE_IF_KUNIT void dcp_disable_typec_work(')
    assert 'disable_work_sync(&dcp->external_modeset_work)' in stop
    iomfb = (apple / 'iomfb.c').read_text()
    template = (apple / 'iomfb_template.c').read_text()
    state = (apple / 'iomfb-state.h').read_text().replace('#include <linux/spinlock.h>', '')
    prefix = r'''
#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#ifndef EREMOTEIO
#define EREMOTEIO 121
#endif
typedef uint32_t u32; typedef uint64_t u64; typedef int spinlock_t;
#define BIT(n) (1U << (n))
#define READ_ONCE(x) (x)
#define WRITE_ONCE(x,v) ((x)=(v))
#define guard(k) fake_guard
#define fake_guard(p) ((void)(p))
#define scoped_guard(k,p) for(int once=((void)(p),1);once;once=0)
#define spin_lock_irqsave(p,f) ((void)(p),(f)=0)
#define spin_unlock_irqrestore(p,f) ((void)(p),(void)(f))
#define lockdep_assert_held(p) ((void)(p))
#define container_of(p,t,m) ((t *)((char *)(p)-offsetof(t,m)))
#define DCP_FIRMWARE_V_27_0 27
#define DCP_IOMFB_METHODS_H17P_EXTERNAL 2
#define DCP_FW_VER 27
#define DCP_FW_VERSION(a,b,c) (a)
#define DCP_FW_H17(v) ((v)>=26)
#define DCP_FW_NAME(n) n##_v27
#define DCP_INHERIT_BOOT_MODE 1
#define GFP_KERNEL 0
#define DRM_MODE_FMT "mode"
#define DRM_MODE_ARG(m) 0
#define dev_info(...) ((void)0)
#define dev_dbg(...) ((void)0)
#define dev_err(...) ((void)0)
#define msecs_to_jiffies(n) (n)
#define jiffies_to_msecs(n) (n)
#define kzalloc(n,f) calloc(1,n)
struct completion { bool done; };
struct kref { int count; };
struct dcp_wait_cookie { struct completion done; struct kref refcount; u32 status; bool replied; };
static void init_completion(struct completion *p) { p->done=false; }
static void complete(struct completion *p) { p->done=true; }
static int wait_for_completion_timeout(struct completion *p,int t) { return p->done?t:0; }
static void kref_init(struct kref *p) { p->count=1; }
static void kref_get(struct kref *p) { p->count++; }
static void release_wait_cookie(struct kref *p) { free(container_of(p,struct dcp_wait_cookie,refcount)); }
static void kref_put(struct kref *p,void (*release)(struct kref *)) { if(!--p->count)release(p); }
struct work_struct { bool pending,disabled; };
struct delayed_work { struct work_struct work; };
static int schedule_work(struct work_struct *w) { if(w->disabled)return 0;w->pending=true;return 1; }
static void cancel_delayed_work_sync(struct delayed_work *w) { w->work.pending=false; }
struct drm_display_mode { int clock,hdisplay,vdisplay; };
struct drm_crtc_state { struct drm_display_mode mode; bool vrr_enabled; };
struct dcp_color_mode { int id,score,depth,format,colorimetry,eotf,range; };
struct dcp_display_mode {
 struct drm_display_mode mode;
 struct dcp_color_mode pq,sdr_rgb,sdr_444,sdr,best;
 u32 timing_mode_id,color_mode_id,min_vrr;
 bool sdr_usable,automatic_rgb8_quirk,firmware_preferred,needs_timestamps,vrr;
};
struct dcp_set_digital_out_mode_req { u32 color_mode_id,timing_mode_id; };
struct apple_connector { bool connected,edid_live,edid_invalidation_pending; };
'''
    prefix += state + r'''
struct apple_dcp {
 int external_lock,modes_lock,fw_compat;
 struct { int iomfb_method_profile; } hw;
 struct { int lock;bool presentation_paused; } iomfb;
 struct { bool connected; } dptxport[1];
 struct apple_connector *connector;
 struct dcp_display_mode *modes;
 unsigned nr_modes;
 u64 modes_generation,external_generation,external_rx_generation,external_crtc_generation;
 u64 external_hotplug_generation,external_stop_generation;
 u64 external_mode_generation,external_mode_catalog_generation;
 bool modes_admitted,modes_provisional,full_catalog_v27;
 bool external_fw_connected,external_hotplug_pending,external_hotplug_connected;
 bool external_mode_valid,external_running,external_cable,external_prepared;
 bool external_stopped,external_stop_deferred,external_power_attempted;
 unsigned external_stop_retries;
 bool typec_work_enabled,quiescing,crashed,external_physically_absent;
 bool pipe_enabled_h17p,typec_crtc_off,fixed_link_held;
 void *active_typec_route,*fixed_phy,*dev;
 int external_error;
 struct dcp_mode_state mode_state;
 struct work_struct external_modeset_work,external_stop_work;
 struct delayed_work typec_reconnect_wq,placeholder_edid_wq;
 struct dcp_set_digital_out_mode_req mode,external_mode;
 bool pq_requested,output_pq,use_timestamps,main_display,vrr_enabled;
 u64 swap_submit_timestamp;
};
static bool dcp_is_external_h17p(struct apple_dcp *d) { return d->hw.iomfb_method_profile==2; }
static bool dcp_modes_for_connector(struct apple_dcp *d,struct apple_connector *c) { return d->modes_admitted&&d->connector==c; }
static u64 dcp_modes_transfer_begin(struct apple_dcp *d) { return d->modes_generation; }
static bool lookup_mode(struct apple_dcp *d,const struct drm_display_mode *m,struct dcp_display_mode *out) {
 for(unsigned i=0;i<d->nr_modes;i++)if(!memcmp(m,&d->modes[i].mode,sizeof(*m))) { *out=d->modes[i];return true; }
 return false;
}
static void dcp_firmware_metadata_announced(struct apple_dcp *d) { (void)d; }
static bool metadata_live;
static void dcp_firmware_metadata_hotplug(struct apple_dcp *d,bool c) { (void)d;metadata_live=c; }
static void apple_connector_edid_set_live(struct apple_connector *c,bool live) { if(c)c->edid_live=live; }
static unsigned notices,powers,stops,rpcs;
static void dcp_handle_hotplug_actions(struct apple_dcp *d,unsigned action) {
 if(d->connector&&d->connector->connected&&d->mode_state.valid)assert(d->external_mode_valid&&metadata_live&&d->connector->edid_live);
 notices+=!!action;
}
static int dcp_external_link_locked(struct apple_dcp *d) { assert(d->external_prepared);return 0; }
static void iomfb_pause_presentation(struct apple_dcp *d,bool p) { d->iomfb.presentation_paused=p; }
static int iomfb_external_power_h17p(struct apple_dcp *d,bool on) { assert(on&&!d->connector->connected);powers++;return 0; }
static int dcp_external_stop_locked(struct apple_dcp *d,u64 g) { assert(g==d->external_generation);stops++;d->external_mode_valid=false;return 0; }
static void dpavservep_detach(struct apple_dcp *d) { (void)d; }
static void dcp_external_report_disconnected(struct apple_dcp *d) { d->connector->connected=false; }
static void dcp_external_hdr_hints(struct apple_dcp *d,bool p) { (void)d;(void)p; }
static void dcp_set_adaptive_sync(struct apple_dcp *d,u32 m,void *c) { (void)d;(void)m;(void)c;assert(false); }
static bool dcp_external_hotplug(struct apple_dcp *,bool);
static void dcp_set_digital_out_mode(struct apple_dcp *,bool,struct dcp_set_digital_out_mode_req *,void (*)(struct apple_dcp *,void *,void *),void *);
'''
    production = block(template, 'static void complete_set_digital_out_mode(')
    production += '\n' + block(template, template.rindex('int DCP_FW_NAME(iomfb_modeset)('))
    for name in ('void dcp_modes_begin_attachment(', 'int dcp_external_initial_mode(', 'int dcp_external_modeset('):
        production += '\n' + block(iomfb, name)
    for name in ('static int dcp_external_start_locked(', 'void dcp_external_queue_initial_mode(',
                 'static void dcp_external_modeset_work(', 'static void dcp_external_stop_work(',
                 'bool dcp_external_hotplug('):
        production += '\n' + block(dcp, name).replace('bool dcp_external_hotplug(', 'static bool dcp_external_hotplug(', 1)
    tests = r'''
enum event { NONE, BOUNCE, LOW, DETACH, STALE, QUIESCE, FAULT };
static enum event event;
static u32 reply;
static bool missing;
static void dcp_set_digital_out_mode(struct apple_dcp *d,bool b,struct dcp_set_digital_out_mode_req *m,
 void (*cb)(struct apple_dcp *,void *,void *),void *c) {
 (void)b;(void)m;rpcs++;
 assert(d->mode_state.changing);
 if(!d->connector->connected)assert(powers==1&&!d->external_mode_valid);
 if(event==BOUNCE||event==LOW) {
  /* The fixed-sink trampoline retires its catalog before generic HPD. */
  if(d->fixed_phy)dcp_modes_begin_attachment(d);
  assert(dcp_external_hotplug(d,false));
  assert(!d->external_stop_work.pending);
  if(event==BOUNCE)assert(dcp_external_hotplug(d,true));
 }
 if(event==DETACH)d->external_physically_absent=true;
 if(event==STALE)d->modes_generation++;
 if(event==QUIESCE)d->typec_work_enabled=false;
 if(event==FAULT)dcp_mode_invalidate(&d->mode_state);
 cb(d,missing?NULL:&reply,c);
}
static struct apple_dcp d;
static struct apple_connector con;
static struct dcp_display_mode modes[4];
static void init(bool fixed) {
 d=(struct apple_dcp){.fw_compat=27,.hw.iomfb_method_profile=2,.connector=&con,
  .external_prepared=true,.external_cable=true,.external_generation=7,.external_rx_generation=7,
  .external_crtc_generation=6,.modes_generation=11,.modes_admitted=true,
  .modes=modes,.nr_modes=4,.typec_work_enabled=true,.active_typec_route=fixed?NULL:&d,
  .fixed_phy=fixed?&d:NULL,.iomfb.presentation_paused=true};
 con=(struct apple_connector){};
 /* Boot 36: native 60 Hz timing 46/color 109; compositor timing 100/color 103.
  * Other geometries ensure neither smallest mode nor largest area wins. */
 modes[0]=(struct dcp_display_mode){.mode={148500,1920,1080},.sdr_usable=true,.timing_mode_id=1};
 modes[1]=(struct dcp_display_mode){.mode={580080,2560,1440},.sdr_usable=true,.timing_mode_id=100,.color_mode_id=103};
 modes[2]=(struct dcp_display_mode){.mode={241500,2560,1440},.sdr_usable=true,.firmware_preferred=true,.timing_mode_id=46,.color_mode_id=109};
 modes[3]=(struct dcp_display_mode){.mode={594000,3840,2160},.sdr_usable=true,.timing_mode_id=200};
 notices=powers=stops=rpcs=0;event=NONE;reply=0;missing=false;metadata_live=false;
}
static void drain_initial(void) { if(d.external_modeset_work.pending){d.external_modeset_work.pending=false;dcp_external_modeset_work(&d.external_modeset_work);} }
static int apply(unsigned i) { struct drm_crtc_state s={.mode=modes[i].mode};return dcp_external_modeset(&d,&s,d.modes_generation); }
int main(void) {
 for(unsigned fixed=0;fixed<2;fixed++) {
  init(fixed);assert(dcp_external_hotplug(&d,true));assert(!con.connected&&powers==0);
  event=BOUNCE;drain_initial();assert(con.connected&&d.mode_state.valid&&d.external_mode_valid);
  assert(d.mode.timing_mode_id==46&&d.mode.color_mode_id==109&&powers==1&&rpcs==1);
  assert(d.modes_generation==11&&!d.external_stop_work.pending&&!d.fixed_link_held);
  assert(!apply(2)&&rpcs==1); /* Established identical pair skips A413. */
  event=NONE;assert(!apply(1)&&rpcs==2&&d.external_mode.timing_mode_id==100);
  reply=6;assert(apply(2)==-EREMOTEIO&&!d.external_mode_valid&&!d.mode_state.valid);
  reply=0;assert(!apply(1)&&rpcs==4); /* Failure invalidates the old pair. */
  event=LOW;assert(apply(2)==-ENOLINK&&!con.connected&&!d.external_mode_valid);
  assert(d.external_stop_work.pending==!fixed); /* No fixed-link withdrawal. */
  if(fixed)assert(d.modes_generation==12&&!d.modes_admitted);
 }
 for(enum event e=DETACH;e<=FAULT;e++) {
  init(false);event=e;dcp_external_hotplug(&d,true);drain_initial();
  assert(!con.connected&&!d.external_mode_valid&&!d.mode_state.valid&&rpcs==1);
 }
 init(false);dcp_external_hotplug(&d,true);missing=true;drain_initial();assert(!con.connected&&!d.external_mode_valid);
 init(false);dcp_external_hotplug(&d,true);drain_initial();
 assert(dcp_external_hotplug(&d,false)&&d.external_stop_work.pending);
 assert(!dcp_external_hotplug(&d,true));dcp_external_stop_work(&d.external_stop_work);assert(stops==0&&con.connected);
 d.external_hotplug_generation--;d.external_fw_connected=false;dcp_external_stop_work(&d.external_stop_work);assert(!stops);
 /* D575/catalog before bind: re-enable schedules after admission opens. */
 init(false);d.external_modeset_work.disabled=true;d.typec_work_enabled=false;
 dcp_external_hotplug(&d,true);assert(!d.external_modeset_work.pending&&!con.connected);
 d.external_modeset_work.disabled=false;d.typec_work_enabled=true;dcp_external_queue_initial_mode(&d);
 drain_initial();assert(con.connected&&rpcs==1);
 /* Catalog after D575, without another HPD edge. */
 init(false);d.modes_admitted=false;dcp_external_hotplug(&d,true);drain_initial();assert(!powers);
 d.modes_admitted=true;dcp_external_queue_initial_mode(&d);drain_initial();assert(con.connected);
 /* Preference has no geometry literal; SDR exclusions still apply. */
 init(false);modes[2].mode.hdisplay=modes[1].mode.hdisplay=3440;
 struct drm_crtc_state s={};u64 g;assert(!dcp_external_initial_mode(&d,&s,&g)&&s.mode.hdisplay==3440);
 modes[2].sdr_usable=false;modes[1].firmware_preferred=true;
 assert(!dcp_external_initial_mode(&d,&s,&g)&&s.mode.clock==580080);
 puts("PASS FW27 initial mode, native clock, cache/status, transient/final HPD, detach/catalog/suspend fences, stale stop and replay");
}
'''
    return prefix + production + tests


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[6])
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    output = args.output or Path(tempfile.mkdtemp(prefix='apple-external-mode-'))
    run_unit('external-mode', transaction(args.source), output, extra=('-Wno-unused-function',))
    print(f'PASS: {output}')
