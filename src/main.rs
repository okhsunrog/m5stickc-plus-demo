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

use axp192_dd::{Axp192, AxpError, ChargeCurrentValue, Gpio0FunctionSelect, LdoId};

#[rustfmt::skip]
fn init_m5stickc_plus_pmic<I>(i2c: I) -> anyhow::Result<()>
where
    I: I2c,
    AxpError<I::Error>: core::fmt::Debug + Send + Sync + 'static,
    I::Error: core::fmt::Debug + Send + Sync + 'static,
{
    let mut axp = Axp192::new(i2c);
    axp.set_ldo_voltage_mv(LdoId::Ldo2, 3300)?;
    axp.ll.adc_enable_1().write(|r| {
        r.set_battery_current_adc_enable(true);
        r.set_acin_voltage_adc_enable(true);
        r.set_acin_current_adc_enable(true);
        r.set_vbus_voltage_adc_enable(true);
        r.set_vbus_current_adc_enable(true);
        r.set_aps_voltage_adc_enable(true);
    })?;
    axp.ll.charge_control_1().write(|r| r.set_charge_current(ChargeCurrentValue::Ma100))?;
    axp.set_gpio0_ldo_voltage_mv(3300)?;
    axp.ll.gpio_0_control().write(|r| {
        r.set_function_select(Gpio0FunctionSelect::LowNoiseLdoOutput);
    })?;
    axp.ll.power_output_control().modify(|r| {
        r.set_dcdc_1_output_enable(true);
        r.set_dcdc_3_output_enable(false);
        r.set_ldo_2_output_enable(true);
        r.set_ldo_3_output_enable(true);
        r.set_dcdc_2_output_enable(false);
        r.set_exten_output_enable(true);
    })?;
    axp.set_battery_charge_high_temp_threshold_mv(3226)?;
    axp.ll.backup_battery_charge_control().write(|r| {
        r.set_backup_charge_enable(true);
    })?;

    info!("Battery voltage: {:.0} mV", axp.get_battery_voltage_mv()?);
    info!("Charge current: {:.0} mA", axp.get_battery_charge_current_ma()?);
    Ok(())
}
