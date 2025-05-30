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
        info!("Tick from embassy executor!");
        Timer::after_secs(1).await;
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::timer::embassy_time_driver::link();
    esp_idf_svc::log::EspLogger::initialize_default();
    let _eventfd_fs = esp_idf_svc::io::vfs::MountedEventfs::mount(5)?;

    let p = Peripherals::take().unwrap();
    let i2c_config = i2c::config::Config {
        baudrate: KiloHertz(400).into(),
        sda_pullup_enabled: true,
        scl_pullup_enabled: true,
        ..Default::default()
    };
    let i2c: I2cDriver<'_> =
        I2cDriver::new(p.i2c1, p.pins.gpio21, p.pins.gpio22, &i2c_config).unwrap();
    init_m5stickc_plus_pmic(i2c);

    log::info!("Starting high-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-high\0"),
        priority: 7,
        ..Default::default()
    }
    .set()
    .unwrap();

    std::thread::Builder::new().stack_size(10_000).spawn(|| {
        let executor = EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(async_task()).unwrap();
        });
    })?;

    Ok(())
}

use axp192_dd::{
    Axp192Blocking,
    LdoId,
    PekBootTime,
    PekLongPressTime,
    PekShutdownDuration,
};

fn init_m5stickc_plus_pmic(i2c: impl I2c) {
    let mut axp = Axp192Blocking::new(i2c);
    info!("Applying M5StickC-Plus AXP192 configuration...");

    // LDO2 & LDO3 to 3.0V, enabled
    axp.set_ldo_voltage(LdoId::Ldo2, 3000)
        .expect("Set LDO2 voltage");
    axp.set_ldo_voltage(LdoId::Ldo3, 3000)
        .expect("Set LDO3 voltage");
    axp.set_output_enable_ldo(LdoId::Ldo2, true)
        .expect("Enable LDO2");
    axp.set_output_enable_ldo(LdoId::Ldo3, true)
        .expect("Enable LDO3");


    // PEK settings: 128ms ON, 4s OFF, PWROK 64ms (Reg 0x36 to 0x0C)
    axp.set_pek_settings(
        PekBootTime::S128ms,
        PekLongPressTime::Ms1000, // M5 default for this field seems 1s
        false,                    // auto_shutdown_by_pwrok_en
        true,                     // pwrok_signal_delay_64ms
        PekShutdownDuration::S4,
    )
    .expect("Set PEK settings");

    info!("AXP192 configured for M5StickC-Plus.");
}
