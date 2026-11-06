#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""FW27 fixed-HDMI SDR converter policy and A413, with synthetic catalogs only."""
import argparse
import copy
import json
from pathlib import Path
import re
import struct
import tempfile

from common import block, run_unit
import rate


def converter_timing(f):
    timing = copy.deepcopy(f['timing_fixture'][0])
    timing.update(ID=147, Score=100, ValidPixelEncodings=2,
                  UnsafeColorElementIDs=[9, 7, 8], DSCRequiredColorElementIDs=[])
    timing['HorizontalAttributes'].update(Active=3840, Total=4400,
        FrontPorch=176, SyncWidth=88, PreciseSyncRate=0)
    timing['VerticalAttributes'].update(Active=2160, Total=2250,
        FrontPorch=8, SyncWidth=10, PreciseSyncRate=60 << 16)
    color = dict(ID=187, Score=100, Depth=8, PixelEncoding=3,
                 DynamicRange=1, EOTF=0, Colorimetry=1, IsVirtual=False)
    color['DownstreamFormat'] = dict(color, ID=186, PixelEncoding=1)
    timing['ColorModes'] = [color]
    return timing


def timing_inputs(f):
    """Mutate timing dictionaries independently of the C fixture builder."""
    pairs = f['ObjectPairs']
    timing = converter_timing(f)
    cases = [('valid', timing)]
    for field in ('ID', 'Score', 'IsVirtual', 'IsInterlaced',
                  'HorizontalAttributes', 'VerticalAttributes'):
        missing = copy.deepcopy(timing)
        del missing[field]
        cases.append(('missing-timing-' + field, missing))
    for field, value in [('ID', 28), ('Score', 1), ('IsVirtual', False),
                         ('HorizontalAttributes', timing['HorizontalAttributes']),
                         ('VerticalAttributes', timing['VerticalAttributes'])]:
        duplicate = pairs(list(copy.deepcopy(timing).items()) + [(field, value)])
        cases.append(('duplicate-timing-' + field, duplicate))
    for scope in ('HorizontalAttributes', 'VerticalAttributes'):
        for field in timing[scope]:
            missing = copy.deepcopy(timing)
            del missing[scope][field]
            cases.append(('missing-' + scope + '-' + field, missing))
            duplicate = copy.deepcopy(timing)
            value = 3839 if field == 'Active' else timing[scope][field]
            duplicate[scope] = pairs(list(duplicate[scope].items()) + [(field, value)])
            cases.append(('duplicate-' + scope + '-' + field, duplicate))
    definitions, checks = '', ''
    for name, value in cases:
        for rgb in (False, True):
            catalog = copy.deepcopy(value)
            rgb_color = dict(ID=176, Score=1, Depth=8, PixelEncoding=0,
                             DynamicRange=0, EOTF=0, Colorimetry=10, IsVirtual=False)
            if rgb:
                if isinstance(catalog, pairs):
                    catalog = pairs([(k, [rgb_color] + v if k == 'ColorModes' else v)
                                     for k, v in catalog])
                else:
                    catalog['ColorModes'].insert(0, rgb_color)
            raw = bytearray(struct.pack('<I', 0xd3))
            f['wire']([catalog], raw)
            index = definitions.count('static const')
            definitions += 'static const u8 identity_%u[]={%s};\n' % (
                index, ','.join(map(str, raw)))
            checks += ('check_timing_input(%s,identity_%u,sizeof(identity_%u),%s,%s);\n' %
                       (json.dumps(name), index, index, str(rgb).lower(),
                        str(name == 'valid').lower()))
    driver = r'''
static void check_timing_input(const char *name,const u8 *raw,size_t len,bool rgb,bool valid)
{
    struct drm_display_mode baseline={0};
    unsigned baseline_count=0;
    u32 baseline_id=0;
    for(unsigned enabled=0;enabled<2;enabled++) {
        struct dcp_parse_ctx ctx;unsigned count=0,before=commands;
        assert(!parse(raw,len,&ctx));ctx.fw27_hdmi_ycbcr420=enabled;
        struct dcp_display_mode *m=enumerate_modes(&ctx,&count,0,0,0,false,true);
        if(IS_ERR(m)){count=0;m=NULL;}
        assert(count<=1);
        if(rgb && !enabled) {
            baseline_count=count;
            if(count){baseline=m[0].mode;baseline_id=m[0].timing_mode_id;}
        }
        unsigned want=rgb?baseline_count:enabled&&valid;
        if(count) {
            struct apple_dcp d={.fw_compat=27,.fixed_connector_type=11,
                .hw={.iomfb_method_profile=2},.selected=m[0]};
            struct drm_crtc_state state={.mode=m[0].mode};
            assert(!iomfb_modeset_v27(&d,&state));
            assert(sent.color_mode_id==(rgb?176U:187U));
            if(rgb) {
                assert(sent.timing_mode_id==baseline_id);
                assert(!memcmp(&baseline,&m[0].mode,sizeof(baseline)));
            } else {
                if(!want)fprintf(stderr,"unexpected A413: %s color=%u timing=%u\n",
                    name,sent.color_mode_id,sent.timing_mode_id);
                assert(sent.timing_mode_id==147);
            }
        }
        assert(count==want && commands==before+want);
        free(m);
    }
}
static void timing_identity_and_command(void)
{
CHECKS
}
'''.replace('CHECKS', checks)
    return definitions + driver, len(cases)


def sibling_inputs(f):
    pairs = f['ObjectPairs']
    timing = converter_timing(f)
    good = timing['ColorModes'][0]
    bad_depth = dict(good, Depth=10)
    bad_output = copy.deepcopy(good)
    bad_output['DownstreamFormat']['ID'] = 9
    distinct = dict(bad_depth, ID=188)
    cases = [('valid-only', [timing], 1, 1, 187),
             ('invalid-only', [dict(timing, ColorModes=[bad_depth])], 0, 0, 187),
             ('unsafe-output-only', [dict(timing, ColorModes=[bad_output])], 0, 0, 187)]
    for name, colors, count, color_id in [
        ('conflicting-depth', [good, bad_depth], 0, 187),
        ('conflicting-output', [good, bad_output], 0, 187),
        ('invalid-distinct-id', [good, distinct], 1, 187),
        ('identical-color-siblings', [good, dict(good, Score=200)], 1, 187),
        ('valid-distinct-colors', [good, dict(good, ID=188, Score=200)], 1, 188),
        ('ambiguous-color-id', [good, pairs(list(good.items()) + [('ID', 188)])], 0, 187),
    ]:
        for reverse in (False, True):
            values = list(reversed(colors)) if reverse else colors
            cases.append((name + str(int(reverse)), [dict(timing, ColorModes=values)],
                          count, count, color_id))
    for reverse in (False, True):
        for duplicate in (False, True):
            slow = copy.deepcopy(timing)
            slow['ID'] = 147 if duplicate else 143
            slow['HorizontalAttributes'].update(Total=5280, FrontPorch=1056)
            slow['VerticalAttributes']['PreciseSyncRate'] = 50 << 16
            for filtered in ('valid', 'depth', 'virtual', 'incomplete', 'ambiguous-id'):
                first = copy.deepcopy(slow)
                if filtered == 'depth':
                    first['ColorModes'] = [bad_depth]
                elif filtered == 'virtual':
                    first['IsVirtual'] = True
                elif filtered == 'incomplete':
                    del first['IsVirtual']
                elif filtered == 'ambiguous-id':
                    first = pairs(list(first.items()) + [('ID', 188)])
                values = [timing, first] if reverse else [first, timing]
                count = 0 if duplicate else 1 + (filtered == 'valid')
                cases.append((f'timing-{reverse}-{duplicate}-{filtered}', values,
                              count, count, 187))
    cases.append(('identical-timing-siblings', [timing, timing], 0, 0, 187))
    cases.append(('color-identity-bound', [dict(timing, ColorModes=[good] * 65)], 0, 0, 187))
    definitions, checks = '', ''
    for name, value, count, commands, color_id in cases:
        for rgb in (False, True):
            catalog = copy.deepcopy(value)
            if rgb:
                rgb_color = dict(ID=176, Score=1, Depth=8, PixelEncoding=0,
                                 DynamicRange=0, EOTF=0, Colorimetry=10, IsVirtual=False)
                for i, entry in enumerate(catalog):
                    if isinstance(entry, pairs):
                        catalog[i] = pairs([(k, [rgb_color] + v if k == 'ColorModes' else v)
                                            for k, v in entry])
                    else:
                        entry['ColorModes'].insert(0, rgb_color)
            raw = bytearray(struct.pack('<I', 0xd3))
            f['wire'](catalog, raw)
            index = definitions.count('static const')
            definitions += 'static const u8 sibling_%u[]={%s};\n' % (
                index, ','.join(map(str, raw)))
            checks += ('check_sibling_input(%s,sibling_%u,sizeof(sibling_%u),%s,%u,%u,%u);\n' %
                       (json.dumps(name), index, index, str(rgb).lower(),
                        count, commands, color_id))
    driver = r'''
static void check_sibling_input(const char *name,const u8 *raw,size_t len,bool rgb,
                                unsigned want,unsigned want_commands,u32 color_id)
{
    unsigned baseline_count=0,baseline_commands=0;
    for(unsigned enabled=0;enabled<2;enabled++) {
        struct dcp_parse_ctx ctx;unsigned count=0;
        assert(!parse(raw,len,&ctx));ctx.fw27_hdmi_ycbcr420=enabled;
        struct dcp_display_mode *m=enumerate_modes(&ctx,&count,0,0,0,false,true);
        if(IS_ERR(m)){count=0;m=NULL;}
        struct apple_dcp d={.fw_compat=27,.fixed_connector_type=11,
            .hw={.iomfb_method_profile=2}};
        commands=0;reply=0;
        for(unsigned i=0;i<count;i++) {
            d.selected=m[i];struct drm_crtc_state state={.mode=m[i].mode};
            assert(!iomfb_modeset_v27(&d,&state));
            assert(sent.color_mode_id==(rgb?176:color_id));
            /* Mirror the enclosing caller's successful pair receipt. */
            d.external_mode_valid=true;d.external_mode=d.mode;
            d.external_mode_generation=d.external_generation;
            d.external_mode_catalog_generation=1;
            unsigned before=commands;
            assert(!iomfb_modeset_v27(&d,&state) && commands==before);
        }
        if(rgb && !enabled){baseline_count=count;baseline_commands=commands;}
        unsigned expected=rgb?baseline_count:enabled?want:0;
        unsigned expected_commands=rgb?baseline_commands:enabled?want_commands:0;
        if(count!=expected || commands!=expected_commands)
            fprintf(stderr,"sibling %s: modes=%u A413=%u expected=%u/%u\n",
                    name,count,commands,expected,expected_commands);
        assert(count==expected && commands==expected_commands);
        free(m);
    }
}
static void sibling_identity_and_cache(void)
{
CHECKS
}
'''.replace('CHECKS', checks)
    return definitions + driver, len(cases)


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
    dcp_ycc_timing_identity_test(&test);cleanup(&test);
    puts("PASS: incomplete/ambiguous converter timings rejected; RGB policy retained");
    dcp_ycc_sibling_colors_test(&test);cleanup(&test);
    dcp_ycc_sibling_timings_test(&test);cleanup(&test);
    puts("PASS: converter sibling identities checked before eligibility filtering");
    scope_and_command(&test);cleanup(&test);
    puts("PASS: fixed FW27 HDMI only; actual modeset sends outer color187 through A413");
    timing_identity_and_command();
    puts("PASS: timing mutation matrix reaches actual A413 only for admitted controls");
    sibling_identity_and_cache();
    puts("PASS: conflicting siblings withheld; distinct commands and A413 cache controls");
    return 0;
}
'''
    identity, _ = timing_inputs(f)
    siblings, _ = sibling_inputs(f)
    return (f['prefix'] + shim + f['metadata'] + f['epoch'] + f['header'] + parser +
            tests + request + output + identity + siblings + main)


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
