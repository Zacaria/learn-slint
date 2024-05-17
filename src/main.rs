extern crate alloc;

struct EspPlatform {
    panel_handle: esp_idf_svc::hal::sys::esp_lcd_panel_handle_t,
    touch_handle: Option<*mut esp_idf_svc::hal::sys::esp_lcd_touch_s>,
    window: alloc::rc::Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
}

impl EspPlatform {
    const DISPLAY_WIDTH: usize = 320;
    const DISPLAY_HEIGHT: usize = 240;
    const DRAW_BUFFER_SIZE: usize = Self::DISPLAY_WIDTH * Self::DISPLAY_HEIGHT;

    /// Create a new instance of the platform
    fn new() -> std::boxed::Box<Self> {
        use esp_idf_svc::hal::sys::*;

        /* Initialize I2C (for touch and audio) */
        unsafe {
            bsp_i2c_init();
        }

        // Initialize LCD panel and touch
        let mut io_handle: esp_lcd_panel_io_handle_t = std::ptr::null_mut();
        let mut panel_handle: esp_lcd_panel_handle_t = std::ptr::null_mut();

        let bsp_disp_cfg = bsp_display_config_t {
            max_transfer_sz: (Self::DRAW_BUFFER_SIZE
                * std::mem::size_of::<slint::platform::software_renderer::Rgb565Pixel>())
                as i32,
        };

        let mut touch_handle: *mut esp_lcd_touch_s = std::ptr::null_mut();
        let bsp_touch_cfg = bsp_touch_config_t::default();

        unsafe {
            bsp_display_new(&bsp_disp_cfg, &mut panel_handle, &mut io_handle);
            bsp_touch_new(&bsp_touch_cfg, &mut touch_handle);
            bsp_display_backlight_on();
        }

        // Setup the window
        let window =
            slint::platform::software_renderer::MinimalSoftwareWindow::new(Default::default());
        window.set_size(slint::PhysicalSize::new(
            Self::DISPLAY_WIDTH as u32,
            Self::DISPLAY_HEIGHT as u32,
        ));

        std::boxed::Box::new(Self {
            panel_handle,
            touch_handle: Some(touch_handle),
            window,
        })
    }
}

impl slint::platform::Platform for EspPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<alloc::rc::Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        // Since on MCUs, there can be only one window, just return a clone of self.window.
        // We'll also use the same window in the event loop.
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> core::time::Duration {
        unsafe {
            let ticks = esp_idf_svc::hal::sys::xTaskGetTickCount(); // One tick is 10ms, according to sdkconfig.defaults
            core::time::Duration::from_millis(ticks as u64 * 10)
        }
    }
    fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
        use esp_idf_svc::hal::sys::*;

        unsafe {
            // Initialize the LCD panel
            if esp_lcd_panel_init(self.panel_handle) != ESP_OK {
                log::error!("Failed to initialize LCD panel");
                return Err(slint::PlatformError::Other(
                    "Failed to initialize LCD panel".into(),
                ));
            }

            // Turn on the display
            esp_lcd_panel_disp_on_off(self.panel_handle, true);

            // Calling this function rotates the display by 180 degrees
            esp_lcd_panel_mirror(self.panel_handle, true, true);

            let mut last_position = slint::LogicalPosition::default();
            let mut touch_down = false;

            // Create a buffer to draw the scene
            use slint::platform::software_renderer::Rgb565Pixel;
            let mut buffer = vec![Rgb565Pixel(0x0); Self::DRAW_BUFFER_SIZE];

            loop {
                slint::platform::update_timers_and_animations();

                if let Some(touch_handle) = self.touch_handle {
                    let mut touchpad_x = [0];
                    let mut touchpad_y = [0];
                    let mut touchpad_cnt = [0 as u8];

                    esp_lcd_touch_read_data(touch_handle);

                    let touchpad_pressed = esp_lcd_touch_get_coordinates(
                        touch_handle,
                        touchpad_x.as_mut_ptr(),
                        touchpad_y.as_mut_ptr(),
                        std::ptr::null_mut(),
                        touchpad_cnt.as_mut_ptr(),
                        1,
                    );

                    if touchpad_pressed && touchpad_cnt[0] > 0 {
                        last_position =
                            slint::LogicalPosition::new(touchpad_x[0] as f32, touchpad_y[0] as f32);

                        self.window
                            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                                position: last_position,
                            });

                        if !touch_down {
                            self.window.dispatch_event(
                                slint::platform::WindowEvent::PointerPressed {
                                    position: last_position,
                                    button: slint::platform::PointerEventButton::Left,
                                },
                            );
                            log::info!("Touchpad pressed: {:?}", last_position);
                        }

                        touch_down = true;
                    } else if touch_down {
                        self.window
                            .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                                position: last_position,
                                button: slint::platform::PointerEventButton::Left,
                            });
                        self.window
                            .dispatch_event(slint::platform::WindowEvent::PointerExited);
                        touch_down = false;
                    }
                }

                // Draw the scene if something needs to be drawn.
                self.window.draw_if_needed(|renderer| {
                    // Do the rendering!
                    let region = renderer.render(&mut buffer, Self::DISPLAY_WIDTH as usize);

                    // Iterate each region to be updated
                    for (o, s) in region.iter() {
                        // Update the display line by line
                        for y in o.y..(o.y + s.height as i32) {
                            for x in o.x..(o.x + s.width as i32) {
                                let offset = (y * Self::DISPLAY_WIDTH as i32 + x) as usize;
                                let pixel = buffer[offset].0;
                                // Convert pixel to big endian
                                let pixel = ((pixel & 0xff) << 8) | ((pixel & 0xff00) >> 8);
                                buffer[offset].0 = pixel;
                            }

                            use std::ffi::c_void;

                            // This is the magic function that sends the buffer to the display
                            esp_lcd_panel_draw_bitmap(
                                self.panel_handle,
                                o.x,
                                y,
                                o.x + s.width as i32,
                                y + 1,
                                buffer
                                    .as_ptr()
                                    .add((y * Self::DISPLAY_WIDTH as i32 + o.x) as usize)
                                    .cast::<c_void>(),
                            );
                        }
                    }
                });

                // Try to put the MCU to sleep
                if !self.window.has_active_animations() {
                    continue;
                }
            }
        }
    }
}

slint::include_modules!();

fn create_slint_app() -> AppWindow {
    let ui = AppWindow::new().expect("Failed to load UI");

    let ui_handle = ui.as_weak();
    ui.on_request_increase_value(move || {
        let ui = ui_handle.unwrap();
        ui.set_counter(ui.get_counter() + 1);
    });

    ui
}

unsafe extern "C" fn dht_task(_: *mut core::ffi::c_void) {
    //let dht = slint::platform::dht::Dht::new(4).unwrap();
    let mut last_temperature = 0.0_f32;
    let mut last_humidity = 0.0_f32;

    loop {
        let (temperature, humidity) = (0.0_f32, 0.0_f32); // dht.read().unwrap();
        //if temperature != last_temperature || humidity != last_humidity {
            last_temperature = temperature;
            last_humidity = humidity;
            log::info!("Temperature: {:.2}°C, Humidity: {:.2}%", temperature, humidity);
        //}

        unsafe {
            esp_idf_svc::sys::vTaskDelay(2000 / 10);
        }
    }

}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    // Set the platform
    slint::platform::set_platform(EspPlatform::new()).unwrap();

    // Create DHT task
    unsafe {
        let mut task_handle = std::ptr::null_mut();
        esp_idf_svc::sys::xTaskCreatePinnedToCore(
            Some(dht_task),
            b"sensor_task\0".as_ptr() as *const i8,
            4096,
            std::ptr::null_mut(),
            5,
            &mut task_handle,
            1
        );
    }

    // Finally, run the app!
    create_slint_app().run().unwrap();
}
