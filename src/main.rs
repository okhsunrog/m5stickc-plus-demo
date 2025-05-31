use embassy_executor::Executor;
use embassy_time::Timer;
use embedded_hal::i2c::I2c;
use esp_idf_svc::hal::{
    gpio::PinDriver,
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
            spawner.spawn(async_task())?;
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

const AXP192_ADDR: u8 = 0x34;

fn init_m5stickc_plus_pmic<I>(i2c: &mut I) -> anyhow::Result<()>
where
    I: I2c,
    I::Error: std::error::Error + Send + Sync + 'static,
{
    // AXP192 Initialization
    // Set LDO2 & LDO3 (TFT_LED & TFT) to 3.0V
    i2c.write(AXP192_ADDR, &[0x28, 0xCC])?;
    // Set ADC to All Enable
    i2c.write(AXP192_ADDR, &[0x82, 0xFF])?;
    // Set battery charge voltage to 4.2V, current 100mA
    i2c.write(AXP192_ADDR, &[0x33, 0xC0])?;
    // Enable Bat, ACIN, VBUS, APS ADC (redundant, already set)
    i2c.write(AXP192_ADDR, &[0x82, 0xFF])?;
    // Enable Ext, LDO2, LDO3, DCDC1
    let mut prev_value: [u8; 1] = Default::default();
    i2c.write_read(AXP192_ADDR, &[0x12], &mut prev_value)?;
    i2c.write(AXP192_ADDR, &[0x12, prev_value[0] | 0x4D])?;
    // 128ms power on, 4s power off
    i2c.write(AXP192_ADDR, &[0x36, 0x0C])?;
    // Set RTC voltage to 3.3V
    i2c.write(AXP192_ADDR, &[0x91, 0xF0])?;
    // Set GPIO0 to LDO
    i2c.write(AXP192_ADDR, &[0x90, 0x02])?;
    // Disable VBUS hold limit
    i2c.write(AXP192_ADDR, &[0x30, 0x80])?;
    // Set temperature protection
    i2c.write(AXP192_ADDR, &[0x39, 0xFC])?;
    // Enable RTC battery charge
    i2c.write(AXP192_ADDR, &[0x35, 0xA2])?;
    // Enable battery detection
    i2c.write(AXP192_ADDR, &[0x32, 0x46])?;

    info!("reading battery voltage");

    // Read battery voltage (auto-increment method)
    let mut buffer = [0u8; 2];
    i2c.write_read(AXP192_ADDR, &[0x78], &mut buffer)?;
    let high = buffer[0];
    let low = buffer[1];
    let adc_value = (high as u16) << 4 | (low & 0x0F) as u16;
    let voltage = (adc_value * 11) / 10;

    // Log the result
    info!("Battery voltage: {} mV", voltage);
    Ok(())
}
