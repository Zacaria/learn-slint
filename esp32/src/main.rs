use std::time::{Duration, Instant};
use slint_workshop_common::ValueStore;

// use std::error;

mod dht22;
mod esp32;

slint::include_modules!();

/// Our App struct that holds the UI
struct App {
    ui: MainWindow,
    timer: slint::Timer,
    last_sensor_data: ValueStore::<SensorData>,
}


impl App {
    /// Create a new App struct.
    /// 
    /// The App struct initializes the UI and the weather controller.
    fn new() -> anyhow::Result<Self> {        
        // Make a new MainWindow
        let ui = MainWindow::new().map_err(|e| anyhow::anyhow!(e))?;

        // Launch the DHT task in a separate thread
        let last_sensor_data = ValueStore::<SensorData>::default();
        let last_for_dht_task = last_sensor_data.clone();
        std::thread::spawn(move || dht_task(last_for_dht_task));

        // Return the App struct
        Ok(Self {
            ui,
            timer: slint::Timer::default(),
            last_sensor_data,
        })
    }

    /// Run the App
    fn run(&mut self) -> anyhow::Result<()> {
        let ui_handler = self.ui.as_weak();

        let last_for_dht_task = self.last_sensor_data.clone();

        self.timer.start(slint::TimerMode::Repeated, Duration::from_secs(5), move || {
            let ui = ui_handler.unwrap();
            let model = ViewModel::get(&ui);

            match last_for_dht_task.get() {
                None => model.set_sensor_status(SensorStatus::Error),
                Some(data) => {
                    model.set_current(data.into());
                    model.set_sensor_status(SensorStatus::Ok);
                }
            }
        });

        // Run the UI (and map an error to an anyhow::Error).
        self.ui.run().map_err(|e| anyhow::anyhow!(e))
    }
}


/// The struct that stores the sensor data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SensorData {
    /// The temperature in Celsius, read from the DHT22 sensor.
    temperature_celsius: f32,

    /// The humidity in percent, read from the DHT22 sensor.
    humidity_percent: f32,

    /// The time when the data was read.
    when: std::time::Duration,

    /// The status of the sensor.
    status: SensorStatus,
}

impl From<SensorData> for WeatherRecord {
    fn from(value: SensorData) -> Self {
        WeatherRecord {
            temperature_celsius: value.temperature_celsius,
            humidity_percent: value.humidity_percent,
            // When devices has started, don't have clock
            timestamp: slint::SharedString::from(value.when.as_secs().to_string()),
        }
    }
}


fn dht_task(last: ValueStore<SensorData>) {
    let start = Instant::now();
    let pin = 13;
    let dht = dht22::DHT22::new(pin);

    loop {
        match dht.read() {
            Ok((temperature_celsius, humidity_percent)) => {
                log::debug!("Temperature: {}°C, Humidity: {}%", temperature_celsius, humidity_percent);
                last.set(SensorData {
                    temperature_celsius,
                    humidity_percent,
                    when: start.elapsed(),
                    status: SensorStatus::Ok,
                });
            }
            Err(e) => {
                log::error!("Error reading DHT22 sensor: {:?}", e);
                // last.set(SensorData {
                //     temperature_celsius: 0.0,
                //     humidity_percent: 0.0,
                //     when: start.elapsed(),
                //     status: SensorStatus::Error(e.to_string()),
                // });
            }
        }
        // Implement the logic to handle the sensor data
        std::thread::sleep(Duration::from_millis(2000));
    }
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    // Set the platform
    slint::platform::set_platform(esp32::EspPlatform::new()).unwrap();

    // // Launch the DHT task in a separate thread
    // let last_sensor_data = ValueStore::<SensorData>::default();
    // let last_for_dht_task = last_sensor_data.clone();
    // std::thread::spawn(move || dht_task(last_sensor_data));

    let mut app = App::new()?;

    app.run()
}
