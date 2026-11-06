#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Extracted firmware-27 initial-mode transaction; no kernel or hardware use."""
from pathlib import Path
import argparse
import tempfile
import copy
import struct
import subprocess
import json

import rate

from common import block, run_unit


def hdmi_state():
    return r'''
#define NSEC_PER_MSEC 1000000ULL
#define NSEC_PER_SEC 1000000000ULL
#define DCP_HDMI_IRQ_HPD_MAX_NS (2 * NSEC_PER_MSEC)
struct dcp_hdmi {
 u64 falling_ns;
 atomic64_t attachment_generation;
 bool timed_edges;
};
'''


def hdmi_helpers(root):
    source = (root / 'drivers/gpu/drm/apple/dcp-hdmi.c').read_text()
    return '\n'.join(block(source, marker) for marker in (
        'static bool dcp_hdmi_is_irq_hpd(',
        'VISIBLE_IF_KUNIT bool dcp_hdmi_record_hpd_edge(',
        'u64 dcp_hdmi_attachment_generation('))


def transaction(root):
    apple = root / 'drivers/gpu/drm/apple'
    dcp = (apple / 'dcp.c').read_text()
    detach = block(dcp, 'static int dcp_external_disconnect(')
    assert detach.index('external_physically_absent, true') < detach.index('guard(mutex)(&dcp->external_lock)')
    enable = block(dcp, 'VISIBLE_IF_KUNIT void dcp_enable_typec_work(')
    assert enable.index('typec_work_enabled = true') < enable.rindex('dcp_external_queue_initial_mode(dcp)')
    stop = block(dcp, 'VISIBLE_IF_KUNIT void dcp_disable_typec_work(')
    assert 'disable_work_sync(&dcp->external_modeset_work)' in stop
    probe = block(dcp, 'static int dcp_platform_probe_pipe(')
    work_init = probe[probe.index('INIT_WORK(&dcp->external_modeset_work,'):
                      probe.index('INIT_WORK(&dcp->external_stop_work,')]
    iomfb = (apple / 'iomfb.c').read_text()
    template = (apple / 'iomfb_template.c').read_text()
    state = (apple / 'iomfb-state.h').read_text().replace('#include <linux/spinlock.h>', '')
    work_state = '\n'.join(line for name in ('dcp-announce.h', 'dcp-relink.h')
                           for line in (apple / name).read_text().splitlines()
                           if not line.startswith('#include'))
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
typedef u64 atomic64_t;
static void atomic64_inc(atomic64_t *p) { ++*p; }
static u64 atomic64_read(atomic64_t *p) { return *p; }
struct dcp_timing_id { u32 id,width,height; };
#define BIT(n) (1U << (n))
#define READ_ONCE(x) (x)
#define WRITE_ONCE(x,v) ((x)=(v))
#define guard(k) fake_guard
#define fake_guard(p) ((void)(p))
#define scoped_guard(k,p) for(int once=((void)(p),1);once;once=0)
#define spin_lock_irqsave(p,f) ((void)(p),(f)=0)
#define spin_unlock_irqrestore(p,f) ((void)(p),(void)(f))
#define lockdep_assert_held(p) ((void)(p))
#define VISIBLE_IF_KUNIT
#define container_of(p,t,m) ((t *)((char *)(p)-offsetof(t,m)))
#define time_before(a,b) ((long)((a)-(b))<0)
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
/* Single-threaded queue model: cancellation removes pending callbacks and
 * disable depth rejects future enqueues, as kernel/workqueue.c requires. */
struct work_struct {
 bool pending,initialized;
 unsigned disable_depth;
 void (*func)(struct work_struct *);
};
struct delayed_work { struct work_struct work; };
static void init_work(struct work_struct *w,void (*func)(struct work_struct *))
{ *w=(struct work_struct){.initialized=true,.func=func}; }
#define INIT_WORK(w,fn) init_work(w,fn)
static bool schedule_work(struct work_struct *w) {
 assert(w->initialized);
 if(w->disable_depth||w->pending)return false;
 w->pending=true;return true;
}
static bool cancel_work_sync(struct work_struct *w)
{ assert(w->initialized);bool pending=w->pending;w->pending=false;return pending; }
static bool disable_work_sync(struct work_struct *w)
{ bool pending=cancel_work_sync(w);w->disable_depth++;return pending; }
#define disable_work disable_work_sync
static bool enable_work(struct work_struct *w)
{ assert(w->initialized&&w->disable_depth);return !--w->disable_depth; }
static void cancel_delayed_work_sync(struct delayed_work *w) { cancel_work_sync(&w->work); }
static void disable_delayed_work_sync(struct delayed_work *w) { disable_work_sync(&w->work); }
static void enable_delayed_work(struct delayed_work *w) { enable_work(&w->work); }
struct drm_display_mode { int clock,hdisplay,vdisplay,htotal,vtotal,type; };
struct drm_crtc_state { struct drm_display_mode mode; bool vrr_enabled; };
struct dcp_color_mode { int id,score,depth,format,colorimetry,eotf,range; };
struct dcp_display_mode {
 struct drm_display_mode mode;
 struct dcp_color_mode pq,sdr_rgb,sdr_444,sdr,best;
 u32 timing_mode_id,color_mode_id,min_vrr;
 bool sdr_usable,automatic_rgb8_quirk,firmware_preferred,needs_timestamps,vrr;
};
struct dcp_set_digital_out_mode_req { u32 color_mode_id,timing_mode_id; };
struct apple_connector {
 bool connected,edid_live,edid_invalidation_pending;
 void *dcp;
 struct work_struct hotplug_wq;
};
'''
    prefix += hdmi_state() + state + work_state + r'''
struct apple_dcp {
 int external_lock,modes_lock,fw_compat,hpd_mutex,relink_lock,hdmi_presence_lock;
 struct dcp_hdmi *hdmi;
 u64 modes_hdmi_generation;
 struct dcp_timing_id *preferred_timings_v27;
 unsigned nr_preferred_timings_v27;
 atomic64_t external_initial_request;
 u64 external_initial_attempt;
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
 struct delayed_work typec_relink_wq,announce_wq,external_stop_retry_wq;
 struct delayed_work hdmi_settle_wq,hdmi_recheck_wq,link_recovery_wq,hybrid_wq;
 struct dcp_announce announce;
 struct dcp_relink_state relink;
 bool typec_cable_connected;
 u64 typec_generation;
 struct dcp_set_digital_out_mode_req mode,external_mode;
 bool pq_requested,output_pq,use_timestamps,main_display,vrr_enabled;
 u64 swap_submit_timestamp;
};
static bool dcp_is_external_h17p(struct apple_dcp *d) { return d->hw.iomfb_method_profile==2; }
bool dcp_modes_for_connector(struct apple_dcp *,struct apple_connector *);
u64 dcp_modes_transfer_begin(struct apple_dcp *);
static bool lookup_mode(struct apple_dcp *d,const struct drm_display_mode *m,struct dcp_display_mode *out) {
 for(unsigned i=0;i<d->nr_modes;i++)if(!memcmp(m,&d->modes[i].mode,sizeof(*m))) { *out=d->modes[i];return true; }
 return false;
}
static void dcp_firmware_metadata_announced(struct apple_dcp *d) { (void)d; }
static bool metadata_live;
static void dcp_firmware_metadata_hotplug(struct apple_dcp *d,bool c) { (void)d;metadata_live=c; }
static void apple_connector_edid_set_live(struct apple_connector *c,bool live) { if(c)c->edid_live=live; }
static unsigned notices,powers,stops,rpcs,detaches;
static void dcp_handle_hotplug_actions(struct apple_dcp *d,unsigned action) {
 if(d->connector&&d->connector->connected&&d->mode_state.valid)assert(d->external_mode_valid&&metadata_live&&d->connector->edid_live);
 notices+=!!action;
}
static int dcp_external_link_locked(struct apple_dcp *d) { assert(d->external_prepared);return 0; }
static void iomfb_pause_presentation(struct apple_dcp *d,bool p) { d->iomfb.presentation_paused=p; }
static int iomfb_external_power_h17p(struct apple_dcp *d,bool on) { assert(on&&!d->connector->connected);powers++;return 0; }
static int dcp_external_stop_locked(struct apple_dcp *d,u64 g) { assert(g==d->external_generation);stops++;d->external_mode_valid=false;return 0; }
static void dpavservep_detach(struct apple_dcp *d) { (void)d;detaches++; }
static void dcp_external_report_disconnected(struct apple_dcp *d) { d->connector->connected=false; }
static void dcp_external_hdr_hints(struct apple_dcp *d,bool p) { (void)d;(void)p; }
static void dcp_set_adaptive_sync(struct apple_dcp *d,u32 m,void *c) { (void)d;(void)m;(void)c;assert(false); }
static bool dcp_external_hotplug(struct apple_dcp *,bool);
void dcp_external_queue_initial_mode(struct apple_dcp *);
static void dcp_set_digital_out_mode(struct apple_dcp *,bool,struct dcp_set_digital_out_mode_req *,void (*)(struct apple_dcp *,void *,void *),void *);
/* The HDMI service and announcement timeout are outside this transaction.
 * Model their boundaries; the mode worker and its enable/disable run below. */
static void dcp_hdmi_enable(struct apple_dcp *d) { (void)d; }
static void dcp_hdmi_disable(struct apple_dcp *d) { (void)d; }
static void dcp_announce_arm(struct apple_dcp *d) { assert(!d->announce.stopped); }
static void dcp_firmware_metadata_connection(struct apple_dcp *d,u64 g,bool connected)
{ assert(g==d->typec_generation&&!connected);metadata_live=false; }
static void dcp_queue_typec_reconnect(struct apple_dcp *d,unsigned long delay)
{ assert(d->typec_work_enabled&&!delay);schedule_work(&d->typec_reconnect_wq.work); }
#define to_platform_device(d) (d)
'''
    production = hdmi_helpers(root) + '\n' + block(template, 'static void complete_set_digital_out_mode(')
    production += '\n' + block(template, template.rindex('int DCP_FW_NAME(iomfb_modeset)('))
    for name in ('static void dcp_modes_invalidate_locked(', 'static bool dcp_modes_check_hdmi_locked(',
                 'bool dcp_modes_for_connector(', 'u64 dcp_modes_transfer_begin(',
                 'void dcp_modes_begin_attachment(', 'static bool dcp_external_native_mode(',
                 'static const struct dcp_display_mode *dcp_external_native_geometry(',
                 'int dcp_external_initial_mode(', 'int dcp_external_modeset('):
        production += '\n' + block(iomfb, name)
    for name in ('static int dcp_external_start_locked(', 'void dcp_external_queue_initial_mode(',
                 'VISIBLE_IF_KUNIT void dcp_external_modeset_work(', 'static void dcp_external_stop_work(',
                 'bool dcp_external_hotplug('):
        production += '\n' + block(dcp, name).replace('bool dcp_external_hotplug(', 'static bool dcp_external_hotplug(', 1)
    production += '\n' + stop + '\n' + enable
    production += '\nstatic void init_external_work(struct apple_dcp *dcp) {\n' + work_init + '\n}\n'
    tests = r'''
enum event { NONE, BOUNCE, LOW, DETACH, STALE, QUIESCE, FAULT, PHYSICAL, REQUEUE, PHYSICAL_CATALOG, IRQ_HPD };
static enum event event;
static u64 pulse_ns;
static void pulse(struct apple_dcp *d,u64 low_ns) {
 assert(!dcp_hdmi_record_hpd_edge(d,0,NSEC_PER_SEC));
 assert(dcp_hdmi_record_hpd_edge(d,1,NSEC_PER_SEC+low_ns)==(low_ns<=2*NSEC_PER_MSEC));
}
static u32 reply;
static bool missing;
static void dcp_set_digital_out_mode(struct apple_dcp *d,bool b,struct dcp_set_digital_out_mode_req *m,
 void (*cb)(struct apple_dcp *,void *,void *),void *c) {
 (void)b;(void)m;rpcs++;
 assert(d->mode_state.changing);
 if(!d->connector->connected)assert(powers==1&&!d->external_mode_valid);
 if(event==BOUNCE||event==LOW||event==PHYSICAL||event==PHYSICAL_CATALOG||event==IRQ_HPD) {
  if(event==PHYSICAL||event==PHYSICAL_CATALOG)pulse(d,2*NSEC_PER_MSEC+1);
  if(event==IRQ_HPD)pulse(d,pulse_ns);
  /* The fixed-sink trampoline retires its catalog before generic HPD. */
  if(d->fixed_phy)dcp_modes_begin_attachment(d);
  assert(dcp_external_hotplug(d,false));
  assert(!d->external_stop_work.pending);
  if(event==BOUNCE||event==PHYSICAL||event==PHYSICAL_CATALOG||event==IRQ_HPD)assert(dcp_external_hotplug(d,true));
 }
 if(event==DETACH)d->external_physically_absent=true;
 if(event==STALE)d->modes_generation++;
 if(event==QUIESCE)d->typec_work_enabled=false;
 if(event==FAULT)dcp_mode_invalidate(&d->mode_state);
 if(event==REQUEUE)dcp_external_queue_initial_mode(d);
 if(event==PHYSICAL_CATALOG) {
  d->modes_admitted=true;d->full_catalog_v27=true;
  for(unsigned i=0;i<d->nr_modes;i++)d->modes[i].timing_mode_id+=300;
  dcp_external_queue_initial_mode(d); /* A413 suppresses this publication. */
  assert(!d->external_modeset_work.pending);
 }
 cb(d,missing?NULL:&reply,c);
}
static struct apple_dcp d;
static struct dcp_hdmi hdmi;
static struct apple_connector con;
static struct dcp_display_mode modes[4];
static void unexpected_work(struct work_struct *w) { (void)w;assert(!"unrelated work executed"); }
static void init_state(bool fixed,bool bound) {
 d=(struct apple_dcp){.fw_compat=27,.hw.iomfb_method_profile=2,.connector=&con,
  .external_prepared=true,.external_cable=true,.external_generation=7,.external_rx_generation=7,
  .external_crtc_generation=6,.modes_generation=11,.modes_admitted=true,
  .modes=modes,.nr_modes=4,.active_typec_route=fixed?NULL:&d,.dev=&d,
  .typec_cable_connected=!fixed,
  .fixed_phy=fixed?&d:NULL,.iomfb.presentation_paused=true};
 con=(struct apple_connector){.dcp=&d};
 hdmi=(struct dcp_hdmi){.timed_edges=true};d.hdmi=&hdmi;
 init_external_work(&d);
 INIT_WORK(&d.external_stop_work,dcp_external_stop_work);
 INIT_WORK(&con.hotplug_wq,unexpected_work);
 init_work(&d.external_stop_retry_wq.work,unexpected_work);
 /* Probe and fabric init leave these disabled until component bind. */
 struct delayed_work *delayed[]={&d.typec_reconnect_wq,&d.placeholder_edid_wq,
  &d.typec_relink_wq,&d.announce_wq,&d.hdmi_settle_wq,&d.hdmi_recheck_wq,
  &d.link_recovery_wq,&d.hybrid_wq};
 for(unsigned i=0;i<sizeof(delayed)/sizeof(delayed[0]);i++) {
  init_work(&delayed[i]->work,unexpected_work);
  disable_work(&delayed[i]->work);
 }
 d.announce.stopped=true;
 /* Boot 36: native 60 Hz timing 46/color 109; compositor timing 100/color 103.
  * Other geometries ensure neither smallest mode nor largest area wins. */
 modes[0]=(struct dcp_display_mode){.mode={148500,1920,1080},.sdr_usable=true,.timing_mode_id=1};
 modes[1]=(struct dcp_display_mode){.mode={580080,2560,1440},.sdr_usable=true,.timing_mode_id=100,.color_mode_id=103};
 modes[2]=(struct dcp_display_mode){.mode={241500,2560,1440},.sdr_usable=true,.firmware_preferred=true,.timing_mode_id=46,.color_mode_id=109};
 modes[3]=(struct dcp_display_mode){.mode={594000,3840,2160},.sdr_usable=true,.timing_mode_id=200};
 notices=powers=stops=rpcs=detaches=0;event=NONE;reply=0;missing=false;metadata_live=false;
 if(bound)dcp_enable_typec_work(&d);
}
static void init(bool fixed) { init_state(fixed,true); }
static void drain_initial(void) {
 struct work_struct *w=&d.external_modeset_work;
 if(w->pending){assert(!w->disable_depth);w->pending=false;w->func(w);}
}
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
 init_state(false,false);
 assert(d.external_modeset_work.initialized&&d.external_modeset_work.disable_depth==1);
 dcp_external_hotplug(&d,true);assert(!d.external_modeset_work.pending&&!con.connected);
 dcp_enable_typec_work(&d);
 assert(d.typec_work_enabled&&d.external_modeset_work.pending&&!d.external_modeset_work.disable_depth);
 drain_initial();assert(con.connected&&rpcs==1);
 /* Suspend before the queued first mode: drain it and reject RX/catalog
  * enqueues until the actual re-enable replays the discovery. */
 for(unsigned fixed=0;fixed<2;fixed++) {
  init(fixed);dcp_external_hotplug(&d,true);assert(d.external_modeset_work.pending);
  dcp_disable_typec_work(&d,false);
  assert(!d.typec_work_enabled&&!d.external_modeset_work.pending&&d.external_modeset_work.disable_depth==1);
  dcp_external_queue_initial_mode(&d);drain_initial();assert(!rpcs&&!powers&&!con.connected);
  assert(d.typec_cable_connected==!fixed&&d.typec_generation==1);
  dcp_enable_typec_work(&d);drain_initial();assert(con.connected&&rpcs==1&&powers==1);
  assert(!d.external_modeset_work.disable_depth&&d.relink.enabled&&!d.announce.stopped);
 }
 /* Teardown also drains the work and releases the Type-C cable. */
 init(false);dcp_external_hotplug(&d,true);dcp_disable_typec_work(&d,true);
 assert(!d.typec_cable_connected&&detaches==1&&!d.external_modeset_work.pending);
 assert(!d.relink.enabled&&d.announce.stopped);
 dcp_external_queue_initial_mode(&d);drain_initial();assert(!rpcs&&!powers);
 /* Catalog after D575, without another HPD edge. */
 init(false);d.modes_admitted=false;dcp_external_hotplug(&d,true);drain_initial();assert(!powers);
 d.modes_admitted=true;dcp_external_queue_initial_mode(&d);drain_initial();assert(con.connected);
 /* Preference has no geometry literal; SDR exclusions still apply. */
 init(false);modes[2].mode.hdisplay=modes[1].mode.hdisplay=3440;
 struct drm_crtc_state s={};u64 g;assert(!dcp_external_initial_mode(&d,&s,&g)&&s.mode.hdisplay==3440);
 modes[2].sdr_usable=false;modes[1].firmware_preferred=true;
 assert(!dcp_external_initial_mode(&d,&s,&g)&&s.mode.clock==580080);
 /* Short physical IRQ_HPD pulses preserve the catalog and a cached pair. */
 const u64 short_pulses[]={500000,1000000,2000000};
 for(unsigned i=0;i<sizeof(short_pulses)/sizeof(short_pulses[0]);i++) {
  init(true);dcp_external_hotplug(&d,true);event=IRQ_HPD;pulse_ns=short_pulses[i];
  drain_initial();assert(con.connected&&con.edid_live&&d.modes_admitted&&d.external_mode_valid);
  assert(d.modes_generation==11&&!dcp_hdmi_attachment_generation(&d)&&rpcs==1);
  unsigned published=notices;
  pulse(&d,pulse_ns);event=NONE;
  assert(!apply(2)&&rpcs==1&&notices==published&&d.modes_generation==11);
 }
 /* Missing timing/GPIO support must not invent a physical epoch. */
 init(true);hdmi.timed_edges=false;
 assert(!dcp_hdmi_record_hpd_edge(&d,0,NSEC_PER_SEC));
 assert(!dcp_hdmi_record_hpd_edge(&d,1,2*NSEC_PER_SEC));
 assert(!dcp_hdmi_attachment_generation(&d));
 d.hdmi=NULL;assert(!dcp_hdmi_attachment_generation(&d));
 /* A real GPIO unplug/replug during A413 revokes A, even if final D575 is high. */
 init(true);dcp_external_hotplug(&d,true);event=PHYSICAL;drain_initial();
 assert(!con.connected&&!con.edid_live&&!metadata_live&&!d.external_mode_valid&&!d.mode_state.valid);
 assert(d.external_fw_connected&&!d.modes_admitted&&d.modes_generation==12&&rpcs==1);
 event=NONE;dcp_external_queue_initial_mode(&d);drain_initial();assert(rpcs==1);
 /* A new catalog B admits new IDs on the still-powered fixed pipe. */
 modes[2].timing_mode_id=346;d.modes_admitted=true;
 dcp_external_queue_initial_mode(&d);drain_initial();
 assert(con.connected&&d.external_mode.timing_mode_id==346&&powers==1&&rpcs==2);
 /* B's full catalog can arrive inside A413 with no later callback. */
 init(true);dcp_external_hotplug(&d,true);event=PHYSICAL_CATALOG;drain_initial();
 assert(!con.connected&&!d.external_mode_valid&&d.modes_admitted&&rpcs==1);
 assert(d.external_modeset_work.pending&&d.external_initial_request>d.external_initial_attempt);
 event=NONE;drain_initial();assert(con.connected&&rpcs==2&&powers==1);
 assert(d.external_mode.timing_mode_id==346&&!d.external_modeset_work.pending);
 /* The same-epoch catalog producer inside a permanently failed A413 is suppressed. */
 init(true);dcp_external_hotplug(&d,true);event=REQUEUE;reply=6;drain_initial();
 assert(!d.external_modeset_work.pending&&d.external_initial_request==d.external_initial_attempt&&rpcs==1);
 dcp_external_modeset_work(&d.external_modeset_work);assert(rpcs==1);
 reply=0;event=NONE;dcp_external_queue_initial_mode(&d);drain_initial();assert(con.connected&&rpcs==2);
 /* An edge before a transaction also forbids a cached A pair, without D575. */
 pulse(&d,2*NSEC_PER_MSEC+1);
 assert(apply(2)==-ESTALE&&!d.modes_admitted&&!d.external_mode_valid&&rpcs==2);
 /* First A413 ends low: replug/catalog may finish the powered, paused pipe. */
 init(true);dcp_external_hotplug(&d,true);event=LOW;drain_initial();
 assert(d.external_running&&!d.external_mode_valid&&d.iomfb.presentation_paused&&!con.connected);
 assert(rpcs==1&&powers==1&&!d.modes_admitted);
 event=NONE;dcp_external_hotplug(&d,true);drain_initial();assert(rpcs==1);
 d.modes_admitted=true;dcp_external_queue_initial_mode(&d);drain_initial();
 assert(con.connected&&d.external_mode_valid&&rpcs==2&&powers==1);
 /* The failed A413's own producers cannot create an infinite retry chain. */
 init(true);dcp_external_hotplug(&d,true);event=REQUEUE;reply=6;drain_initial();
 assert(!d.external_modeset_work.pending&&!con.connected&&rpcs==1);
 dcp_external_modeset_work(&d.external_modeset_work);assert(rpcs==1);
 /* Max area and the parser-derived DRM preference are not native evidence. */
 init(false);modes[2].firmware_preferred=false;modes[3].mode.clock=237600;
 assert(dcp_external_initial_mode(&d,&s,&g)==-ENODATA);
 dcp_external_hotplug(&d,true);drain_initial();assert(!rpcs&&!powers);
 struct dcp_timing_id pref={46,2560,1440};
 d.preferred_timings_v27=&pref;d.nr_preferred_timings_v27=1;
 dcp_external_queue_initial_mode(&d);drain_initial();
 assert(con.connected&&d.external_mode.timing_mode_id==46&&d.mode.timing_mode_id!=200);
 /* A mismatched preferred receipt must not lend native identity. */
 d.preferred_timings_v27->width=1920;
 assert(dcp_external_initial_mode(&d,&s,&g)==-ENODATA);
 d.preferred_timings_v27->width=2560;
 dcp_modes_begin_attachment(&d);assert(!d.nr_preferred_timings_v27);
 puts("PASS FW27 initial mode, native clock, cache/status, transient/final HPD, detach/catalog/suspend fences, stale stop and replay");
}
'''
    return prefix + production + tests


def catalog_fixture(root, catalog_only=False):
    """Real catalog parser and publisher, shared by preference and receipt tests."""
    f = rate.parser_fixture(root)
    apple = root / 'drivers/gpu/drm/apple'
    iomfb = (apple / 'iomfb.c').read_text()
    parser = (apple / 'parser.c').read_text()
    prefix = f['prefix'] + f['epoch'] + f['metadata'] + f['header'] + r'''
typedef u64 atomic64_t;
static void atomic64_inc(atomic64_t *p) { ++*p; }
static u64 atomic64_read(atomic64_t *p) { return *p; }
''' + hdmi_state() + r'''
#define READ_ONCE(x) (x)
#define guard(k) fake_guard
#define fake_guard(p) ((void)(p))
#define scoped_guard(k,p) for(int once=((void)(p),1);once;once=0)
#define spin_lock_irqsave(p,f) ((void)(p),(f)=0)
#define spin_unlock_irqrestore(p,f) ((void)(p),(void)(f))
#define lockdep_assert_held(p) ((void)(p))
#define DCP_FIRMWARE_V_27_0 27
#define DCP_IOMFB_METHODS_H17P_EXTERNAL 2
struct dcp_timing_id { u32 id,width,height; };
struct apple_connector { void *dcp; bool connected,edid_invalidation_pending; };
struct drm_crtc_state { struct drm_display_mode mode; };
struct apple_dcp {
 int modes_lock,hdmi_presence_lock,fw_compat,hpd_mutex;
 struct { int iomfb_method_profile; } hw;
 struct dcp_hdmi *hdmi;
 struct { bool changing; } mode_state;
 void *active_typec_route,*fixed_phy,*dev;
 bool quiescing,placeholder_retried,typec_cable_connected,external_cable;
 u64 typec_generation;
 struct { int lock; u64 epoch; bool enabled; void *service; } dcpavserv;
 int width_mm,height_mm,notch_height,fixed_connector_type,external_held;
 struct apple_connector *connector;
 struct dcp_display_mode *modes;
 unsigned nr_modes,nr_preferred_timings_v27;
 u64 modes_generation,modes_hdmi_generation;
 bool modes_admitted,modes_provisional,full_catalog_v27;
 struct dcp_timing_id *preferred_timings_v27;
 struct { int lock; struct dcp_metadata_ticket catalog_ticket;
  struct dcp_metadata_epoch epoch;
  bool hpd_seen,attributes_valid,timing_valid;
  u64 catalog_connection_sequence; } metadata;
 struct { u64 catalog_sequence,connection_sequence,sequence;
  struct { bool valid; } rate,lanes; } rate_proof;
 struct { bool payload_complete,automatic_service_pending,contiguous;
  size_t length,received; void *data; u64 modes_generation;
  struct dcp_metadata_ticket mode_ticket,metadata_ticket;
  u64 av_service_epoch,automatic_connection_sequence; } chunks;
};
#define DCP_FIRMWARE_V_26_6_DESKTOP 26
#define DRM_MODE_CONNECTOR_eDP 14
#define to_platform_device(d) (d)
#define atomic_read(p) (*(p))
#define mutex_lock(p) ((void)(p))
#define mutex_unlock(p) ((void)(p))
static void apple_connector_invalidate_edid(struct apple_connector *c)
{ if(c)c->edid_invalidation_pending=true; }
static u32 metadata_id;
static unsigned notifications;
static void dcp_firmware_metadata_timing(struct apple_dcp *d) {
 metadata_id=0;
 for(unsigned i=0;i<d->nr_modes;i++)
  if(d->modes[i].mode.type&DRM_MODE_TYPE_PREFERRED)metadata_id=d->modes[i].timing_mode_id;
}
static bool dcp_firmware_metadata_enabled(struct apple_dcp *d) { return d->hw.iomfb_method_profile==2; }
static void dcp_180hz_catalog_changed(struct apple_dcp *d) { (void)d; }
static void dcp_queue_hotplug(struct apple_connector *c) { assert(c->connected);notifications++; }
static unsigned requests;
static void dcp_external_queue_initial_mode(struct apple_dcp *d) { (void)d;requests++; }
'''
    production = hdmi_helpers(root) + '\n' + f['parser'] + '\n' + block(parser, 'int dcp_parse_timing_ids(')
    for marker in ('static void dcp_modes_invalidate_locked(', 'static bool dcp_modes_check_hdmi_locked(',
                   'bool dcp_modes_for_connector(', 'u64 dcp_modes_transfer_begin(',
                   'void dcp_modes_begin_attachment(', 'static bool dcp_external_native_mode(',
                   'static const struct dcp_display_mode *dcp_external_native_geometry(',
                   'static bool dcp_modes_retag_preferred_v27(', 'int dcp_modes_replace(',
                   'int dcp_modes_set_preferred_v27(', 'int dcp_external_initial_mode('):
        production += '\n' + block(iomfb, marker)
    native = copy.deepcopy(f['timing_fixture'][0])
    native['ID'] = 46
    native.pop('IsPreferred')
    native['HorizontalAttributes'].update(Active=2560, Total=2720)
    native['VerticalAttributes'].update(Active=1440, Total=1481)
    native['ColorModes'][0].update(ID=109, Depth=8)
    fast = copy.deepcopy(native)
    fast['ID'] = 100
    fast['VerticalAttributes']['PreciseSyncRate'] = 144 * 65536
    blanking = copy.deepcopy(native)
    blanking['ID'] = 101
    blanking['HorizontalAttributes']['Total'] = 4000
    blanking['VerticalAttributes']['Total'] = 2500
    uhd = copy.deepcopy(native)
    uhd['ID'] = 200
    uhd['HorizontalAttributes'].update(Active=3840, Total=4400)
    uhd['VerticalAttributes'].update(Active=2160, Total=2250, PreciseSyncRate=24*65536)
    pref = {k: v for k, v in native.items() if k in ('ID', 'HorizontalAttributes', 'VerticalAttributes')}
    blobs = ''
    for name, value in [('catalog', [native, fast, uhd, blanking]), ('preferred', [pref]),
                        ('id_only', [{'ID': 46}]), ('unknown', [{'ID': 999}]),
                        ('wrong_size', [dict(pref, HorizontalAttributes=dict(pref['HorizontalAttributes'], Active=1920))])]:
        if catalog_only and name != 'catalog':
            continue
        raw = bytearray(struct.pack('<I', 0xd3))
        f['wire'](value, raw)
        blobs += f'static const u8 {name}[]={{' + ','.join(map(str, raw)) + '};\n'
    return prefix, production, blobs


def preferred(root):
    """Real serialized preferred timings, retained independently of the full catalog."""
    prefix, production, blobs = catalog_fixture(root)
    return prefix + production + blobs + r'''
static void publish(struct apple_dcp *d) {
 struct dcp_parse_ctx c;
 struct dcp_display_mode *old=d->modes;
 assert(!parse(catalog,sizeof(catalog),&c));
 assert(!dcp_modes_replace(d,&c,d->modes_generation));
 assert(d->nr_modes==4);d->full_catalog_v27=true;
 free(old);
}
static void preferred_id(struct apple_dcp *d,u32 id) {
 unsigned found=0;
 for(unsigned i=0;i<d->nr_modes;i++) {
  assert(!d->modes[i].firmware_preferred);
  if(d->modes[i].mode.type&DRM_MODE_TYPE_PREFERRED) {
   assert(d->modes[i].timing_mode_id==id);found++;
  }
 }
 assert(found==!!id);
 assert(metadata_id==id);
}
int main(void) {
 struct apple_connector con={};
 struct dcp_hdmi hdmi={.timed_edges=true};
 struct apple_dcp d={.fw_compat=27,.hw.iomfb_method_profile=2,.active_typec_route=&con,
  .connector=&con,.modes_generation=11,.dev=&d,.hdmi=&hdmi};
 con.dcp=&d;
 publish(&d);
 struct drm_crtc_state state={};u64 g;
 preferred_id(&d,0); /* The parser's 4K24 area fallback was cleared. */
 assert(dcp_external_initial_mode(&d,&state,&g)==-ENODATA);
 /* Full first: retag before initialization and derived metadata publication. */
 assert(!dcp_modes_set_preferred_v27(&d,preferred,sizeof(preferred),11));
 assert(requests==2&&d.nr_preferred_timings_v27==1&&d.full_catalog_v27);
 preferred_id(&d,100); /* Native 144 Hz wins, including over larger blanking. */
 assert(con.edid_invalidation_pending&&!notifications);
 assert(d.modes[3].mode.clock>d.modes[1].mode.clock);
 assert(!dcp_external_initial_mode(&d,&state,&g)&&g==11);
 assert(state.mode.hdisplay==2560&&state.mode.vdisplay==1440&&state.mode.clock<300000);
 assert(!dcp_modes_set_preferred_v27(&d,id_only,sizeof(id_only),11));
 preferred_id(&d,100);
 assert(!dcp_external_initial_mode(&d,&state,&g)&&state.mode.hdisplay==2560);
 /* A late change on an already-connected connector asks DRM to reprobe. */
 con.connected=true;con.edid_invalidation_pending=false;
 assert(!dcp_modes_set_preferred_v27(&d,unknown,sizeof(unknown),11));
 preferred_id(&d,0);
 assert(notifications==1&&con.edid_invalidation_pending);
 assert(dcp_external_initial_mode(&d,&state,&g)==-ENODATA);
 assert(!dcp_modes_set_preferred_v27(&d,wrong_size,sizeof(wrong_size),11));
 preferred_id(&d,0);
 assert(dcp_external_initial_mode(&d,&state,&g)==-ENODATA);
 /* Preferred first: actual full-catalog publication consumes the hint. */
 dcp_modes_begin_attachment(&d);con.connected=false;
 assert(d.modes_generation==12&&!d.modes_admitted);
 assert(!dcp_modes_set_preferred_v27(&d,preferred,sizeof(preferred),12));
 assert(dcp_external_initial_mode(&d,&state,&g)==-ENODATA);
 publish(&d);preferred_id(&d,100);
 assert(!dcp_external_initial_mode(&d,&state,&g)&&state.mode.hdisplay==2560);
 /* Unsafe/non-SDR timing variants never become the preferred DRM mode. */
 d.modes[1].automatic_rgb8_quirk=true;
 assert(!dcp_modes_set_preferred_v27(&d,preferred,sizeof(preferred),12));
 assert(!(d.modes[1].mode.type&DRM_MODE_TYPE_PREFERRED));
 d.modes[3].sdr_usable=false;
 assert(!dcp_modes_set_preferred_v27(&d,preferred,sizeof(preferred),12));
 preferred_id(&d,46);
 /* Retire hints with the sink and refuse an old-sink receipt during A413. */
 d.active_typec_route=NULL;d.fixed_phy=&d;d.mode_state.changing=true;
 assert(!dcp_hdmi_record_hpd_edge(&d,0,NSEC_PER_SEC));
 assert(!dcp_hdmi_record_hpd_edge(&d,1,NSEC_PER_SEC+2*NSEC_PER_MSEC+1));
 dcp_modes_begin_attachment(&d);
 assert(d.modes_generation==13&&!d.modes_admitted&&!d.nr_preferred_timings_v27);
 assert(dcp_modes_set_preferred_v27(&d,preferred,sizeof(preferred),12)==-ESTALE);
 d.modes_admitted=true;
 assert(dcp_external_initial_mode(&d,&state,&g)==-ENODATA);
 free(d.preferred_timings_v27);free(d.modes);
 puts("PASS FW27 real preferred/full publication orders, native 144 Hz DRM/60 Hz bootstrap, blanking/SDR filters and stale receipts");
}
'''


def catalog_ready(root):
    """Transfer-start tickets through the real parser/publisher to FW27 readiness."""
    prefix, production, blobs = catalog_fixture(root, catalog_only=True)
    apple = root / 'drivers/gpu/drm/apple'
    dcp = (apple / 'dcp.c').read_text()
    template = (apple / 'iomfb_template.c').read_text()
    readiness = (apple / 'dcp-180hz.c').read_text()
    prefix += r'''
#define DCP_FW_VER 27
#define DCP_FW_H17(v) 1
#define WRITE_ONCE(p,v) ((p)=(v))
#define SZ_1M (1024*1024)
#define SZ_16M (16*SZ_1M)
#define kzalloc(n,f) calloc(1,n)
#define dev_warn(...) ((void)0)
struct dcp_set_dcpav_prop_chunk_req { char data[0x1000]; u32 offset,length; };
static bool dcp_sink_metadata(struct apple_dcp *d) { (void)d;return false; }
static void dcp_desktop_metadata_new_sink(struct apple_dcp *d) { (void)d;abort(); }
static void dcp_180hz_notify(struct apple_dcp *d) { (void)d; }
/* FW27 readiness must never fall through to unqualified EDID/exception checks. */
static bool dcpavserv_edid_current(struct apple_dcp *d,u64 g) { (void)d;(void)g;abort(); }
static bool dcpavserv_edid_service_current(struct apple_dcp *d,u64 e) { (void)d;(void)e;abort(); }
static bool dcp_180hz_link_current(struct apple_dcp *d,const struct dcp_display_mode *m)
{ (void)d;(void)m;abort(); }
'''
    prefix += '\n' + block((apple / 'dcp-property.h').read_text(),
                            'static inline bool dcp_property_size_valid(')
    for marker in ('void dcp_firmware_metadata_connection(',
                   'static void dcp_firmware_metadata_present_locked(',
                   'void dcp_firmware_metadata_hotplug(', 'void dcp_firmware_metadata_begin('):
        production += '\n' + block(dcp, marker)
    for marker in ('static u8 dcpep_cb_prop_start(', 'static u8 dcpep_cb_prop_chunk('):
        production += '\n' + block(template, marker)
    for marker in ('static bool dcp_catalog_current(', 'bool dcp_external_modes_ready('):
        production += '\n' + block(readiness, marker)
    f = rate.policy(root)
    unsafe = bytearray(struct.pack('<I', 0xd3))
    f['namespace']['wire'](f['fixture'], unsafe)
    blobs += 'static const u8 unsafe_catalog[]={' + ','.join(map(str, unsafe)) + '};\n'
    return prefix + production + blobs + r'''
static bool ready(struct apple_dcp *d) { return dcp_external_modes_ready(d); }
static void start(struct apple_dcp *d,const u8 *data,size_t size) {
 u32 length=size;
 assert(dcpep_cb_prop_start(d,&length));
 struct dcp_set_dcpav_prop_chunk_req chunk={.length=size};
 assert(size<=sizeof(chunk.data));memcpy(chunk.data,data,size);
 assert(dcpep_cb_prop_chunk(d,&chunk));
}
static int finish(struct apple_dcp *d) {
 struct dcp_parse_ctx ctx;
 struct dcp_display_mode *old=d->modes;
 /* The completion gate used by dcpep_process_chunks(), with the real decoder. */
 d->chunks.payload_complete=d->chunks.contiguous&&
  dcp_property_complete(d->chunks.data,d->chunks.received,d->chunks.length);
 assert(!parse(d->chunks.data,d->chunks.length,&ctx));
 int ret=dcp_modes_replace(d,&ctx,d->chunks.modes_generation);
 if(d->fw_compat==27) {
  assert(!ctx.automatic_1080p180_rgb8);
  for(unsigned i=0;i<d->nr_modes;i++)assert(!d->modes[i].automatic_rgb8_quirk);
 }
 free(d->chunks.data);d->chunks.data=NULL;
 if(d->modes!=old)free(old);
 return ret;
}
static void publish(struct apple_dcp *d) {
 start(d,catalog,sizeof(catalog));assert(!finish(d));
}
int main(void) {
 for(unsigned fixed=0;fixed<2;fixed++) {
  struct apple_dcp d={.fw_compat=27,.hw.iomfb_method_profile=2,
   .typec_generation=7,.typec_cable_connected=!fixed,.external_cable=true};
  d.active_typec_route=fixed?NULL:&d;
  /* Use the real enable policy for metadata producers. */
  dcp_firmware_metadata_connection(&d,7,true);
  assert(!ready(&d));
  d.dcpavserv.epoch=13;publish(&d);
  assert(d.metadata.catalog_ticket.associated&&!ready(&d));
  assert(d.chunks.av_service_epoch==13&&d.chunks.automatic_service_pending);
  dcp_firmware_metadata_hotplug(&d,true);assert(ready(&d));
  assert(!d.dcpavserv.enabled); /* No EDID RPC or rate exception proof. */
  /* Short, noncontiguous, overlong and malformed replacements prove nothing. */
  for(unsigned bad=0;bad<4;bad++) {
   start(&d,catalog,sizeof(catalog));
   if(bad==0)d.chunks.received--;
   if(bad==1)d.chunks.contiguous=false;
   if(bad==2)d.chunks.length+=2;
   if(bad==3)((u8 *)d.chunks.data)[7]=0x1f;
   assert(finish(&d)<=0);assert(!ready(&d));
   publish(&d);assert(ready(&d));
  }
  /* The special 180 Hz RGB element remains forbidden on FW27. */
  start(&d,unsafe_catalog,sizeof(unsafe_catalog));assert(!finish(&d));
  assert(!d.nr_modes&&!ready(&d));publish(&d);assert(ready(&d));
  /* Publication retains receipt-start cable and connection provenance. */
  start(&d,catalog,sizeof(catalog));
  u64 old_generation=d.chunks.mode_ticket.generation;
  u64 old_sequence=d.chunks.automatic_connection_sequence;
  d.typec_cable_connected=false;d.typec_generation++;
  dcp_firmware_metadata_connection(&d,d.typec_generation,false);
  d.typec_cable_connected=!fixed;d.typec_generation++;
  dcp_firmware_metadata_connection(&d,d.typec_generation,true);
  dcp_firmware_metadata_hotplug(&d,true);assert(!ready(&d));
  assert(!finish(&d));assert(!ready(&d));
  assert(d.metadata.catalog_ticket.generation==old_generation);
  assert(d.metadata.catalog_connection_sequence==old_sequence);
  publish(&d);assert(ready(&d));
  /* A same-generation downstream loss still revokes the old receipt. */
  start(&d,catalog,sizeof(catalog));
  dcp_firmware_metadata_hotplug(&d,false);
  dcp_firmware_metadata_hotplug(&d,true);
  assert(!finish(&d)&&!ready(&d));publish(&d);assert(ready(&d));
  /* A new attachment rejects an old transfer at the mode-generation fence. */
  start(&d,catalog,sizeof(catalog));
  dcp_modes_begin_attachment(&d);
  assert(finish(&d)==-ESTALE);
  /* A pre-attachment transfer must not associate on a later announcement. */
  d.typec_cable_connected=false;d.external_cable=false;
  start(&d,catalog,sizeof(catalog));assert(!d.chunks.mode_ticket.associated);
  d.typec_cable_connected=!fixed;d.external_cable=true;
  dcp_firmware_metadata_hotplug(&d,true);
  assert(!finish(&d)&&!ready(&d));publish(&d);assert(ready(&d));
  free(d.modes);
 }
 /* Older firmware keeps its qualified exception and immutable service ticket. */
 struct apple_dcp legacy={.fw_compat=14,.hw.iomfb_method_profile=2,
  .typec_generation=9,.typec_cable_connected=true};
 dcp_firmware_metadata_connection(&legacy,9,true);
 legacy.dcpavserv.epoch=23;
 start(&legacy,unsafe_catalog,sizeof(unsafe_catalog));legacy.dcpavserv.epoch++;
 assert(!finish(&legacy)&&legacy.nr_modes==1);
 const struct dcp_display_mode *m=&legacy.modes[0];
 assert(m->automatic_rgb8_quirk&&m->unsafe_rgb8_ticket.associated);
 assert(m->unsafe_rgb8_ticket.generation==9&&m->unsafe_rgb8_service_epoch==23);
 assert(m->automatic_service_pending);
 assert(m->automatic_connection_sequence==legacy.rate_proof.connection_sequence);
 assert(m->automatic_catalog_sequence==legacy.rate_proof.catalog_sequence);
 free(legacy.modes);
 puts("PASS FW27 real receipt/publisher/readiness: Type-C/fixed, complete, incomplete/malformed, replug, late old receipts and no RGB8 exception; legacy exception tickets retained");
}
'''


def mutations(unit, output):
    """A compile failure is never a kill; every mutation must hit an assertion."""
    enable = block(unit, 'VISIBLE_IF_KUNIT void dcp_enable_typec_work(')
    cases = [
        ('missing-probe-disable', 'disable_work(&dcp->external_modeset_work);', ''),
        ('missing-bind-enable', 'enable_work(&dcp->external_modeset_work);', ''),
        ('missing-quiesce-disable', 'disable_work_sync(&dcp->external_modeset_work);', ''),
        ('missing-enable-replay', enable, enable.replace('dcp_external_queue_initial_mode(dcp);', '')),
        ('pending-not-cancelled', 'bool pending=w->pending;w->pending=false;', 'bool pending=w->pending;'),
        ('ignore-physical-epoch', 'if (generation == dcp->modes_hdmi_generation)', 'if (true)'),
        ('reject-powered-initial', '(dcp->external_mode_valid && READ_ONCE(dcp->mode_state.valid))', 'dcp->external_running'),
        ('a413-self-requeue', '&&\n\t    !READ_ONCE(dcp->mode_state.changing))', ')'),
        ('lose-new-catalog-replay', 'if (ret == -ESTALE && connected && retry_initial)',
         'if (false && ret == -ESTALE && connected && retry_initial)'),
        ('largest-area-as-native', ' ||\n\t\t    !dcp_external_native_mode(dcp, m)', ''),
    ]
    results = []
    for name, old, new in cases:
        assert unit.count(old) == 1, (name, unit.count(old))
        try:
            run_unit(name, unit.replace(old, new, 1), output, extra=('-Wno-unused-function',))
        except subprocess.CalledProcessError as error:
            log = output / name / 'case-0.log'
            assert log.exists(), f'{name}: compile failure is not a mutation kill'
            detail = log.read_text()
            assert error.returncode == -6 and 'Assertion failed:' in detail, (name, error.returncode, detail)
            results.append({'name': name, 'returncode': error.returncode, 'assertion': detail.strip()})
            print(f'PASS mutation caught: {name}')
        else:
            raise AssertionError(f'mutation survived: {name}')
    (output / 'mutations.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[6])
    parser.add_argument('--output', type=Path)
    parser.add_argument('--mutations', action='store_true')
    args = parser.parse_args()
    output = args.output or Path(tempfile.mkdtemp(prefix='apple-external-mode-'))
    unit = transaction(args.source)
    run_unit('external-mode', unit, output, extra=('-Wno-unused-function',))
    if args.mutations:
        mutations(unit, output)
    print(f'PASS: {output}')
