#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use embedded_io::{Read as _, Write as _};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    main,
    rng::Rng,
    time::{Duration, Instant},
    timer::timg::TimerGroup,
};

use log::info;

use esp_wifi::wifi::{
    event::{self, EventExt},
    AccessPointConfiguration, Configuration,
};

use blocking_network_stack::ipv4;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.5.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    // Set event handlers for wifi before init to avoid missing any.
    let mut connections = 0u32;
    _ = event::ApStart::replace_handler(|_| esp_println::println!("ap start event"));
    event::ApStaconnected::update_handler(move |event| {
        connections += 1;
        esp_println::println!("connected {}, mac: {:?}", connections, event.0.mac);
    });
    event::ApStaconnected::update_handler(|event| {
        esp_println::println!("connected aid: {}", event.0.aid);
    });
    event::ApStadisconnected::update_handler(|event| {
        esp_println::println!(
            "disconnected mac: {:?}, reason: {:?}",
            event.0.mac,
            event.0.reason
        );
    });

    let mut rng = Rng::new(peripherals.RNG);

    let esp_wifi_ctrl = esp_wifi::init(timg0.timer0, rng).unwrap();

    let (mut controller, interfaces) =
        esp_wifi::wifi::new(&esp_wifi_ctrl, peripherals.WIFI).unwrap();

    let mut device = interfaces.ap;
    let iface = create_interface(&mut device);

    let now = || Instant::now().duration_since_epoch().as_millis();

    let mut socket_set_entries: [smoltcp::iface::SocketStorage; 3] = Default::default();
    let socket_set = smoltcp::iface::SocketSet::new(&mut socket_set_entries[..]);
    let mut stack =
        blocking_network_stack::Stack::new(iface, device, socket_set, now, rng.random());

    let client_config = Configuration::AccessPoint(AccessPointConfiguration {
        ssid: "esp-wifi".into(),
        ..Default::default()
    });
    let res = controller.set_configuration(&client_config);
    info!("wifi_set_configuration returned {res:?}");

    controller.start().unwrap();
    info!("is wifi started: {:?}", controller.is_started());

    info!("{:?}", controller.capabilities());

    stack
        .set_iface_configuration(&ipv4::Configuration::Client(
            ipv4::ClientConfiguration::Fixed(ipv4::ClientSettings {
                ip: ipv4::Ipv4Addr::from([192, 168, 2, 1]),
                subnet: ipv4::Subnet {
                    gateway: ipv4::Ipv4Addr::from([192, 168, 2, 1]),
                    mask: ipv4::Mask(24),
                },
                dns: None,
                secondary_dns: None,
            }),
        ))
        .unwrap();

    info!("Connect to the `esp-wifi` and point your browser to http://192.168.2.1:8080/");
    info!("Use a static IP in the range 192.168.2.2 .. 192.168.2.255, use gateway 192.168.2.1");

    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];
    let mut socket = stack.get_socket(&mut rx_buffer, &mut tx_buffer);

    socket.listen(8080).unwrap();

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
                    info!("{to_print}");
                    break;
                }

                pos += len;

                if Instant::now() > deadline {
                    info!("Timeout");
                    time_out = true;
                    break;
                }
            }

            if !time_out {
                socket
                    .write_all(
                        b"HTTP/1.0 200 OK\r\n\r\n\
                    <html>\
                        <body>\
                            <h1>Hello Rust! Hello esp-wifi!</h1>\
                        </body>\
                    </html>\r\n\
                    ",
                    )
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

// some smoltcp boilerplate
fn timestamp() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(Instant::now().duration_since_epoch().as_micros() as i64)
}

pub fn create_interface(device: &mut esp_wifi::wifi::WifiDevice) -> smoltcp::iface::Interface {
    use smoltcp::{
        iface::{Config, Interface},
        wire::{EthernetAddress, HardwareAddress},
    };

    // users could create multiple instances but since they only have one WifiDevice
    // they probably can't do anything bad with that
    Interface::new(
        Config::new(HardwareAddress::Ethernet(EthernetAddress::from_bytes(
            &device.mac_address(),
        ))),
        device,
        timestamp(),
    )
}
