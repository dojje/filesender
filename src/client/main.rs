use std::{net::{SocketAddr}, error::Error, fs::{File, self}, io::Write, path::Path, vec, str::FromStr};

use log::info;
use rand::{Rng};
use serde::{Serialize, Deserialize};
use shared::{messages::{have_file::HaveFile, you_have_file::{YouHaveFile}, ServerMsg, Message, ip_for_code::IpForCode, taker_ip::TakerIp, i_have_code::{IHaveCode}}, send_msg};
use tokio::net::UdpSocket;
use clap::Parser;

const CONFIG_FILENAME: &'static str = "filesender_data.toml";

#[derive(Serialize, Deserialize)]
struct Config {
    server_ips: Vec<String>
}

impl Config {
    fn new() -> Self {
        Self {
            server_ips: vec!["127.0.0.1:47335".to_string()]
        }
    }
}

#[derive(clap::Subcommand, Debug)]
enum Action {
    Give,
    Take
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Subcommand to execute
    #[clap(subcommand)]
    action: Action,

    /// Code for file to recieve
    /// Must be set if it should recieve files
    #[clap(short, long)]
    code: Option<String>,
}

fn _get_msg_from_raw(raw: &[u8]) -> Result<ServerMsg, &'static str> {
    if let Ok(have_file) = YouHaveFile::from_raw(raw) {
        Ok(ServerMsg::YouHaveFile(have_file))
    }
    else if let Ok(i_have_code) = IpForCode::from_raw(raw) {
        Ok(ServerMsg::IpForCode(i_have_code))
    }
    else if let Ok(taker_ip) = TakerIp::from_raw(raw) {
        Ok(ServerMsg::TakerIp(taker_ip))
    }

    else {
        Err("could not make into any message")
    }
}

async fn punch_hole(sock: &UdpSocket, addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    sock.send_to(&[254u8], addr).await?;

    Ok(())
}

fn get_config() -> Config {
    // Check if settings file exists
    if !Path::new(CONFIG_FILENAME).exists() {
        let config = Config::new();

        let confing_str = toml::to_string(&config).unwrap();
        let mut file = File::create(CONFIG_FILENAME).unwrap();

        // Write a &str in the file (ignoring the result).
        write!(&mut file, "{}", confing_str).unwrap();
    }

    let contents = fs::read_to_string(CONFIG_FILENAME)
        .expect("Something went wrong reading the appdata file");

    let config: Config = toml::from_str(contents.as_str()).unwrap();

    config
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = get_config();
    let args = Args::parse();
    let mut rng = rand::thread_rng();

    let port: u16 = rng.gen_range(8192..u16::MAX);
    info!("using port {}", port);

    let server_addr = SocketAddr::from_str(config.server_ips[0].as_str())?;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let sock = UdpSocket::bind(addr).await?;
    
    match args.action {
        Action::Give => {
            sender("hey_guys.txt", sock, server_addr).await?;
        },
        Action::Take => {
            let code = match args.code {
                Some(code) => code,
                None => {panic!("code must be set");},
            };
            
            reciever(code, sock, server_addr).await?;
        },
    }

    Ok(())
}

async fn recv(sock: &UdpSocket) -> Result<(Vec<u8>, SocketAddr), Box<dyn Error>> {
    let mut buf = [0u8;8192];
    let (amt, src) = sock.recv_from(&mut buf).await?;
    let msg_buf = &buf[0..amt];

    Ok((msg_buf.to_owned(), src))
}

async fn reciever(code: String, sock: UdpSocket, server_addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    // Send message to server
    let i_have_code = IHaveCode::new(code);
    send_msg(&sock, i_have_code, server_addr).await?;

    let (msg_buf, _from) = recv(&sock).await?;

    let ip_for_code = IpForCode::from_raw(msg_buf.as_slice())?;
    println!("file name: {}", &ip_for_code.file_name);
    println!("other ip: {}", &ip_for_code.ip);

    punch_hole(&sock, ip_for_code.ip).await?;
    
    let mut file = File::create(ip_for_code.file_name).unwrap();

    loop {
        let (msg_buf, from) = recv(&sock).await?;
        if from != ip_for_code.ip {
            continue;
        }
        file.write(&msg_buf.as_slice()).unwrap();
        println!("msg 0th: {}", msg_buf[0]);
    }

    // Ok(())
}

async fn sender(file_name: &'static str, sock: UdpSocket, server_addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    let have_file = HaveFile::new(file_name.to_string());

    send_msg(&sock, have_file, server_addr).await?;
    
    // TODO Send this once a second until it gets answer from server

    let msg_buf = loop {
        let (msg_buf, from) = recv(&sock).await?;
        if from == server_addr {
            break msg_buf;
        }
    };

    // TODO Check where the server message is comming from

    println!("you have file 0th: {}", msg_buf[0]);
    let you_have_file = YouHaveFile::from_raw(msg_buf.as_slice())?;

    let code = you_have_file.code;
    println!("Code for recv: {}", &code);

    // TODO Keep the hole punched

    let msg_buf = loop {
        let (msg_buf, from) = recv(&sock).await?;
        if from == server_addr {
            break msg_buf;
        }
    };

    println!("msg 0th: {}", msg_buf[0]);
    let file_reciever = TakerIp::from_raw(msg_buf.as_slice())?;

    println!("reciever ip: {}", file_reciever.ip);

    sock.send_to(&[0xCB, 0xCB, 65, 65], file_reciever.ip).await?;

    Ok(())
}
