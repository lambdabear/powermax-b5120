use chrono::prelude::*;
use crc::{Crc, CRC_8_SMBUS};
use reqwest::Client;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration};

const CRC_8: Crc<u8> = Crc::<u8>::new(&CRC_8_SMBUS);

const TCPTIMEOUT: u64 = 10; // seconds
const INFLUXDBURL: &str =
    "http://localhost:9999/api/v2/write?org=kideasoft&bucket=env-sensor-data&precision=ms";
const TOKEN: &str = "Token 1iihb5Rr-Fa5g7xun-FD1-av-3Flurp0RnORNAe-mZgiUBEpX0L1w3Zez3syS8sU_rKNxPyu2yD_rC3664dvjg==";

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

fn crc8_check(send: &[u8], rev: &[u8]) -> bool {
    let mut data = Vec::new();
    data.extend_from_slice(send);
    data.extend_from_slice(rev);

    let len = data.len();

    if len > 3 {
        CRC_8.checksum(&data[..len - 1]) == data[len - 1]
    } else {
        false
    }
}

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
                for cmd in 0x01..=0x10 {
                    let data_len = 2;
                    let send = [0x0A, cmd, data_len];
                    // println!("Send: {:#X?}", send);

                    if let Err(e) = socket.write_all(&send).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }

                    sleep(Duration::from_secs(1)).await;

                    match socket.read(&mut buf).await {
                        // socket closed
                        Ok(n) if n == 0 => return,
                        Ok(n) => {
                            // println!("REV: {:#X?}", &buf[..n]);
                            if n == data_len as usize + 1 {
                                if crc8_check(&send, &buf[..n]) {
                                    let voltage = u16::from_be_bytes([buf[0], buf[1]]);
                                    println!("Cell {}: {}mV", cmd, voltage);

                                    let id = id.clone();
                                    tokio::spawn(async move {
                                        let http_client = Client::new();
                                        let res = http_client
                                            .post(INFLUXDBURL)
                                            .header("Authorization", TOKEN)
                                            .body(format!(
                                                "powermax_b5120,location={} cell_{}={}",
                                                id, cmd, voltage
                                            ))
                                            .send()
                                            .await;

                                        match res {
                                            Ok(r) => println!(
                                                "{} write {} cell_{} voltage to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                cmd,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} cell_{} voltage to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                cmd,
                                                e
                                            )
                                        };
                                    });
                                } else {
                                    println!("CRC-8 checksum error");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to read from socket; err = {:?}", e);
                            return;
                        }
                    };
                    sleep(Duration::from_secs(1)).await;
                }

                // Read temperatures
                for cmd in 0x13..=0x15 {
                    let data_len = 2;
                    let send = [0x0A, cmd, data_len];
                    // println!("Send: {:#X?}", send);

                    if let Err(e) = socket.write_all(&send).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }

                    sleep(Duration::from_secs(1)).await;

                    match socket.read(&mut buf).await {
                        // socket closed
                        Ok(n) if n == 0 => return,
                        Ok(n) => {
                            // println!("REV: {:#X?}", &buf[..n]);
                            if n == data_len as usize + 1 {
                                if crc8_check(&send, &buf[..n]) {
                                    let temperature = i16::from_be_bytes([buf[0], buf[1]]);
                                    let temperature = temperature as f32 / 100.0;
                                    println!("Temperature {}: {}C°", cmd - 0x12, temperature,);

                                    let id = id.clone();
                                    tokio::spawn(async move {
                                        let http_client = Client::new();
                                        let res = http_client
                                            .post(INFLUXDBURL)
                                            .header("Authorization", TOKEN)
                                            .body(format!(
                                                "powermax_b5120,location={} temperature_{}={}",
                                                id,
                                                cmd - 0x12,
                                                temperature
                                            ))
                                            .send()
                                            .await;

                                        match res {
                                            Ok(r) => println!(
                                                "{} write {} temperature_{} to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                cmd - 0x12,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} temperature_{} to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                cmd - 0x12,
                                                e
                                            )
                                        };
                                    });
                                } else {
                                    println!("CRC-8 checksum error");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to read from socket; err = {:?}", e);
                            return;
                        }
                    };
                    sleep(Duration::from_secs(1)).await;
                }

                // Read total voltage, current, full capacity, remaining capacity
                for cmd in [0x11, 0x12, 0x16, 0x17] {
                    let data_len = 4;
                    let send = [0x0A, cmd, data_len];

                    if let Err(e) = socket.write_all(&send).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }

                    sleep(Duration::from_secs(1)).await;

                    match socket.read(&mut buf).await {
                        // socket closed
                        Ok(n) if n == 0 => return,
                        Ok(n) => {
                            // println!("REV: {:#X?}", &buf[..n]);
                            if n == data_len as usize + 1 {
                                if crc8_check(&send, &buf[..n]) {
                                    match cmd {
                                        0x11 => {
                                            let voltage = u32::from_be_bytes([
                                                buf[0], buf[1], buf[2], buf[3],
                                            ]);
                                            println!("Total voltage: {}mV", voltage);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                "powermax_b5120,location={} total_voltage={}",
                                                id,
                                                voltage
                                            ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} total_voltage to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} total_voltage to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        0x12 => {
                                            let current = i32::from_be_bytes([
                                                buf[0], buf[1], buf[2], buf[3],
                                            ]);
                                            println!("Current: {}mA", current);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} current={}",
                                                        id, current,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} current to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} current to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        0x16 => {
                                            let full_capacity = u32::from_be_bytes([
                                                buf[0], buf[1], buf[2], buf[3],
                                            ]);
                                            println!("Full capacity: {}mAH", full_capacity);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} full_capacity={}",
                                                        id, full_capacity,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} full_capacity to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} full_capacity to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        0x17 => {
                                            let remaining_capacity = u32::from_be_bytes([
                                                buf[0], buf[1], buf[2], buf[3],
                                            ]);
                                            println!(
                                                "Remaining capacity: {}mAH",
                                                remaining_capacity
                                            );

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} remaining_capacity={}",
                                                        id, remaining_capacity,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} remaining_capacity to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} remaining_capacity to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        _ => (),
                                    }
                                } else {
                                    println!("CRC-8 checksum error");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("failed to read from socket; err = {:?}", e);
                            return;
                        }
                    };
                    sleep(Duration::from_secs(1)).await;
                }

                // Read RSOC, cycle count, pack status, battery status, pack config
                for cmd in 0x18..=0x1C {
                    let data_len = 2;
                    let send = [0x0A, cmd, data_len];

                    if let Err(e) = socket.write_all(&send).await {
                        eprintln!("failed to write to socket; err = {:?}", e);
                        return;
                    }

                    sleep(Duration::from_secs(1)).await;

                    match socket.read(&mut buf).await {
                        // socket closed
                        Ok(n) if n == 0 => return,
                        Ok(n) => {
                            // println!("REV: {:#X?}", &buf[..n]);
                            if n == data_len as usize + 1 {
                                if crc8_check(&send, &buf[..n]) {
                                    let data = u16::from_be_bytes([buf[0], buf[1]]);

                                    match cmd {
                                        0x18 => {
                                            println!("RSOC: {}%", data);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} RSOC={}",
                                                        id, data,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                                    Ok(r) => println!(
                                                        "{} write {} RSOC to influxDB: resp = {:?}",
                                                        Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                        id,
                                                        r
                                                    ),
                                                    Err(e) => eprintln!(
                                                "{}: write {} RSOC to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            ),
                                                };
                                            });
                                        }
                                        0x19 => {
                                            println!("Cycle count: {}", data);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} cycle_count={}",
                                                        id, data,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} cycle count to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} cycle count to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        0x1A => {
                                            println!("Pack status: {:#04X}", data);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} pack_status={}",
                                                        id, data,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} pack_status to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} pack_status to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        0x1B => {
                                            println!("Battery status: {:#04X}", data);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} battery_status={}",
                                                        id, data,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} battery_status to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} battery_status to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        0x1C => {
                                            println!("Pack config: {:#04X}", data);

                                            let id = id.clone();
                                            tokio::spawn(async move {
                                                let http_client = Client::new();
                                                let res = http_client
                                                    .post(INFLUXDBURL)
                                                    .header("Authorization", TOKEN)
                                                    .body(format!(
                                                        "powermax_b5120,location={} pack_config={}",
                                                        id, data,
                                                    ))
                                                    .send()
                                                    .await;

                                                match res {
                                            Ok(r) => println!(
                                                "{} write {} pack_config to influxDB: resp = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                r
                                            ),
                                            Err(e) => eprintln!(
                                                "{}: write {} pack_config to influxDB failed. err = {:?}",
                                                Local::now().format("%Y-%m-%d %H:%M:%S"),
                                                id,
                                                e
                                            )
                                        };
                                            });
                                        }
                                        _ => (),
                                    }
                                } else {
                                    println!("CRC-8 checksum error");
                                }
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
