use esp_hal::{rng::Rng, time::Instant};
use esp_wifi::wifi::{
    self,
    event::{ApStaconnected, ApStadisconnected, EventExt},
    WifiController, WifiDevice,
};

use smoltcp::{
    iface as tcp,
    wire::{EthernetAddress, HardwareAddress},
};

use blocking_network_stack::ipv4;
type BlockingStack<'a> = blocking_network_stack::Stack<'a, WifiDevice<'a>>;
type BlockingSocket<'a> = blocking_network_stack::Socket<'a, 'a, WifiDevice<'a>>;

use esp_println::println;
use log::{info, error};

pub fn setup_event_handlers() {
    ApStaconnected::update_handler(move |event| {
        let mac = event.0.mac;
        info!("connected mac: {mac:?}",);
    });

    ApStadisconnected::update_handler(|event| {
        let mac = event.0.mac;
        let reason = event.0.reason;
        info!("disconnected mac: {mac:?}, reason: {reason:?}",);
    });
}

pub fn get_stack<'a>(
    rng: &mut Rng,
    mut device: WifiDevice<'a>,
    sockets: tcp::SocketSet<'a>,
) -> BlockingStack<'a> {
    let network_interface = create_interface(&mut device);
    BlockingStack::new(
        network_interface,
        device,
        sockets,
        || Instant::now().duration_since_epoch().as_millis(),
        rng.random(),
    )
}

pub fn get_socket<'a>(
    controller: &mut WifiController,
    stack: &'a mut BlockingStack<'a>,
    rx: &'a mut [u8],
    tx: &'a mut [u8],
) -> BlockingSocket<'a> {
    let client_config = wifi::Configuration::AccessPoint(wifi::AccessPointConfiguration {
        ssid: "esp-wifi".into(),
        ..Default::default()
    });

    let res = controller.set_configuration(&client_config);
    if res.is_err() {
        error!("wifi_set_configuration returned {res:?}");
    }

    controller.start().unwrap();
    let controller_started = controller.is_started();
    match controller_started {
        Ok(true) => {}
        Ok(false) | Err(_) => error!("is wifi started: {controller_started:?}"),
    };

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

    println!("Connect to `esp-wifi` and goto http://192.168.2.1:8080/");
    println!("Use a static IP 192.168.2.2 .. 192.168.2.255, use gateway 192.168.2.1");

    stack.get_socket(rx, tx)
}

// some smoltcp boilerplate
fn timestamp() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(Instant::now().duration_since_epoch().as_micros() as i64)
}

fn create_interface(device: &mut esp_wifi::wifi::WifiDevice) -> tcp::Interface {
    // users could create multiple instances but since they only have one WifiDevice
    // they probably can't do anything bad with that
    tcp::Interface::new(
        tcp::Config::new(HardwareAddress::Ethernet(EthernetAddress::from_bytes(
            &device.mac_address(),
        ))),
        device,
        timestamp(),
    )
}
