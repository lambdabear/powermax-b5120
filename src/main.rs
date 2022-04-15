use chrono::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration};

// const TCPTIMEOUT: u64 = 10; // seconds
// const INFLUXDBURL: &str =
//     "http://localhost:9999/api/v2/write?org=kideasoft&bucket=env-sensor-data&precision=ms";
// const TOKEN: &str = "Token 1iihb5Rr-Fa5g7xun-FD1-av-3Flurp0RnORNAe-mZgiUBEpX0L1w3Zez3syS8sU_rKNxPyu2yD_rC3664dvjg==";

// #[derive(Debug, Clone)]
// struct PowerStatus {
//     pub grid_voltage: f32,
//     pub grid_frequency: f32,
//     pub ac_output_voltage: f32,
//     pub ac_output_frequency: f32,
//     pub ac_output_apparent_power: u32,
//     pub ac_output_active_power: u32,
//     pub output_load_percent: u32,
//     pub bus_voltage: u32,
//     pub battery_voltage: f32,
//     pub battery_charging_current: u32,
//     pub battery_capacity: u32,
//     pub inverter_heat_sink_temperature: u32,
//     pub pv_input_current: u32,
//     pub pv_input_voltage: f32,
//     pub battery_voltage_from_scc: f32,
//     pub battery_discharge_current: u32,
//     pub sbu_priority: bool,
//     pub configuration_status: bool,
//     pub scc_firmware_version_status: bool,
//     pub load_status: bool,
//     pub charging_status: ChargingStatus,
//     pub battery_voltage_offset_for_fans_on: u32,
//     pub eeprom_version: u32,
//     pub pv_charging_power: u32,
//     pub charging_in_floating_mode: bool,
//     pub switch_on: bool,
// }

// #[derive(Debug, Clone)]
// enum ChargingStatus {
//     Scc,
//     Ac,
//     SccAc,
//     DoNothing,
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("0.0.0.0:30278").await?;

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            let id: String;

            match socket.read(&mut buf).await {
                // socket closed
                Ok(n) if n == 0 => return,
                Ok(n) if n == 6 => {
                    // id = 6 bytes MAC address
                    let id_num = u64::from_be_bytes([
                        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
                    ]);
                    id = format!("{:#x}", id_num).to_string();

                    // TODO:
                    // authentication process

                    // DEBUG:
                    println!("******************************************************");
                    println!(
                        "{} Connected device: {}",
                        Local::now().format("%Y-%m-%d %H:%M:%S"),
                        id
                    );
                    println!("******************************************************");
                }
                Ok(n) => {
                    println!("MAC addr error\nREV: {:#X?}", &buf[..n]);
                    return;
                }
                Err(e) => {
                    eprintln!("failed to read from socket; err = {:?}", e);
                    return;
                }
            };

            // In a loop, write command to the socket and read the data.
            loop {
                // Read cells voltage
                for cmd in 0x01..0x10 {
                    let data_len = 2;
                    let msg = [0x0A, cmd, data_len];
                    println!("Send: {:#X?}", msg);

                    if let Err(e) = socket.write_all(&msg).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }

                    sleep(Duration::from_secs(1)).await;

                    match socket.read(&mut buf).await {
                        // socket closed
                        Ok(n) if n == 0 => return,
                        Ok(n) => {
                            println!("REV: {:#X?}", &buf[..n]);
                            if n == 3 {
                                let voltage = u16::from_be_bytes(buf[0], buf[1]);
                                println!("Cell {}: {}mV", cmd, voltage);
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to read from socket; err = {:?}", e);
                            return;
                        }
                    };
                    sleep(Duration::from_secs(1)).await;
                }
            }
        });
    }
}
