use embassy_executor::Executor;
use embassy_time::Timer;
use embedded_hal::i2c::I2c;
use esp_idf_svc::hal::{
    i2c::{self, I2cDriver},
    prelude::Peripherals,
    task::thread::ThreadSpawnConfiguration,
    units::KiloHertz,
};
use log::info;
use static_cell::StaticCell;

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn async_task() {
    loop {
        //info!("Tick from embassy executor!");
        Timer::after_secs(1).await;
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::timer::embassy_time_driver::link();
    esp_idf_svc::log::EspLogger::initialize_default();
    let _eventfd_fs = esp_idf_svc::io::vfs::MountedEventfs::mount(5)?;

    let p = Peripherals::take()?;
    let i2c_config = i2c::config::Config {
        baudrate: KiloHertz(400).into(),
        sda_pullup_enabled: true,
        scl_pullup_enabled: true,
        ..Default::default()
    };
    let mut i2c: I2cDriver<'_> = I2cDriver::new(p.i2c1, p.pins.gpio21, p.pins.gpio22, &i2c_config)?;
    init_m5stickc_plus_pmic(&mut i2c)?;

    // indicate that the device in turned on
    // let mut led = PinDriver::output(p.pins.gpio10)?;
    // led.set_high()?;

    log::info!("Starting high-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-high\0"),
        priority: 7,
        ..Default::default()
    }
    .set()?;

    std::thread::Builder::new().stack_size(10_000).spawn(|| {
        let executor = EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(async_task()).unwrap();
        });
    })?;

    Ok(())
}

use axp192_dd::{
    Axp192,
    AxpError, // Your public API enums
    // Import generated enums directly from the crate root (where device_driver places them)
    BackupChargeCurrentValue,
    BackupTargetVoltageValue,
    ChargeCurrentValue,
    ChargeEndCurrentThresholdValue,
    ChargeTargetVoltageValue,
    ChgLedControlSourceSelect,
    ChgLedFunctionSetting,
    Gpio0FunctionSelect,
    NoeShutdownDelayValue,
    PekLongPressTime,
    PekPowerOnTime,
    PekShutdownTime,
    PwrokSignalDelay,
    VbusCurrentLimitValue,
    VbusPathSelectionControl,
    VholdVoltageValue, // Add others as needed
};

fn init_m5stickc_plus_pmic<I>(i2c: I) -> anyhow::Result<()>
where
    I: I2c,
    AxpError<I::Error>: core::fmt::Debug + Send + Sync + 'static,
    I::Error: core::fmt::Debug + Send + Sync + 'static,
{
    info!("Initializing AXP192 with axp192-dd driver...");
    let mut axp = Axp192::new(i2c);

    // Set LDO2 & LDO3 to 3.0V (0xCC for REG28)
    axp.ll.ldo_2_and_3_voltage_setting().write(|r| {
        r.set_ldo_2_voltage_setting(0x0C);
        r.set_ldo_3_voltage_setting(0x0C);
    })?;

    // Set ADC to All Enable (REG82 = 0xFF, REG83 = 0xFF - or specific bits if not all GPIOs are ADC)
    axp.ll.adc_enable_1().write(|r| {
        // All true
        r.set_battery_voltage_adc_enable(true);
        r.set_battery_current_adc_enable(true);
        r.set_acin_voltage_adc_enable(true);
        r.set_acin_current_adc_enable(true);
        r.set_vbus_voltage_adc_enable(true);
        r.set_vbus_current_adc_enable(true);
        r.set_aps_voltage_adc_enable(true);
        r.set_ts_pin_adc_enable(true);
    })?;
    axp.ll.adc_enable_2().write(|r| {
        // Enable relevant bits
        r.set_internal_temperature_adc_enable(true);
        r.set_gpio_0_adc_enable(true); // Assuming you want these for M5Stick
        r.set_gpio_1_adc_enable(true);
        r.set_gpio_2_adc_enable(true);
        r.set_gpio_3_adc_enable(true);
    })?;

    // Set battery charge voltage to 4.2V, current 100mA (REG33H = 0xC0)
    axp.ll.charge_control_1().write(|r| {
        r.set_charge_enable(true); // Bit 7 = 1
        r.set_target_voltage(ChargeTargetVoltageValue::V420); // Bits 6-5 = 10 (4.2V)
        r.set_end_current_threshold(ChargeEndCurrentThresholdValue::Percent10); // Bit 4 = 0 (10%)
        r.set_charge_current(ChargeCurrentValue::Ma100); // Bits 3-0 = 0000 (100mA)
    })?;

    // Enable Exten, LDO2, LDO3, DCDC1 (Original: REG12 = val | 0x4D)
    axp.ll.power_output_control().modify(|r| {
        r.set_dcdc_1_output_enable(true); // Bit 0
        r.set_dcdc_3_output_enable(false); // Bit 1
        r.set_ldo_2_output_enable(true); // Bit 2
        r.set_ldo_3_output_enable(true); // Bit 3
        r.set_dcdc_2_output_enable(false); // Bit 4 (Mirrors REG10H[0])
        r.set_exten_output_enable(true); // Bit 6 (Mirrors REG10H[2])
    })?;

    // PEK settings: 128ms power on, 4s power off (REG36H = 0x0C)
    axp.ll.pek_key_parameters().write(|r| {
        r.set_power_on_time(PekPowerOnTime::Ms128);
        r.set_long_press_time(PekLongPressTime::S10);
        r.set_auto_shutdown_if_pek_held_longer_than_shutdown_time(true);
        r.set_pwrok_signal_delay(PwrokSignalDelay::Ms64); // Assuming PwrokSignalDelay enum exists
        r.set_shutdown_time(PekShutdownTime::S4);
    })?;

    // Set GPIO0 to LDO mode (REG90H = 0x02)
    axp.ll.gpio_0_control().modify(|r| {
        r.set_function_select(Gpio0FunctionSelect::LowNoiseLdoOutput);
    })?;

    // Set LDOIO0 (GPIO0 LDO) voltage to 3.3V (REG91H = 0xF0)
    axp.ll.gpio_0_ldo_voltage_setting().modify(|r| {
        r.set_voltage_setting_raw(0x0F);
    })?;

    // VBUS settings (REG30H = 0x80)
    axp.ll.vbus_ipsout_path_management().write(|r| {
        r.set_path_selection_override(VbusPathSelectionControl::ForcedOpen);
        r.set_vhold_limit_enabled(false);
        r.set_vhold_voltage(VholdVoltageValue::V40);
        r.set_vbus_current_limit_enabled(false);
        r.set_vbus_current_limit(VbusCurrentLimitValue::Ma500);
    })?;

    // Set temperature protection (REG39H = 0xFC)
    axp.ll.battery_charge_high_temp_threshold().write(|r| {
        r.set_threshold_setting_raw(0xFC);
    })?;

    // Enable RTC/Backup battery charge (REG35H = 0xA2)
    axp.ll.backup_battery_charge_control().write(|r| {
        r.set_backup_charge_enable(true);
        r.set_backup_target_voltage(BackupTargetVoltageValue::V30); // Assumes 0b01 variant maps to V3_0
        r.set_backup_charge_current(BackupChargeCurrentValue::Ua200);
    })?;

    // Shutdown/BatteryDetection/CHGLED (REG32H = 0x46)
    axp.ll.shutdown_bat_chg_led_control().write(|r| {
        r.set_request_shutdown_mode_a(false);
        r.set_battery_monitoring_enable(true);
        r.set_chgled_function(ChgLedFunctionSetting::HighZ);
        r.set_chgled_control_source(ChgLedControlSourceSelect::ByChargeLogic);
        r.set_n_oe_shutdown_delay(NoeShutdownDelayValue::S2);
    })?;

    info!("AXP192 initialized using driver.");
    info!("Reading battery voltage via driver...");

    let voltage_f32 = axp.get_battery_voltage_mv()?;
    info!("Battery voltage (driver): {:.0} mV", voltage_f32);

    Ok(())
}
