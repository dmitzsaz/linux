// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* J704 telemetry through the existing SMC transport. No SMC writes.
 * SMC keys and units follow the Asahi Linux macsmc-power driver.
 */
#include <linux/init.h>
#include <linux/mfd/macsmc.h>
#include <linux/module.h>
#include <linux/notifier.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/power_supply.h>
#include <linux/slab.h>
#include <linux/workqueue.h>

struct j704_power {
	struct apple_smc *smc;
	struct power_supply *battery, *ac;
	struct notifier_block notifier;
	struct delayed_work poll;
	bool native_charge;
};

static enum power_supply_property j704_battery_properties[] = {
	POWER_SUPPLY_PROP_STATUS,      POWER_SUPPLY_PROP_PRESENT,
	POWER_SUPPLY_PROP_CAPACITY,    POWER_SUPPLY_PROP_VOLTAGE_NOW,
	POWER_SUPPLY_PROP_CURRENT_NOW, POWER_SUPPLY_PROP_POWER_NOW,
	POWER_SUPPLY_PROP_TEMP,	       POWER_SUPPLY_PROP_CHARGE_NOW,
	POWER_SUPPLY_PROP_CHARGE_FULL, POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN,
	POWER_SUPPLY_PROP_CYCLE_COUNT, POWER_SUPPLY_PROP_HEALTH,
	POWER_SUPPLY_PROP_SCOPE,
};

static enum power_supply_property j704_ac_properties[] = {
	POWER_SUPPLY_PROP_ONLINE,
	POWER_SUPPLY_PROP_INPUT_POWER_LIMIT,
};

static int j704_battery_status(struct j704_power *power)
{
	u32 online;
	s16 current_ma;
	bool full = false, charging = false;
	int ret;

	ret = apple_smc_read_u32(power->smc, SMC_KEY(CHIS), &online);
	if (ret)
		return ret;
	ret = apple_smc_read_s16(power->smc, SMC_KEY(B0AC), &current_ma);
	if (ret)
		return ret;
	if (!online || current_ma < 0)
		return POWER_SUPPLY_STATUS_DISCHARGING;
	apple_smc_read_flag(power->smc, SMC_KEY(BSFC), &full);
	apple_smc_read_flag(power->smc, SMC_KEY(CHSC), &charging);
	if (full)
		return POWER_SUPPLY_STATUS_FULL;
	if (charging || current_ma > 0)
		return POWER_SUPPLY_STATUS_CHARGING;
	return POWER_SUPPLY_STATUS_NOT_CHARGING;
}

static int j704_battery_get(struct power_supply *psy,
			    enum power_supply_property property,
			    union power_supply_propval *value)
{
	struct j704_power *power = power_supply_get_drvdata(psy);
	u8 byte;
	u16 word;
	s16 signed_word;
	s32 signed_dword;
	bool flag;
	int ret;

	switch (property) {
	case POWER_SUPPLY_PROP_STATUS:
		ret = j704_battery_status(power);
		if (ret < 0)
			return ret;
		value->intval = ret;
		return 0;
	case POWER_SUPPLY_PROP_PRESENT:
		value->intval = 1;
		return 0;
	case POWER_SUPPLY_PROP_CAPACITY:
		ret = apple_smc_read_u8(power->smc, SMC_KEY(BUIC), &byte);
		if (!ret)
			value->intval = byte;
		return ret;
	case POWER_SUPPLY_PROP_VOLTAGE_NOW:
		ret = apple_smc_read_u16(power->smc, SMC_KEY(B0AV), &word);
		if (!ret)
			value->intval = word * 1000;
		return ret;
	case POWER_SUPPLY_PROP_CURRENT_NOW:
		ret = apple_smc_read_s16(power->smc, SMC_KEY(B0AC),
					 &signed_word);
		if (!ret)
			value->intval = signed_word * 1000;
		return ret;
	case POWER_SUPPLY_PROP_POWER_NOW:
		ret = apple_smc_read_s32(power->smc, SMC_KEY(B0AP),
					 &signed_dword);
		if (!ret)
			value->intval = signed_dword * 1000;
		return ret;
	case POWER_SUPPLY_PROP_TEMP:
		ret = apple_smc_read_u16(power->smc, SMC_KEY(B0AT), &word);
		if (!ret)
			value->intval = (int)word - 2732;
		return ret;
	case POWER_SUPPLY_PROP_CHARGE_NOW:
		ret = apple_smc_read_u16(power->smc, SMC_KEY(B0RM), &word);
		if (!ret)
			value->intval =
				(power->native_charge ? word : swab16(word)) *
				1000;
		return ret;
	case POWER_SUPPLY_PROP_CHARGE_FULL:
		ret = apple_smc_read_u16(power->smc, SMC_KEY(B0FC), &word);
		if (!ret)
			value->intval = word * 1000;
		return ret;
	case POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN:
		ret = apple_smc_read_u16(power->smc, SMC_KEY(B0DC), &word);
		if (!ret)
			value->intval = word * 1000;
		return ret;
	case POWER_SUPPLY_PROP_CYCLE_COUNT:
		ret = apple_smc_read_u16(power->smc, SMC_KEY(B0CT), &word);
		if (!ret)
			value->intval = word;
		return ret;
	case POWER_SUPPLY_PROP_HEALTH:
		ret = apple_smc_read_flag(power->smc, SMC_KEY(BBAD), &flag);
		if (!ret)
			value->intval = flag ? POWER_SUPPLY_HEALTH_DEAD :
					       POWER_SUPPLY_HEALTH_GOOD;
		return ret;
	case POWER_SUPPLY_PROP_SCOPE:
		value->intval = POWER_SUPPLY_SCOPE_SYSTEM;
		return 0;
	default:
		return -EINVAL;
	}
}

static int j704_ac_get(struct power_supply *psy,
		       enum power_supply_property property,
		       union power_supply_propval *value)
{
	struct j704_power *power = power_supply_get_drvdata(psy);
	u32 data;
	int ret;

	if (property == POWER_SUPPLY_PROP_ONLINE) {
		ret = apple_smc_read_u32(power->smc, SMC_KEY(CHIS), &data);
		if (!ret)
			value->intval = !!data;
		return ret;
	}
	if (property == POWER_SUPPLY_PROP_INPUT_POWER_LIMIT) {
		ret = apple_smc_read_u32(power->smc, SMC_KEY(ACPW), &data);
		if (!ret)
			value->intval = data * 1000;
		return ret;
	}
	return -EINVAL;
}

static const struct power_supply_desc j704_battery_desc = {
	.name = "j704-battery",
	.type = POWER_SUPPLY_TYPE_BATTERY,
	.properties = j704_battery_properties,
	.num_properties = ARRAY_SIZE(j704_battery_properties),
	.get_property = j704_battery_get,
};

static const struct power_supply_desc j704_ac_desc = {
	.name = "j704-ac",
	.type = POWER_SUPPLY_TYPE_MAINS,
	.properties = j704_ac_properties,
	.num_properties = ARRAY_SIZE(j704_ac_properties),
	.get_property = j704_ac_get,
};

static void j704_power_changed(struct j704_power *power)
{
	power_supply_changed(power->battery);
	power_supply_changed(power->ac);
}

static int j704_power_event(struct notifier_block *notifier,
			    unsigned long event, void *data)
{
	struct j704_power *power =
		container_of(notifier, struct j704_power, notifier);

	if ((event & 0xff000000) == 0x71000000)
		j704_power_changed(power);
	return NOTIFY_OK;
}

static void j704_power_poll(struct work_struct *work)
{
	struct j704_power *power =
		container_of(to_delayed_work(work), struct j704_power, poll);

	j704_power_changed(power);
	schedule_delayed_work(&power->poll, msecs_to_jiffies(10000));
}

static int j704_power_probe(struct platform_device *pdev)
{
	struct power_supply_config config = {};
	struct apple_smc_key_info info;
	struct j704_power *power;
	u16 voltage;
	int ret;

	if (!of_machine_is_compatible("apple,j704") ||
	    !of_machine_is_compatible("apple,t8142"))
		return -ENODEV;
	power = devm_kzalloc(&pdev->dev, sizeof(*power), GFP_KERNEL);
	if (!power)
		return -ENOMEM;
	power->smc = dev_get_drvdata(pdev->dev.parent);
	if (!power->smc)
		return -EPROBE_DEFER;
	ret = apple_smc_read_u16(power->smc, SMC_KEY(B0AV), &voltage);
	if (ret)
		return dev_err_probe(&pdev->dev, ret,
				     "Battery voltage unavailable\n");
	ret = apple_smc_get_key_info(power->smc, SMC_KEY(BCF0), &info);
	if (ret || (info.size != 1 && info.size != 4))
		return dev_err_probe(&pdev->dev, ret ?: -EINVAL,
				     "Unknown battery firmware format\n");
	power->native_charge = info.size == 1;
	platform_set_drvdata(pdev, power);
	config.drv_data = power;
	power->battery = devm_power_supply_register(
		&pdev->dev, &j704_battery_desc, &config);
	if (IS_ERR(power->battery))
		return PTR_ERR(power->battery);
	power->ac =
		devm_power_supply_register(&pdev->dev, &j704_ac_desc, &config);
	if (IS_ERR(power->ac))
		return PTR_ERR(power->ac);
	INIT_DELAYED_WORK(&power->poll, j704_power_poll);
	power->notifier.notifier_call = j704_power_event;
	ret = blocking_notifier_chain_register(&power->smc->event_handlers,
					       &power->notifier);
	if (ret)
		return ret;
	schedule_delayed_work(&power->poll, msecs_to_jiffies(10000));
	dev_info(
		&pdev->dev,
		"J704 battery/AC telemetry ready, native-charge=%u; no SMC writes\n",
		power->native_charge);
	return 0;
}

static void j704_power_remove(struct platform_device *pdev)
{
	struct j704_power *power = platform_get_drvdata(pdev);

	blocking_notifier_chain_unregister(&power->smc->event_handlers,
					   &power->notifier);
	cancel_delayed_work_sync(&power->poll);
}
