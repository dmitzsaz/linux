#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Production M5 profile, property receipts, readiness and derived EDID on host.

Locks/work queues and the A413 transport are synchronous mocks. Match data,
probe's metadata branch, property callbacks, decoders, publishers, metadata
lifecycle, EDID builder, HPD handling and modeset settlement are extracted.
No kernel, device, firmware, GPIO or hardware protocol is exercised.
"""
import argparse
import copy
import json
from pathlib import Path
import struct
import subprocess
import tempfile

from common import block, run_unit
import external_mode
import rate


def fixture(root):
    apple = root / 'drivers/gpu/drm/apple'
    dcp = (apple / 'dcp.c').read_text()
    internal = (apple / 'dcp-internal.h').read_text()
    template = (apple / 'iomfb_template.c').read_text()
    iomfb = (apple / 'iomfb.c').read_text()
    readiness = (apple / 'dcp-180hz.c').read_text()
    f = rate.parser_fixture(root)
    prefix, production, _ = external_mode.catalog_fixture(root)
    blobs = ''
    prefix = prefix.replace('static u32 metadata_id;', '')
    # Use the full production hardware declaration and initializers, including
    # flags omitted by older fixtures' handwritten H17P profiles.
    hw = block(internal, 'struct apple_dcp_hw_data {') + ';\n'
    hw = hw.replace('enum dcp_firmware_version', 'int').replace('enum dcp_iomfb_method_profile', 'int')
    proof = (apple / 'dcp-180hz.h').read_text()
    declarations = ''.join(block(proof, marker) + ';\n' for marker in
                           ('struct dcp_180hz_token {', 'struct dcp_180hz_fact {', 'struct dcp_180hz_proof {'))
    declarations += r'''
struct apple_epic { struct apple_dcp *dcp; unsigned endpoint; bool wire_v2; };
struct apple_epic_service { struct apple_epic *ep; void *cookie; bool enabled; };
struct dptx_port { bool connected,enabled,closed; void *atcphy; struct apple_epic_service *service; };
'''
    prefix = prefix.replace('struct apple_dcp {', hw + declarations + 'struct apple_dcp {', 1)
    start = prefix.index(' struct { u64 catalog_sequence,connection_sequence,sequence;')
    end = prefix.index(' } rate_proof;', start) + len(' } rate_proof;')
    prefix = prefix[:start] + ' struct dcp_180hz_proof rate_proof;' + prefix[end:]
    prefix = prefix.replace('struct { int iomfb_method_profile; } hw;', 'struct apple_dcp_hw_data hw;')
    prefix = prefix.replace('struct { bool changing; } mode_state;',
                            'struct dcp_mode_state mode_state;')
    state = (apple / 'iomfb-state.h').read_text().replace('#include <linux/spinlock.h>', '')
    prefix = prefix.replace('struct apple_dcp {', '#define BIT(n) (1U << (n))\ntypedef int spinlock_t;\n' + state + '\nstruct apple_dcp {', 1)
    prefix = prefix.replace('struct apple_connector { void *dcp; bool connected,edid_invalidation_pending; };',
        'struct work_struct { unsigned scheduled; };\n'
        'struct apple_connector { void *dcp; bool connected,edid_invalidation_pending,edid_live; '
        'struct work_struct hotplug_wq,bl_sync_wq; };')
    prefix = prefix.replace('bool quiescing,placeholder_retried,typec_cable_connected,external_cable,external_physically_absent;', r'''
 bool quiescing,placeholder_retried,typec_cable_connected,external_cable;
 int external_lock;
 bool external_physically_absent,crashed,typec_work_enabled,external_fw_connected;
 bool external_hotplug_pending,external_hotplug_connected,external_mode_valid,fixed_link_held;
 bool external_error,external_power_attempted,external_stopped,typec_crtc_off,ext_backlight;
 u64 external_generation,external_rx_generation,external_hotplug_generation;
 u64 external_stop_generation,external_crtc_generation,external_mode_generation;
 u64 external_mode_catalog_generation,dimensions_generation;
 struct { u32 color_mode_id,timing_mode_id; } mode,external_mode;
 struct { int lock; bool presentation_paused; } iomfb;
 struct dptx_port dptxport[1];
 void *phy;bool dptx_tunnel;
 struct { bool holding,stopped; } announce;
 struct work_struct external_stop_work,dimensions_wq;
 struct { int width_mm,height_mm; } panel;
''')
    prefix = prefix.replace('bool hpd_seen,attributes_valid,timing_valid;', r'''
  bool hpd_seen,attributes_valid,timing_valid;
  struct dcp_display_metadata attributes;
  struct drm_display_mode preferred;
  struct dcp_metadata_ticket attributes_ticket,timing_ticket;
  u64 snapshot_sequence;
''')
    for marker in ('static void dcp_firmware_metadata_timing(',
                   'static bool dcp_firmware_metadata_enabled('):
        prefix = prefix.replace(block(prefix, marker), '')
    prefix += r'''
#define DCP_FW_VER 27
#define DCP_FW_H17(v) 1
#define WRITE_ONCE(p,v) ((p)=(v))
#define SZ_1M (1024*1024)
#define SZ_16M (16*SZ_1M)
#define kzalloc(n,f) calloc(1,n)
#define dev_warn(...) ((void)0)
#define dev_info(...) ((void)0)
#define dev_dbg(...) ((void)0)
#define DCP_IOMFB_METHODS_DEFAULT 0
#define DPTX_ENDPOINT_V2 0x2c
#define U64_MAX UINT64_MAX
#define BIT(n) (1U << (n))
struct dcp_set_dcpav_prop_chunk_req { char data[0x1000]; u32 offset,length; };
struct dcp_set_dcpav_prop_end_req { char key[64]; };
static void schedule_work(struct work_struct *w) { w->scheduled++; }
static bool dcp_is_external_h17p(struct apple_dcp *d) { return d->hw.iomfb_method_profile==2; }
static bool dcp_has_panel(struct apple_dcp *d) { (void)d;return false; }
static bool dcp_fw_is_14_7(struct apple_dcp *d) { (void)d;return false; }
static void dcp_handle_hotplug_actions(struct apple_dcp *d,unsigned action) { (void)d;(void)action; }
static void dcp_180hz_notify(struct apple_dcp *d) { (void)d; }
static void apple_connector_edid_set_live(struct apple_connector *c,bool live) { if(c)c->edid_live=live; }
static void dcp_panel_edid_receive(struct apple_dcp *d,const void *p,size_t n) { (void)d;(void)p;(void)n; }
static void dcp_panel_metadata_replace(struct apple_dcp *d,const struct dcp_display_metadata *m) { (void)d;(void)m;abort(); }
#define dcp_trace_panel_property(...) ((void)0)
static void dcp_connector_update_dict(struct apple_connector *c,const char *key,struct dcp_chunks *chunks);
static bool dcpavserv_edid_current(struct apple_dcp *d,u64 g) { (void)d;(void)g;return false; }
static bool dcpavserv_edid_service_current(struct apple_dcp *d,u64 e) { (void)d;(void)e;return false; }
static bool dcp_is_usb4_output(struct apple_dcp *d) { (void)d;return false; }
static bool dcp_external_hotplug(struct apple_dcp *,bool);
static int iomfb_modeset_v27(struct apple_dcp *,struct drm_crtc_state *);
static void dcp_firmware_metadata_timing(struct apple_dcp *);
static bool dcp_firmware_metadata_enabled(struct apple_dcp *);
'''
    # The debugfs owner only takes/frees the transfer allocation. Keep the
    # callbacks' actual END ownership and cleanup rather than a fabricated END.
    chunks = block(internal, 'struct dcp_chunks {') + ';\n'
    start = prefix.index(' struct { bool payload_complete,automatic_service_pending,contiguous;')
    end = prefix.index(' } chunks;', start) + len(' } chunks;')
    prefix = prefix[:start] + ' struct dcp_chunks chunks;' + prefix[end:]
    prefix = prefix.replace('struct apple_dcp {', chunks + '\nstruct apple_dcp {', 1)
    prefix = prefix.replace('static void dcp_connector_update_dict(struct apple_connector *c,const char *key,struct dcp_chunks *chunks);',
        'static void dcp_connector_update_dict(struct apple_connector *c,const char *key,struct dcp_chunks *chunks) '
        '{ (void)c;(void)key;free(chunks->data); }')
    prefix += '\n' + f['edid_header'] + f['stubs']
    # Snapshot-race injection is at the allocation boundary only. The builder
    # and its inputs remain real, and current() must reject the finished EDID.
    prefix = prefix.replace('static const struct drm_edid *drm_edid_alloc(',
        'static void (*allocation_event)(void);\nstatic const struct drm_edid *drm_edid_alloc(', 1)
    prefix = prefix.replace('    result->size = size;\n    return result;',
        '    result->size = size;\n    if(allocation_event) { void (*event)(void)=allocation_event;allocation_event=NULL;event(); }\n    return result;', 1)
    prefix += '\n' + f['builder']
    prefix += '\n' + block((apple / 'dcp-180hz-policy.h').read_text(), 'static inline bool dcp_180hz_payload(')
    # Extract the exact hardware initializer and exact metadata init branch
    # from probe. of_device_get_match_data() is the only replaced boundary.
    profiles = '\n'.join(block(dcp, 'static const struct apple_dcp_hw_data ' + name + ' =') + ';'
                         for name in ('apple_dcp_hw_t8142_external', 'apple_dcp_hw_t6041_dcpext'))
    probe = block(dcp, 'static int dcp_platform_probe_pipe(')
    start = probe.index('dcp->hw = *(struct apple_dcp_hw_data *)of_device_get_match_data(dev);')
    end = probe.index('\n\tif (dcp->hw.iomfb_method_profile ==', start)
    init = probe[start:end]
    init = init.replace('of_device_get_match_data(dev)', 'match_data')
    prefix += '\n' + profiles + '\n'
    # Definitions are placed before the publisher, which calls metadata timing.
    lifecycle = ''
    names = ('static bool dcp_sink_metadata(', 'bool dcp_firmware_metadata_enabled(',
             'static void dcp_desktop_metadata_new_sink(', 'void dcp_firmware_metadata_connection(',
             'static void dcp_firmware_metadata_present_locked(', 'static void dcp_announce_kick(',
             'void dcp_firmware_metadata_announced(', 'void dcp_firmware_metadata_hotplug(',
             'static bool dcp_firmware_metadata_recoverable(', 'static bool dcp_firmware_metadata_physical(',
             'static bool dcp_firmware_metadata_matches(', 'static bool dcp_firmware_metadata_valid(',
             'void dcp_firmware_metadata_begin(', 'static void dcp_firmware_metadata_notify(',
             'static bool dcp_firmware_metadata_complete(', 'void dcp_firmware_metadata_attributes(',
             'void dcp_firmware_metadata_timing(', 'bool dcp_firmware_metadata_current(',
             'const struct drm_edid *dcp_firmware_edid(')
    for marker in names:
        if marker in dcp:  # Also compile the real uncorrected baseline.
            lifecycle += '\n' + block(dcp, marker)
    # HDMI helpers must precede receipt capture.
    hdmi = external_mode.hdmi_helpers(root)
    production = production.replace(hdmi, '', 1)
    link = '\n'.join(block(readiness, marker) for marker in
                     ('static bool dcp_180hz_direct_port(', 'void dcp_180hz_invalidate(',
                      'struct dcp_180hz_token dcp_180hz_config_begin(',
                      'static bool dcp_180hz_timing_epoch(', 'void dcp_180hz_config_complete(',
                      'static bool dcp_180hz_fact_current(', 'static bool dcp_180hz_link_current('))
    production = hdmi + '\n' + link + '\n' + lifecycle + '\n' + production
    production += '\nstatic void probe_metadata(struct apple_dcp *dcp,const struct apple_dcp_hw_data *match_data) {\n'
    production += 'int fw_compat=dcp->fw_compat;\n' + init + '\n}\n'
    production += '\n' + block(iomfb, 'int dcp_attributes_replace(')
    production = production.replace('modes = enumerate_modes(ctx, &nr_modes,',
        'assert(dcp->fw_compat != 27 || !ctx->automatic_1080p180_rgb8);\n\tmodes = enumerate_modes(ctx, &nr_modes,', 1)
    for marker in ('static u8 dcpep_cb_prop_start(', 'static u8 dcpep_cb_prop_chunk(',
                   'static bool dcpep_process_chunks(', 'static u8 dcpep_cb_prop_end('):
        production += '\n' + block(template, marker)
    for marker in ('static bool dcp_catalog_current(', 'bool dcp_external_modes_ready('):
        production += '\n' + block(readiness, marker)
    production += '\n' + block(dcp, 'bool dcp_external_hotplug(').replace(
        'bool dcp_external_hotplug(', 'static bool dcp_external_hotplug(', 1)
    production += '\n' + block(iomfb, 'int dcp_external_modeset(')
    prefix += '\n' + '\n'.join(block((apple / 'dcp-property.h').read_text(), marker)
        for marker in ('static inline bool dcp_property_size_valid(', 'static inline bool dcp_property_key_valid('))
    def blob(name, value):
        raw = bytearray(struct.pack('<I', 0xd3))
        f['wire'](value, raw)
        return 'static const u8 ' + name + '[]={' + ','.join(map(str, raw)) + '};\n'
    timings = [dict(copy.deepcopy(f['timing_fixture'][0]), ID=62+i) for i in range(12)]
    preferred = [{key: value for key, value in timings[0].items()
                  if key in ('ID', 'HorizontalAttributes', 'VerticalAttributes')}]
    blobs += blob('attributes', f['fixture']) + blob('timing', timings) + blob('empty', [])
    blobs += blob('preferred', preferred)
    unsafe = rate.policy(root)
    blobs += blob('unsafe_catalog', unsafe['fixture'])
    return prefix + production + blobs + TESTS


TESTS = r'''
static struct apple_dcp d;
static struct apple_connector con;
static struct dcp_hdmi hdmi;
static void cleanup(void) { free(d.modes);free(d.preferred_timings_v27);free(d.chunks.data); }
static void init(bool fixed,bool desktop) {
 cleanup();memset(&d,0,sizeof(d));memset(&con,0,sizeof(con));memset(&hdmi,0,sizeof(hdmi));
 d.fw_compat=desktop?26:27;d.connector=&con;d.dev=&d;con.dcp=&d;d.hdmi=&hdmi;hdmi.timed_edges=true;
 d.active_typec_route=fixed?NULL:&con;d.fixed_phy=fixed?&hdmi:NULL;
 d.typec_work_enabled=true;d.dptxport[0].connected=true;
 probe_metadata(&d,desktop?&apple_dcp_hw_t6041_dcpext:&apple_dcp_hw_t8142_external);
}
static void attach(void) {
 /* Route activation, followed by the physical attach's generation. */
 dcp_firmware_metadata_connection(&d,d.typec_generation,false);
 d.typec_generation++;d.typec_cable_connected=!!d.active_typec_route;d.external_cable=true;
 dcp_firmware_metadata_connection(&d,d.typec_generation,true);
 d.external_generation++;d.external_rx_generation=d.external_generation;
 d.external_physically_absent=false;
}
static void detach(void) {
 d.external_cable=false;d.typec_cable_connected=false;d.typec_generation++;
 dcp_firmware_metadata_connection(&d,d.typec_generation,false);
 dcp_modes_begin_attachment(&d);
}
static void start(const u8 *data,size_t size,unsigned bad) {
 u32 length=size+(bad==3?2:0);
 assert(dcpep_cb_prop_start(&d,&length));
 struct dcp_set_dcpav_prop_chunk_req chunk={};
 for(size_t offset=0;offset<size;) {
  chunk.offset=offset;chunk.length=min(sizeof(chunk.data),size-offset);
  if(bad==1&&offset+chunk.length==size)chunk.length--;
  if(!chunk.length)break;
  memcpy(chunk.data,data+offset,chunk.length);
  assert(dcpep_cb_prop_chunk(&d,&chunk));offset+=chunk.length;
  if(bad==1)break;
 }
 if(bad==2) { chunk.offset=0;chunk.length=1;chunk.data[0]=data[0];assert(dcpep_cb_prop_chunk(&d,&chunk)); }
 if(bad==4)((u8 *)d.chunks.data)[0]^=1;
}
static bool finish(const char *key) {
 struct dcp_display_mode *old=d.modes;
 struct dcp_set_dcpav_prop_end_req end={};strcpy(end.key,key);
 bool accepted=dcpep_cb_prop_end(&d,&end);
 assert(!d.chunks.data&&!d.chunks.length);
 if(old!=d.modes)free(old);
 if(d.fw_compat==27)for(unsigned i=0;i<d.nr_modes;i++)assert(!d.modes[i].automatic_rgb8_quirk);
 return accepted;
}
static bool send(const char *key,const u8 *data,size_t size) { start(data,size,0);return finish(key); }
static void publish_catalog(void) { assert(send("TimingElements",timing,sizeof(timing))); }
static void attrs(void) { assert(send("DisplayAttributes",attributes,sizeof(attributes))); }
static void pref(void) { assert(send("PreferredTimingElements",preferred,sizeof(preferred))); }
static bool ready(void) { return dcp_external_modes_ready(&d); }
static bool edid_ok(void) {
 u64 generation=0;const struct drm_edid *edid=dcp_firmware_edid(&d,&generation);
 if(IS_ERR(edid))return false;
 assert(drm_edid_valid(edid));assert(dcp_firmware_metadata_current(&d,generation));
 const u8 *raw=(const u8 *)&edid->raw;assert(raw[8]==0x04&&raw[9]==0x72);
 drm_edid_free(edid);return true;
}
static void hpd(bool connected) {
 if(!dcp_external_hotplug(&d,connected))dcp_firmware_metadata_hotplug(&d,connected);
}
static int iomfb_modeset_v27(struct apple_dcp *p,struct drm_crtc_state *state) {
 (void)state;assert(p==&d&&d.mode_state.changing);
 u64 sequence=d.rate_proof.connection_sequence;
 hpd(false);hpd(true);
 assert(d.rate_proof.connection_sequence==sequence);
 return 0; /* Synchronous successful A413 transport/completion boundary. */
}
static void modeset(void) {
 struct drm_crtc_state state={};
 assert(!dcp_external_modeset(&d,&state,d.modes_generation));
 assert(d.external_mode_valid&&d.mode_state.valid&&con.connected&&con.edid_live);
}
static void recover(void) { attrs();publish_catalog();hpd(true);assert(ready()&&edid_ok()); }
static void physical_edge(void) {
 assert(!dcp_hdmi_record_hpd_edge(&d,0,NSEC_PER_SEC));
 assert(!dcp_hdmi_record_hpd_edge(&d,1,NSEC_PER_SEC+2*NSEC_PER_MSEC+1));
}
static void lose_during_alloc(void) { hpd(false);hpd(true); }
static void replace_during_alloc(void) { hpd(false);hpd(true);recover(); }
static void hdmi_during_alloc(void) { physical_edge();recover(); }
int main(int argc,char **argv) {
 unsigned only=argc>1?strtoul(argv[1],NULL,10):0;
 assert(sizeof(timing)>2*0x1000);
 if(!only||only==1)for(unsigned fixed=0;fixed<2;fixed++)for(unsigned first=0;first<2;first++) {
  init(fixed,false);attach();
  if(first)hpd(true);
  /* Preferred-only input must not create a receipt or readiness. */
  pref();assert(!ready()&&!edid_ok());attrs();assert(!edid_ok());
  publish_catalog();assert(d.metadata.catalog_ticket.associated);
  assert(d.metadata.epoch.generation==d.typec_generation);
  assert(ready()==!!first);hpd(true);assert(ready()&&edid_ok());
  u64 sequence=d.rate_proof.connection_sequence;
  modeset();assert(ready()&&edid_ok()&&d.rate_proof.connection_sequence==sequence);
  pref();assert(ready()&&edid_ok()&&d.metadata.catalog_ticket.generation==d.typec_generation);
  /* Replug must restore real firmware-derived EDID, not just catalog readiness. */
  detach();attach();assert(!ready()&&!edid_ok());recover();
 }
 if(!only||only==2)for(unsigned fixed=0;fixed<2;fixed++) {
  init(fixed,false);attach();recover();
  for(unsigned attr=0;attr<2;attr++) {
   start(attr?attributes:timing,attr?sizeof(attributes):sizeof(timing),0);
   detach();attach();hpd(true);
   assert(!finish(attr?"DisplayAttributes":"TimingElements"));
   assert(!ready()&&!edid_ok());recover();
  }
  /* Test receipt provenance without the additional modes-generation fence. */
  start(attributes,sizeof(attributes),0);
  d.typec_cable_connected=false;d.external_cable=false;d.typec_generation++;
  dcp_firmware_metadata_connection(&d,d.typec_generation,false);
  attach();hpd(true);assert(finish("DisplayAttributes"));assert(!edid_ok());
  recover();
 }
 if(!only||only==3)for(unsigned fixed=0;fixed<2;fixed++)for(unsigned attr=0;attr<2;attr++) {
  init(fixed,false);attach();recover();
  start(attr?attributes:timing,attr?sizeof(attributes):sizeof(timing),0);
  u64 sequence=d.rate_proof.connection_sequence;
  /* Ordinary HPD, outside A413, fences both metadata and catalog receipts. */
  hpd(false);assert(d.rate_proof.connection_sequence>sequence);
  assert(!ready()&&!edid_ok());hpd(true);assert(!ready()&&!edid_ok());
  assert(finish(attr?"DisplayAttributes":"TimingElements"));assert(!ready()&&!edid_ok());
  if(attr) {
   assert(!d.metadata.attributes_valid);
   publish_catalog();assert(ready()&&!edid_ok());attrs();assert(edid_ok());
   hpd(false);hpd(true);
  }
  /* A preferred receipt cannot reassociate the retained full catalog. */
  attrs();pref();assert(!d.metadata.timing_valid);assert(!ready()&&!edid_ok());
  /* Fresh attrs/catalog before the HPD return must also recover. */
  hpd(false);attrs();publish_catalog();assert(!ready()&&!edid_ok());
  hpd(true);assert(ready()&&edid_ok());
 }
 if(!only||only==4) {
  init(false,false);attach();recover();
  for(unsigned bad=1;bad<=4;bad++) {
   start(timing,sizeof(timing),bad);finish("TimingElements");assert(!ready());
   publish_catalog();assert(ready()&&edid_ok());
  }
  assert(send("TimingElements",empty,sizeof(empty)));assert(!ready()&&!edid_ok());
  publish_catalog();assert(ready()&&edid_ok());
  /* A pre-attachment transfer is never associated by a later HPD high. */
  detach();start(timing,sizeof(timing),0);attach();hpd(true);
  assert(finish("TimingElements"));assert(!ready()&&!edid_ok());recover();
  /* Missing CHUNK and ENDs cannot create a catalog. */
  u32 length=sizeof(timing);assert(dcpep_cb_prop_start(&d,&length));
  finish("TimingElements");assert(!ready());publish_catalog();assert(ready());
  start(timing,sizeof(timing),0);hpd(false);hpd(true);assert(!ready());
  finish("TimingElements");assert(!ready());publish_catalog();assert(ready());
 }
 if(!only||only==5) {
  init(true,false);attach();recover();
  for(unsigned attr=0;attr<2;attr++) {
   start(attr?attributes:timing,attr?sizeof(attributes):sizeof(timing),0);
   physical_edge();assert(!edid_ok());
   assert(!finish(attr?"DisplayAttributes":"TimingElements"));assert(!edid_ok());recover();
  }
  /* A short converter IRQ pulse leaves the physical metadata receipt intact. */
  assert(!dcp_hdmi_record_hpd_edge(&d,0,NSEC_PER_SEC));
  assert(dcp_hdmi_record_hpd_edge(&d,1,NSEC_PER_SEC+2*NSEC_PER_MSEC));assert(edid_ok());
  for(unsigned event=0;event<3;event++) {
   allocation_event=event==0?lose_during_alloc:event==1?replace_during_alloc:hdmi_during_alloc;
   u64 generation;const struct drm_edid *edid=dcp_firmware_edid(&d,&generation);
   assert(IS_ERR(edid)&&PTR_ERR(edid)==-ESTALE);recover();
  }
 }
 if(!only||only==6) {
  init(false,false);attach();hpd(true);attrs();
  /* AV availability/configuration must not enable the legacy FW27 exception. */
  struct apple_epic ep={.dcp=&d,.endpoint=DPTX_ENDPOINT_V2,.wire_v2=true};
  struct apple_epic_service service={.ep=&ep,.cookie=&d.dptxport[0],.enabled=true};
  d.phy=&con;d.dptxport[0].atcphy=d.phy;d.dptxport[0].enabled=true;d.dptxport[0].service=&service;
  d.dcpavserv.enabled=true;d.dcpavserv.service=&service;d.dcpavserv.epoch=23;
  for(unsigned lanes=0;lanes<2;lanes++) {
   struct dcp_180hz_token token=dcp_180hz_config_begin(&service,lanes);
   assert(token.valid);dcp_180hz_config_complete(&service,lanes,token,lanes?2:0x1e,true);
  }
  assert(dcp_180hz_direct_port(&service));
  assert(dcp_180hz_fact_current(&d,&d.rate_proof.rate)&&dcp_180hz_fact_current(&d,&d.rate_proof.lanes));
  assert(send("TimingElements",unsafe_catalog,sizeof(unsafe_catalog)));
  assert(!d.nr_modes&&!ready()&&!edid_ok());publish_catalog();assert(ready()&&edid_ok());
  /* Production desktop match data and probe retain independent sink epochs. */
  init(true,true);assert(d.hw.sink_metadata_edid&&d.metadata.epoch.generation==1);
  attach();assert(d.metadata.epoch.generation==3&&d.typec_generation==1);
  attrs();publish_catalog();assert(edid_ok());
  u64 generation=d.metadata.epoch.generation;
  dcp_firmware_metadata_hotplug(&d,false);dcp_firmware_metadata_hotplug(&d,true);
  assert(d.metadata.epoch.generation==generation&&edid_ok());
  dcp_firmware_metadata_connection(&d,99,false);assert(!edid_ok());
  attrs();publish_catalog();assert(edid_ok()&&d.metadata.epoch.generation==generation+1);
 }
 cleanup();puts("PASS production M5 profile/probe, real property/EDID lifecycle, readiness, stale fences, A413 bounce and desktop control");
}
'''


def expect_assertion(name, unit, output, case):
    try:
        run_unit(name, unit, output, cases=((case,),), extra=('-Wno-unused-function',))
    except subprocess.CalledProcessError as error:
        log = output / name / 'case-0.log'
        assert log.exists(), f'{name}: compilation failure is not a negative-control kill'
        detail = log.read_text()
        assert error.returncode == -6 and ('Assertion failed:' in detail or 'Assertion ' in detail), (name, error.returncode, detail)
        return {'name': name, 'returncode': error.returncode, 'assertion': detail.strip()}
    raise AssertionError(f'negative control survived: {name}')


def baseline(root, ref, output):
    """Run the actual old source, then only its one-line profile correction."""
    with tempfile.TemporaryDirectory(prefix='dcp-metadata-baseline-') as directory:
        archive = subprocess.run(['git', '-C', str(root), 'archive', ref,
                                  'drivers/gpu/drm/apple', 'include/drm'],
                                 check=True, capture_output=True)
        subprocess.run(['tar', '-xf', '-', '-C', directory], input=archive.stdout, check=True)
        unit = fixture(Path(directory))
    results = [expect_assertion('actual-baseline', unit, output, '1')]
    profile = block(unit, 'static const struct apple_dcp_hw_data apple_dcp_hw_t8142_external =')
    assert '.sink_metadata_edid = true,' in profile
    corrected = profile.replace('.sink_metadata_edid = true,', '')
    results.append(expect_assertion('baseline-profile-only', unit.replace(profile, corrected, 1), output, '1'))
    (output / 'baseline.json').write_text(json.dumps({'ref': ref, 'results': results}, indent=2) + '\n')


def mutations(unit, output):
    """Compile every counterfactual; only a failed assertion counts as a kill."""
    cases = [
        ('profile-mismatch', '.adopt_live_session = true,\n};', '.adopt_live_session = true,\n\t.sink_metadata_edid = true,\n};', '1'),
        ('sticky-only', 'if (!dcp_firmware_metadata_recoverable(dcp))\n\t\treturn dcp_metadata_matches',
         'if (true)\n\t\treturn dcp_metadata_matches', '1'),
        ('no-metadata-connection-fence', 'ticket.connection_sequence == dcp->rate_proof.connection_sequence &&', '', '3'),
        ('no-metadata-hdmi-fence', 'ticket.hdmi_generation == dcp_hdmi_attachment_generation(dcp)', 'true', '5'),
        ('no-catalog-connection-fence', 'dcp->metadata.catalog_connection_sequence == dcp->rate_proof.connection_sequence', 'true', '3'),
        ('retag-preferred-timing', 'ticket = dcp->metadata.catalog_ticket;', 'ticket = dcp->chunks.metadata_ticket;', '3'),
        ('no-edid-snapshot-fence', 'generation == dcp->metadata.snapshot_sequence', 'true', '5'),
    ]
    results = []
    for name, old, new, case in cases:
        assert unit.count(old) == 1, (name, unit.count(old))
        results.append(expect_assertion(name, unit.replace(old, new, 1), output, case))
    (output / 'mutations.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[6])
    parser.add_argument('--output', type=Path)
    parser.add_argument('--mutations', action='store_true')
    parser.add_argument('--old-ref', help='compile and reject this baseline and its profile-only fix')
    args = parser.parse_args()
    output = args.output or Path(tempfile.mkdtemp(prefix='apple-dcp-metadata-'))
    unit = fixture(args.source)
    run_unit('metadata-lifecycle', unit, output, extra=('-Wno-unused-function',))
    if args.mutations:
        mutations(unit, output)
    if args.old_ref:
        baseline(args.source, args.old_ref, output)
    print(f'PASS: {output}')
