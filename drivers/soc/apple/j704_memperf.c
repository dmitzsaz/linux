// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * J704 (Apple M5, T8142) memory and fabric performance floors.
 *
 * The PMC arbitrates the DRAM controller (DCS) and SoC fabric (SOC) states between voter
 * agents: PMP 0, CLPC 1, GFX 2, AP 3 (ADT /arm-io/pmgr voter-agent). It only honours an agent
 * whose bit is set in a rail's interface enable word. macOS enables the static agents at
 * startup (ApplePMC::restorePMC) and its CLPC raises its own floor while the CPUs or the GPU
 * are busy. Under Linux nothing did either: every enable word read 0, the memory stayed in
 * its lowest state and CPU and GPU bandwidth were 15-19 GB/s instead of ~156 GB/s
 * (j704-linux/features/gpu-bringup/sol-m5-dram-bandwidth-study.md, AGENTS.md 2026-10-31).
 *
 * This module is the T8142 case of Aurora's drivers/pmdomain/apple/pmc.c and
 * drivers/soc/apple/clpc.c (which have no T8142 entry and are built into the kernel):
 *
 *  - at load it admits the PMP and CLPC agents on the DCS and SOC rails (read-modify-write,
 *    read back), keeping every other bit;
 *  - every sampling period it derives a CLPC floor from Linux CPU load (busiest CPU of each
 *    cpufreq policy, by frequency quartile) and from GPU activity (core power or the GFX
 *    power state), holds each rail's request for a while after demand stops and then
 *    releases it, so an idle machine gives the floor back;
 *  - before suspend and on unload it releases the floor; on unload it restores the enables.
 *
 * Register layout (T8132 layout of pmc.c, independently decoded for T8142 by the study):
 * voters at 0x382700000 (agent floor at agent << 14, pending at +4 with BIT(agent)), rails at
 * 0x382806000 (accepted agent state at 4 * agent, enable words at 0x2000 + 4 * rail), inside
 * ADT /arm-io/pmgr reg[47] (0x382000000, 16 MiB). GPU core power is SGX + 0xe01000 (as read
 * by the asahi driver), the GFX power state is the GPU PMGR's gfx@110 record.
 *
 * Floor encoding: DCS bits 3:0 (states 0-7), SOC bits 7:4 (states 0-4). Measured on J704 with
 * the floor held: 0x11 45/63 GB/s (1-core memset / 10-thread read), 0x23 70/107, 0x35 76/139,
 * 0x47 86/156; GPU fill 15.6 -> 130 GB/s. The mapping below is measured Linux policy, not
 * recovered macOS policy.
 */

#include <linux/bitfield.h>
#include <linux/cpufreq.h>
#include <linux/debugfs.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/jiffies.h>
#include <linux/kernel.h>
#include <linux/kernel_stat.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/seq_file.h>
#include <linux/slab.h>
#include <linux/suspend.h>
#include <linux/tick.h>
#include <linux/workqueue.h>

#define PMC_AGENT_STRIDE	0x4000
#define PMC_PENDING		0x4
#define PMC_STATE(agent)	(4 * (agent))
#define PMC_ENABLE(rail)	(0x2000 + 4 * (rail))


#define AGENT_PMP		0
#define AGENT_CLPC		1
#define RAIL_DCS		0
#define RAIL_SOC		1
#define RAILS			2
#define FLOOR_MASK		0xffU

#define DCS_MAX			7
#define SOC_MAX			4

#define MAX_CLUSTERS		8
#define LEVELS			4
#define SAMPLE_MS		4
#define BUSY_PCT		50
/* A frame at 60 Hz is 16.7 ms apart; hold across one missed frame. */
#define HOLD_MS			40
/* A cluster requests only after this many busy samples in a row. */
#define SUSTAIN			2

/* Busy cluster below 25/50/75/100 percent of its maximum frequency. */
static const u8 cpu_map[RAILS][LEVELS] = {
	[RAIL_DCS] = { 3, 5, 7, 7 },
	[RAIL_SOC] = { 2, 3, 4, 4 },
};
/* GPU active: what the desktop measurements were taken with. */
static const u8 gpu_map[RAILS] = { [RAIL_DCS] = 7, [RAIL_SOC] = 4 };

static bool policy = true;
module_param(policy, bool, 0644);
MODULE_PARM_DESC(policy, "Adjust the CLPC floor from CPU and GPU load (default on)");

struct cluster {
	unsigned int policy_cpu;
	cpumask_var_t cpus;
	unsigned int busy, level, streak;
	unsigned int request[RAILS];
};

static struct {
	void __iomem *voters, *rails, *gpu_power, *gfx_pstate;
	struct mutex lock;		/* floor/enable read-modify-write */
	struct delayed_work work, idle_work;
	bool stopped;
	u32 saved_enable[RAILS];
	u32 pinned;			/* nonzero: fixed floor from debugfs */
	unsigned int state[RAILS];
	unsigned long last_demand[RAILS];
	u32 floor;
	struct cluster clusters[MAX_CLUSTERS];
	unsigned int nr_clusters;
	cpumask_var_t covered;
	unsigned long next_scan;
	u64 *idle, *wall;
	unsigned long samples, gpu_samples, updates, pending_timeouts;
	struct dentry *debugfs;
} mp;

static u32 floor_read(void)
{
	return readl(mp.voters + AGENT_CLPC * PMC_AGENT_STRIDE);
}

/* Replace the CLPC floor's low byte and wait for the PMC to take it. Caller holds the lock. */
static void floor_write(u32 value)
{
	void __iomem *reg = mp.voters + AGENT_CLPC * PMC_AGENT_STRIDE;
	u32 old = readl(reg), val, pending;

	value &= FLOOR_MASK;
	if ((old & FLOOR_MASK) == value)
		return;
	writel((old & ~FLOOR_MASK) | value, reg);
	mp.updates++;
	if (readl_poll_timeout(reg + PMC_PENDING, pending, !(pending & BIT(AGENT_CLPC)), 10, 2000))
		mp.pending_timeouts++;
	val = readl(reg);
	mp.floor = val & FLOOR_MASK;
}

static int enable_agents(bool on)
{
	unsigned int rail;
	u32 bits = BIT(AGENT_PMP) | BIT(AGENT_CLPC);

	for (rail = 0; rail < RAILS; rail++) {
		void __iomem *reg = mp.rails + PMC_ENABLE(rail);
		u32 old = readl(reg);
		u32 val = on ? (old | bits) : ((old & ~bits) | (mp.saved_enable[rail] & bits));

		writel(val, reg);
		if (readl(reg) != val) {
			pr_err("j704-memperf: rail %u enable %#x -> %#x did not stick\n", rail, old, val);
			return -EIO;
		}
	}
	return 0;
}

static void find_clusters(void)
{
	unsigned int cpu, i;

	for_each_possible_cpu(cpu) {
		struct cpufreq_policy *pol;

		if (cpumask_test_cpu(cpu, mp.covered))
			continue;
		pol = cpufreq_cpu_get(cpu);
		if (!pol)
			continue;
		for (i = 0; i < mp.nr_clusters; i++)
			if (mp.clusters[i].policy_cpu == pol->cpu)
				break;
		if (i == mp.nr_clusters && i < MAX_CLUSTERS)
			mp.clusters[mp.nr_clusters++].policy_cpu = pol->cpu;
		if (i < mp.nr_clusters) {
			cpumask_set_cpu(cpu, mp.clusters[i].cpus);
			cpumask_set_cpu(cpu, mp.covered);
			mp.idle[cpu] = get_cpu_idle_time(cpu, &mp.wall[cpu], 0);
		}
		cpufreq_cpu_put(pol);
	}
}

static void sample_cluster(struct cluster *cl)
{
	struct cpufreq_policy *pol;
	unsigned int cpu, busy = 0, q, rail;

	for_each_cpu(cpu, cl->cpus) {
		u64 wall, idle, dw, di;
		bool valid;

		if (!cpu_online(cpu)) {
			mp.wall[cpu] = 0;
			continue;
		}
		idle = get_cpu_idle_time(cpu, &wall, 0);
		dw = wall - mp.wall[cpu];
		di = idle - mp.idle[cpu];
		valid = mp.wall[cpu];
		mp.wall[cpu] = wall;
		mp.idle[cpu] = idle;
		if (valid && dw && di <= dw)
			busy = max_t(unsigned int, busy, div64_u64((dw - di) * 100, dw));
	}
	cl->busy = busy;
	cl->level = 0;
	pol = cpufreq_cpu_get(cl->policy_cpu);
	if (pol) {
		if (pol->cpuinfo.max_freq)
			cl->level = pol->cur * 100 / pol->cpuinfo.max_freq;
		cpufreq_cpu_put(pol);
	}
	q = min_t(unsigned int, cl->level / (100 / LEVELS), LEVELS - 1);
	if (busy < BUSY_PCT)
		cl->streak = 0;
	else if (cl->streak < SUSTAIN)
		cl->streak++;
	for (rail = 0; rail < RAILS; rail++)
		cl->request[rail] = busy >= BUSY_PCT && cl->streak >= SUSTAIN ? cpu_map[rail][q] : 0;
}

static bool gpu_active(void)
{
	return (readl(mp.gpu_power) & 0xf) ||
	       FIELD_GET(GENMASK(7, 4), readl(mp.gfx_pstate)) == 0xf;
}

static void queue_sample(bool idle)
{
	if (READ_ONCE(mp.stopped))
		return;
	/* A held floor is sampled on time so that it drops; otherwise wait for a busy CPU. */
	queue_delayed_work(system_freezable_power_efficient_wq, idle ? &mp.idle_work : &mp.work,
			   msecs_to_jiffies(SAMPLE_MS));
}

static void sample(void)
{
	unsigned int demand[RAILS] = {}, rail, i;
	unsigned long now = jiffies;
	u32 floor = 0;

	mutex_lock(&mp.lock);
	mp.samples++;
	if (!cpumask_equal(mp.covered, cpu_possible_mask) && time_after_eq(now, mp.next_scan)) {
		find_clusters();
		mp.next_scan = now + HZ;
	}
	for (i = 0; i < mp.nr_clusters; i++) {
		sample_cluster(&mp.clusters[i]);
		for (rail = 0; rail < RAILS; rail++)
			demand[rail] = max(demand[rail], mp.clusters[i].request[rail]);
	}
	if (gpu_active()) {
		mp.gpu_samples++;
		for (rail = 0; rail < RAILS; rail++)
			demand[rail] = max_t(unsigned int, demand[rail], gpu_map[rail]);
	}
	for (rail = 0; rail < RAILS; rail++) {
		if (demand[rail]) {
			mp.last_demand[rail] = now;
			mp.state[rail] = demand[rail];
		} else if (time_after_eq(now, mp.last_demand[rail] + msecs_to_jiffies(HOLD_MS))) {
			mp.state[rail] = 0;
		}
	}
	floor = min_t(unsigned int, mp.state[RAIL_DCS], DCS_MAX) |
		(min_t(unsigned int, mp.state[RAIL_SOC], SOC_MAX) << 4);
	if (mp.pinned)
		floor = mp.pinned;
	else if (!READ_ONCE(policy))
		floor = 0;
	floor_write(floor);
	mutex_unlock(&mp.lock);
	queue_sample(!floor);
}

static void work_fn(struct work_struct *w) { sample(); }

static void halt(void)
{
	int i;

	WRITE_ONCE(mp.stopped, true);
	for (i = 0; i < 2; i++) {
		cancel_delayed_work_sync(&mp.work);
		cancel_delayed_work_sync(&mp.idle_work);
	}
	mutex_lock(&mp.lock);
	floor_write(0);
	mp.state[RAIL_DCS] = mp.state[RAIL_SOC] = 0;
	mutex_unlock(&mp.lock);
}

static void restart(void)
{
	unsigned int cpu;

	for_each_possible_cpu(cpu)
		mp.wall[cpu] = 0;
	mutex_lock(&mp.lock);
	enable_agents(true);
	mutex_unlock(&mp.lock);
	WRITE_ONCE(mp.stopped, false);
	queue_sample(true);
}

static int pm_notify(struct notifier_block *nb, unsigned long action, void *data)
{
	switch (action) {
	case PM_SUSPEND_PREPARE:
	case PM_HIBERNATION_PREPARE:
		halt();
		break;
	case PM_POST_SUSPEND:
	case PM_POST_HIBERNATION:
		restart();
		break;
	}
	return NOTIFY_DONE;
}

static struct notifier_block pm_nb = { .notifier_call = pm_notify };

static int status_show(struct seq_file *s, void *unused)
{
	unsigned int i, a;

	seq_printf(s, "policy %d pinned %#x floor %#x (DCS %u SOC %u)\n", READ_ONCE(policy),
		   mp.pinned, floor_read() & FLOOR_MASK, floor_read() & 0xf, (floor_read() >> 4) & 0xf);
	seq_printf(s, "enable dcs %#x soc %#x (saved %#x %#x)\n", readl(mp.rails + PMC_ENABLE(0)),
		   readl(mp.rails + PMC_ENABLE(1)), mp.saved_enable[0], mp.saved_enable[1]);
	for (a = 0; a < 4; a++)
		seq_printf(s, "agent %u floor %#x accepted %#x\n", a,
			   readl(mp.voters + a * PMC_AGENT_STRIDE), readl(mp.rails + PMC_STATE(a)));
	seq_printf(s, "samples %lu gpu_active %lu updates %lu pending_timeouts %lu\n", mp.samples,
		   mp.gpu_samples, mp.updates, mp.pending_timeouts);
	for (i = 0; i < mp.nr_clusters; i++)
		seq_printf(s, "cluster cpu%u busy %u%% level %u%% -> dcs %u soc %u\n",
			   mp.clusters[i].policy_cpu, mp.clusters[i].busy, mp.clusters[i].level,
			   mp.clusters[i].request[RAIL_DCS], mp.clusters[i].request[RAIL_SOC]);
	return 0;
}
DEFINE_SHOW_ATTRIBUTE(status);

/* Write a hex floor byte (DCS bits 3:0 <= 7, SOC bits 7:4 <= 4) to pin it; 0 returns to policy. */
static ssize_t pin_write(struct file *f, const char __user *buf, size_t len, loff_t *pos)
{
	u32 val;
	int ret = kstrtou32_from_user(buf, len, 16, &val);

	if (ret)
		return ret;
	if (val > FLOOR_MASK || (val & 0xf) > DCS_MAX || (val >> 4) > SOC_MAX)
		return -EINVAL;
	WRITE_ONCE(mp.pinned, val);
	mod_delayed_work(system_freezable_power_efficient_wq, &mp.work, 0);
	return len;
}

static const struct file_operations pin_fops = {
	.owner = THIS_MODULE,
	.write = pin_write,
};

static void unmap_all(void)
{
	if (mp.gfx_pstate)
		iounmap(mp.gfx_pstate);
	if (mp.gpu_power)
		iounmap(mp.gpu_power);
	if (mp.rails)
		iounmap(mp.rails);
	if (mp.voters)
		iounmap(mp.voters);
}

static void free_all(void)
{
	unsigned int i;

	for (i = 0; i < MAX_CLUSTERS; i++)
		free_cpumask_var(mp.clusters[i].cpus);
	free_cpumask_var(mp.covered);
	kfree(mp.idle);
	kfree(mp.wall);
	unmap_all();
	memset(&mp, 0, sizeof(mp));
}

static int memperf_probe(struct platform_device *pdev)
{
	unsigned int i, rail;
	struct device_node *node;
	struct platform_device *gpu;
	struct resource *resource;
	void __iomem **windows[] = { &mp.voters, &mp.rails, &mp.gpu_power, &mp.gfx_pstate };
	static const resource_size_t minimum[] = { 0x10000, 0x3000, 4, 4 };
	int ret = -ENOMEM;

	if (!of_machine_is_compatible("apple,t8142") || !of_machine_is_compatible("apple,j704")) {
		pr_info("j704-memperf: not a J704 (T8142); refusing\n");
		return -ENODEV;
	}
	/* Sample SGX only after its normal driver has completed startup. */
	node = of_parse_phandle(pdev->dev.of_node, "apple,gpu", 0);
	if (!node)
		return -EINVAL;
	if (!of_device_is_available(node)) {
		of_node_put(node);
		return -ENODEV;
	}
	gpu = of_find_device_by_node(node);
	of_node_put(node);
	if (!gpu)
		return -EPROBE_DEFER;
	if (!device_is_bound(&gpu->dev)) {
		put_device(&gpu->dev);
		return -EPROBE_DEFER;
	}
	if (!device_link_add(&pdev->dev, &gpu->dev, DL_FLAG_AUTOREMOVE_CONSUMER)) {
		put_device(&gpu->dev);
		return -ENOMEM;
	}
	put_device(&gpu->dev);
	mutex_init(&mp.lock);
	for (i = 0; i < ARRAY_SIZE(windows); i++) {
		resource = platform_get_resource(pdev, IORESOURCE_MEM, i);
		if (!resource || resource_size(resource) < minimum[i]) {
			ret = -EINVAL;
			goto err;
		}
		*windows[i] = ioremap(resource->start, resource_size(resource));
		if (!*windows[i])
			goto err;
	}
	mp.idle = kcalloc(nr_cpu_ids, sizeof(*mp.idle), GFP_KERNEL);
	mp.wall = kcalloc(nr_cpu_ids, sizeof(*mp.wall), GFP_KERNEL);
	if (!mp.voters || !mp.rails || !mp.gpu_power || !mp.gfx_pstate || !mp.idle || !mp.wall)
		goto err;
	for (i = 0; i < MAX_CLUSTERS; i++)
		if (!zalloc_cpumask_var(&mp.clusters[i].cpus, GFP_KERNEL))
			goto err;
	if (!zalloc_cpumask_var(&mp.covered, GFP_KERNEL))
		goto err;

	for (rail = 0; rail < RAILS; rail++)
		mp.saved_enable[rail] = readl(mp.rails + PMC_ENABLE(rail));
	mutex_lock(&mp.lock);
	ret = enable_agents(true);
	mutex_unlock(&mp.lock);
	if (ret)
		goto err_restore;

	find_clusters();
	mp.next_scan = jiffies + HZ;
	INIT_DELAYED_WORK(&mp.work, work_fn);
	INIT_DEFERRABLE_WORK(&mp.idle_work, work_fn);
	mp.debugfs = debugfs_create_dir("j704-memperf", NULL);
	debugfs_create_file("status", 0400, mp.debugfs, NULL, &status_fops);
	debugfs_create_file("pin", 0200, mp.debugfs, NULL, &pin_fops);
	register_pm_notifier(&pm_nb);
	pr_info("j704-memperf: PMP and CLPC admitted on DCS/SOC (enables %#x %#x -> %#x %#x); floor policy %s\n",
		mp.saved_enable[0], mp.saved_enable[1], readl(mp.rails + PMC_ENABLE(0)),
		readl(mp.rails + PMC_ENABLE(1)), policy ? "on" : "off");
	queue_sample(true);
	return 0;

err_restore:
	for (rail = 0; rail < RAILS; rail++)
		writel(mp.saved_enable[rail], mp.rails + PMC_ENABLE(rail));
err:
	free_all();
	return ret;
}

static void memperf_remove(struct platform_device *pdev)
{
	unsigned int rail;

	unregister_pm_notifier(&pm_nb);
	halt();
	debugfs_remove_recursive(mp.debugfs);
	mutex_lock(&mp.lock);
	for (rail = 0; rail < RAILS; rail++)
		writel(mp.saved_enable[rail], mp.rails + PMC_ENABLE(rail));
	mutex_unlock(&mp.lock);
	free_all();
}

static const struct of_device_id memperf_of_match[] = {
	{ .compatible = "apple,t8142-memory-performance" },
	{},
};
MODULE_DEVICE_TABLE(of, memperf_of_match);

static struct platform_driver memperf_driver = {
	.probe = memperf_probe,
	.remove = memperf_remove,
	.driver = {
		.name = "apple-j704-memperf",
		.of_match_table = memperf_of_match,
		.suppress_bind_attrs = true,
	},
};
module_platform_driver(memperf_driver);
MODULE_DESCRIPTION("J704 (Apple M5) PMC memory/fabric performance floors");
MODULE_LICENSE("Dual MIT/GPL");
