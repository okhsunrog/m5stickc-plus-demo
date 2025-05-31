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
    let mut i2c: I2cDriver<'_> =
        I2cDriver::new(p.i2c1, p.pins.gpio21, p.pins.gpio22, &i2c_config).unwrap();
    init_m5stickc_plus_pmic(&mut i2c)?;

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

// use axp192_dd::{
//     Axp192Blocking,
//     LdoId,
//     PekBootTime,
//     PekLongPressTime,
//     PekShutdownDuration,
// };

fn init_m5stickc_plus_pmic<I>(i2c: &mut I) -> anyhow::Result<()>
where
    I: I2c,
    I::Error: std::error::Error + Send + Sync + 'static,
{
    info!("reading battery voltage");

    // Enable battery voltage ADC (set bit 7 of register 0x82)
    let mut reg82 = [0u8; 1];
    i2c.write_read(0x34, &[0x82], &mut reg82)?;
    info!("Initial value of register 0x82: 0x{:02X}", reg82[0]);
    let new_reg82 = reg82[0] | 0x80;
    i2c.write(0x34, &[0x82, new_reg82])?;

    // Method 1: Read two bytes from 0x78 with auto-increment
    let mut buffer = [0u8; 2];
    i2c.write_read(0x34, &[0x78], &mut buffer)?;
    info!("Method 1 - Raw values: 0x78: 0x{:02X}, 0x79: 0x{:02X}", buffer[0], buffer[1]);
    let high1 = buffer[0];
    let low1 = buffer[1];
    let adc_value1 = (high1 as u16) << 4 | (low1 & 0x0F) as u16;
    let voltage1 = (adc_value1 * 11) / 10;

    // Method 2: Read one byte from 0x78 and one from 0x79 separately
    let mut high_byte = [0u8; 1];
    let mut low_byte = [0u8; 1];
    i2c.write_read(0x34, &[0x78], &mut high_byte)?;
    i2c.write_read(0x34, &[0x79], &mut low_byte)?;
    info!("Method 2 - Raw values: 0x78: 0x{:02X}, 0x79: 0x{:02X}", high_byte[0], low_byte[0]);
    let high2 = high_byte[0];
    let low2 = low_byte[0];
    let adc_value2 = (high2 as u16) << 4 | (low2 & 0x0F) as u16;
    let voltage2 = (adc_value2 * 11) / 10;

    // Log the results
    info!("Voltage method 1 (auto-increment): {} mV", voltage1);
    info!("Voltage method 2 (separate reads): {} mV", voltage2);
    
    Ok(())
}