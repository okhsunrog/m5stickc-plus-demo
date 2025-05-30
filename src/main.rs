use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use embassy_executor::Executor;
use embassy_time::Timer;
use static_cell::StaticCell;
use log::info;

#[embassy_executor::task]
async fn async_task() {
    loop {
        info!("Tick from embassy executor!");
        Timer::after_secs(1).await;
    }
}

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::timer::embassy_time_driver::link();
    esp_idf_svc::log::EspLogger::initialize_default();
    let _eventfd_fs = esp_idf_svc::io::vfs::MountedEventfs::mount(5)?;

    log::info!("Starting high-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-high\0"),
        priority: 7,
        ..Default::default()
    }
    .set()
    .unwrap();

    std::thread::Builder::new()
        .stack_size(10_000)
        .spawn(|| {
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|spawner| {
                spawner.spawn(async_task()).unwrap();
            });
        })?;

    Ok(())
}