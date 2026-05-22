#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use shared::wifi;

use embedded_io::{Read as _, Write as _};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    main,
    rng::Rng,
    time::{Duration, Instant},
    timer::timg::TimerGroup,
};

use esp_println::println;
use log::warn;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    // Set event handlers for wifi before init to avoid missing any.
    wifi::setup_event_handlers();

    let esp_controller = esp_wifi::init(timg0.timer0, rng).unwrap();
    let (mut controller, interfaces) =
        esp_wifi::wifi::new(&esp_controller, peripherals.WIFI).unwrap();

    let mut socket_storage: [smoltcp::iface::SocketStorage; 3] = Default::default();
    let sockets = smoltcp::iface::SocketSet::new(&mut socket_storage[..]);

    let mut stack = wifi::get_stack(&mut rng, interfaces.ap, sockets);

    let mut rx_buf = [0; 1536];
    let mut tx_buf = [0; 1536];
    let mut socket = wifi::get_socket(&mut controller, &mut stack, &mut rx_buf, &mut tx_buf);

    loop {
        socket.work();

        if !socket.is_open() {
            socket.listen(8080).unwrap();
        }

        if socket.is_connected() {
            let mut time_out = false;
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut buffer = [0u8; 1024];
            let mut pos = 0;
            while let Ok(len) = socket.read(&mut buffer[pos..]) {
                let to_print = unsafe { core::str::from_utf8_unchecked(&buffer[..(pos + len)]) };

                if to_print.contains("\r\n\r\n") {
                    println!("{to_print}");
                    break;
                }

                pos += len;

                if Instant::now() > deadline {
                    warn!("Timeout");
                    time_out = true;
                    break;
                }
            }

            if !time_out {
                socket
                    .write_all(b"HTTP/1.0 200 OK\r\n\r\nHello, World!")
                    .unwrap();

                socket.flush().unwrap();
            }

            socket.close();
        }

        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(200) {
            socket.work();
        }
    }
}
