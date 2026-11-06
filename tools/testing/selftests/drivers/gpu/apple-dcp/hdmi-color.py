#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""FW27 fixed-HDMI SDR converter policy and A413, with synthetic catalogs only."""
import argparse
import json
from pathlib import Path
import re
import tempfile

from common import block, run_unit
import rate


def fixture(root):
    apple = root / 'drivers/gpu/drm/apple'
    f = rate.parser_fixture(root)
    # Execute the KUnit cases against the actual parser, with host allocations.
    tests = (apple / 'dcp-parser-test.c').read_text()
    tests = tests[tests.index('enum ycc_change {'):
                  tests.index('static struct kunit_case dcp_ycc_cases[]')]
    parser = f['parser'].split('/* Entries of the array at the cursor, which is not consumed. */')[0]
    shim = r'''
#define __GFP_NOWARN 0
#define ALIGN(n,a) round_up(n,a)
#define kzalloc(n,f) calloc(1,n)
#define READ_ONCE(x) (x)
#define WRITE_ONCE(x,y) ((x)=(y))
#ifndef EREMOTEIO
#define EREMOTEIO 121
#endif
static void put_unaligned_le32(u32 v,void *p) { memcpy(p,&v,4); }
static void put_unaligned_le64(u64 v,void *p) { memcpy(p,&v,8); }
struct kunit { void *allocations[128]; unsigned count; };
static void *kunit_kzalloc(struct kunit *t,size_t n,int flags)
{
    (void)flags; assert(t->count<ARRAY_SIZE(t->allocations));
    void *p=calloc(1,n); assert(p); t->allocations[t->count++]=p; return p;
}
static void cleanup(struct kunit *t)
{
    while(t->count)free(t->allocations[--t->count]);
}
#define KUNIT_FAIL(t,...) assert(0)
#define KUNIT_ASSERT_NOT_NULL(t,p) assert(p)
#define KUNIT_ASSERT_NOT_ERR_OR_NULL(t,p) assert(!IS_ERR_OR_NULL(p))
#define KUNIT_ASSERT_EQ(t,a,b) assert((a)==(b))
#define KUNIT_ASSERT_LT(t,a,b) assert((a)<(b))
#define KUNIT_EXPECT_EQ(t,a,b) assert((a)==(b))
#define KUNIT_EXPECT_EQ_MSG(t,a,b,...) assert((a)==(b))
#define KUNIT_EXPECT_LT(t,a,b) assert((a)<(b))
#define KUNIT_EXPECT_TRUE(t,a) assert(a)
#define KUNIT_EXPECT_FALSE(t,a) assert(!(a))
'''
    template = (apple / 'iomfb_template.c').read_text()
    iomfb = (apple / 'iomfb.c').read_text()
    publisher = block(iomfb, 'int dcp_modes_replace(')
    opt_in = re.search(r'ctx->fw27_hdmi_ycbcr420\s*=.*?;', publisher, re.S).group()
    profile = re.search(r'bool h17p_external\s*=.*?;', publisher, re.S).group()
    request = block((apple / 'iomfb.h').read_text(),
                    'struct dcp_set_digital_out_mode_req {') + ' __packed;\n'
    registration = next(line.strip().rstrip(',') for line in
                        (apple / 'iomfb_v27.c').read_text().splitlines()
                        if 'IOMFB_METHOD("A413",' in line)
    output = r'''
#include <stddef.h>
#define DCP_FIRMWARE_V_27_0 27
#define DCP_IOMFB_METHODS_H17P_EXTERNAL 2
#define DCP_FW_VER 27
#define DCP_FW_VERSION(a,b,c) (a)
#define DCP_FW_H17(v) ((v)>=26)
#define DCP_FW_NAME(n) n##_v27
#define DCP_INHERIT_BOOT_MODE 1
#define dev_info(...) ((void)0)
#define dev_dbg(...) ((void)0)
#define dev_err(...) ((void)0)
#define DRM_MODE_FMT "mode"
#define DRM_MODE_ARG(m) 0
#define IOMFB_METHOD(name,method) name
struct kref { unsigned count; };
struct dcp_wait_cookie { u32 status; bool replied; int done; struct kref refcount; };
struct drm_crtc_state { struct drm_display_mode mode; bool vrr_enabled; };
struct apple_dcp {
    int fw_compat,fixed_connector_type;
    struct { int iomfb_method_profile; } hw;
    void *active_typec_route;
    bool pq_requested,output_pq,main_display,use_timestamps,vrr_enabled;
    bool external_mode_valid;
    u64 external_mode_generation,external_generation,external_mode_catalog_generation;
    u64 swap_submit_timestamp;
    struct dcp_set_digital_out_mode_req mode,external_mode;
    struct dcp_display_mode selected;
};
static void kref_init(struct kref *k) { k->count=1; }
static void kref_get(struct kref *k) { k->count++; }
static void kref_put(struct kref *k,void (*release)(struct kref *))
{ assert(k->count); if(!--k->count)release(k); }
static void release_wait_cookie(struct kref *k)
{ free((char *)k - offsetof(struct dcp_wait_cookie,refcount)); }
static void init_completion(int *done) { *done=0; }
static void complete(int *done) { *done=1; }
static int wait_for_completion_timeout(int *done,int timeout)
{ (void)timeout; return *done; }
static int msecs_to_jiffies(int n) { return n; }
static bool lookup_mode(struct apple_dcp *d,const struct drm_display_mode *m,
                        struct dcp_display_mode *selected)
{ assert(m->clock==d->selected.mode.clock); *selected=d->selected; return true; }
static u64 dcp_modes_transfer_begin(struct apple_dcp *d) { (void)d; return 1; }
static void dcp_external_hdr_hints(struct apple_dcp *d,bool pq)
{ (void)d;(void)pq;assert(0); }
static void dcp_set_adaptive_sync(struct apple_dcp *d,u32 rate,void *cookie)
{ (void)d;(void)rate;(void)cookie;assert(0); }
static struct dcp_set_digital_out_mode_req sent;
static unsigned commands;
static u32 reply;
static void dcp_set_digital_out_mode(struct apple_dcp *d,bool async,
        struct dcp_set_digital_out_mode_req *req,
        void (*callback)(struct apple_dcp *,void *,void *),void *cookie)
{
    assert(!async); sent=*req; commands++; callback(d,&reply,cookie);
}
static void context(struct apple_dcp *dcp,struct dcp_parse_ctx *ctx)
{
PROFILE
OPT_IN
}
static const char *a413 = REGISTRATION;
'''.replace('PROFILE', profile).replace('OPT_IN', opt_in).replace('REGISTRATION', registration)
    output += block(template, 'static void complete_set_digital_out_mode(') + '\n'
    # The second implementation is the non-desktop path instantiated for FW27.
    output += block(template, template.rindex('int DCP_FW_NAME(iomfb_modeset)(')) + '\n'
    main = r'''
static void scope_and_command(struct kunit *test)
{
    struct ycc_blob *b=ycc_begin(test,1);
    ycc_timing(b,147,3840,60U<<16,YCC_NORMAL,false,true,true);
    struct apple_dcp base={.fw_compat=27,.fixed_connector_type=11,
        .hw={.iomfb_method_profile=2}};
    for(unsigned c=0;c<7;c++) {
        struct apple_dcp d=base;
        switch(c) {
        case 1:d.fw_compat=26;break;
        case 2:d.fw_compat=14;break;
        case 3:d.fixed_connector_type=10;break; /* DP */
        case 4:d.fixed_connector_type=14;break; /* eDP */
        case 5:d.active_typec_route=&d;break; /* borrowed HDMI pipe */
        case 6:d.hw.iomfb_method_profile=0;break;
        }
        struct dcp_parse_ctx ctx;unsigned count;
        assert(!parse(b->data,b->size,&ctx));context(&d,&ctx);
        assert(ctx.fw27_hdmi_ycbcr420==(c==0));
        assert(!ctx.automatic_1080p180_rgb8);
        struct dcp_display_mode *m=enumerate_modes(&ctx,&count,0,0,0,false,true);
        assert(!IS_ERR(m) && count==(c==0?1U:0U));
        if(count) {
            d.selected=m[0];
            struct drm_crtc_state state={.mode=m[0].mode};
            assert(sizeof(sent)==8 && offsetof(__typeof__(sent),color_mode_id)==0);
            assert(offsetof(__typeof__(sent),timing_mode_id)==4 && !strcmp(a413,"A413"));
            assert(!iomfb_modeset_v27(&d,&state));
            assert(commands==1 && sent.color_mode_id==187 && sent.timing_mode_id==147);
            assert(d.selected.sdr.id==187 && d.selected.sdr.format==3);
            assert(d.selected.sdr.range==1 && d.selected.sdr.colorimetry==1);
            assert(d.selected.sdr_rgb.score<0 && !d.output_pq);
            /* HDR still chooses the timing's PQ element through the same path. */
            d.pq_requested=true;
            assert(!iomfb_modeset_v27(&d,&state));
            assert(commands==2 && sent.color_mode_id==190 && d.output_pq);
            reply=6;
            assert(iomfb_modeset_v27(&d,&state)==-EREMOTEIO);
            reply=0;
        }
        free(m);
    }
}
int main(void)
{
    struct kunit test={0};
    dcp_ycc_catalog_test(&test);cleanup(&test);
    puts("PASS: actual parser retains 40 RGB timings and admits six SDR 422/420 timings");
    dcp_ycc_restrictions_test(&test);cleanup(&test);
    puts("PASS: unsafe/DSC, BT.601, mismatched/missing/duplicate/virtual output rejected");
    dcp_ycc_rgb_hdr_test(&test);cleanup(&test);
    dcp_ycc_unsafe_rgb_test(&test);cleanup(&test);
    puts("PASS: preferred RGB, PQ metadata and FW27 unsafe RGB8 1080p180 exclusion");
    dcp_ycc_truncated_test(&test);cleanup(&test);
    puts("PASS: truncated converter catalogs never publish a partial mode");
    scope_and_command(&test);cleanup(&test);
    puts("PASS: fixed FW27 HDMI only; actual modeset sends outer color187 through A413");
    return 0;
}
'''
    return (f['prefix'] + shim + f['metadata'] + f['epoch'] + f['header'] + parser +
            tests + request + output + main)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[6])
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    output = args.output or Path(tempfile.mkdtemp(prefix='apple-hdmi-color-'))
    output.mkdir(parents=True, exist_ok=True)
    digest = run_unit('hdmi-color', fixture(args.source.resolve()), output,
                      extra=('-Wno-unused-function',))
    (output / 'results.json').write_text(json.dumps({'status': 'PASS',
        'translation_unit_sha256': digest}, indent=2) + '\n')
    print((output / 'hdmi-color/case-0.log').read_text(), end='')
    print(f'PASS: {output}')


if __name__ == '__main__':
    main()
