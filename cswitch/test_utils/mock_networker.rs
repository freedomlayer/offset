#![allow(unused)]
extern crate ring;
extern crate futures;
extern crate tokio_core;
extern crate futures_mutex;
extern crate pretty_env_logger;

extern crate cswitch;

use std::io;
use std::time;
use std::net::SocketAddr;
use std::collections::HashMap;

use futures::sync::mpsc;
use futures::{Future, IntoFuture, Stream, Sink};

use futures_mutex::FutMutex;

use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use ring::test::rand::FixedByteRandom;

use cswitch::channeler::channel::Channel;
use cswitch::channeler::ChannelerNeighbor;
use cswitch::close_handle::create_close_handle;
use cswitch::security_module::create_security_module;
use cswitch::crypto::identity::{SoftwareEd25519Identity, PublicKey, Identity};

use cswitch::timer::TimerModule;
use cswitch::channeler::Channeler;

use cswitch::inner_messages::{
    ChannelerAddress, ChannelerNeighborInfo,
    ChannelerToNetworker, NetworkerToChanneler
};

fn main() {
    pretty_env_logger::init().unwrap();

    let mut line = String::new();

    println!("Enter your fixed byte random value: ");

    io::stdin().read_line(&mut line).unwrap();
    let fixed_byte = line.trim().parse::<u8>().unwrap();

    println!("Enter addr you want to listen: ");

    line.clear();
    io::stdin().read_line(&mut line).unwrap();
    let addr = line.trim().parse().unwrap();

    // Bootstrap
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    // SecurityModule
    let fixed_rand = FixedByteRandom { byte: fixed_byte };
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
    let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

    let (sm_handle, mut sm) = create_security_module(identity);
    let sm_client = sm.new_client();

    // Timer & Channeler
    let (networker_sender, channeler_receiver) = mpsc::channel::<ChannelerToNetworker>(0);
    let (mut channeler_sender, networker_receiver) = mpsc::channel::<NetworkerToChanneler>(0);

    let mut timer_module = TimerModule::new(time::Duration::from_millis(100));

    let (_channeler_close_handle, channeler) = Channeler::new(
        &addr,
        &handle,
        timer_module.create_client(),
        networker_sender,
        networker_receiver,
        sm_client,
    );

    let mock_networker_receiver_part = channeler_receiver.map_err(|_| ()).for_each(|msg| {
        match msg {
            ChannelerToNetworker::ChannelOpened(_) => {
                println!("Recv ChannelOpened message");
            }
            ChannelerToNetworker::ChannelClosed(_) => {
                println!("Recv ChannelClosed message");
            }
            ChannelerToNetworker::ChannelMessageReceived(msg) => {
                println!("ChannelMessageReceived:");

                let mut raw_msg = msg.message_content;
                let msg_str = unsafe { ::std::str::from_utf8_unchecked_mut(&mut raw_msg) };

                println!("{}", msg_str);
            }
        }
        Ok(())
    });

    // Hook the stdin to accept command
    let (stdin_tx, stdin_rx) = mpsc::channel(0);

    // Currently supported command
    //
    // 1. add [fixed byte: u8] <addr>
    //     add neighbor, generate using FixedByteRandom
    // 2. del [fixed byte: u8]
    //     del neighbor, generate using FixedByteRandom
    // 3. send [fixed byte: u8]  [token: u32]  [content]
    //     send message to neighbor via token channel with content
    //
    // Each command split by '\n'
    let mock_networker_sender_part = stdin_rx
        .map(move |command: String| {
            (command, channeler_sender.clone())
        })
        .for_each(|(command, mut channeler_sender)| {
            println!("receive a command");
            let items = command.as_str().split_whitespace()
                .map(String::from)
                .collect::<Vec<String>>();

            match items[0].as_str().trim() {
                "add" => {
                    if items.len() > 3 {
                        println!("Too many args.");
                        print_usage();
                    } else {
                        let fixed_byte = items[1].parse::<u8>().unwrap();

                        let fixed_rand = FixedByteRandom { byte: fixed_byte };
                        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
                        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

                        let neighbor_public_key = identity.get_public_key();

                        let addr = if items.len() > 2 {
                            Some(items[2].parse::<SocketAddr>().unwrap())
                        } else {
                            None
                        };

                        let neighbor_info = ChannelerNeighborInfo {
                            neighbor_address: ChannelerAddress {
                                socket_addr: addr,
                                neighbor_public_key,
                            },
                            max_channels: 8u32,
                        };

                        let message = NetworkerToChanneler::AddNeighbor {
                            neighbor_info,
                        };

                        if channeler_sender.try_send(message).is_err() {
                            println!("Failed to send [add] command.");
                        }
                    }
                }
                "del" => {
                    if items.len() > 2 {
                        println!("Too many args.");
                        print_usage();
                    } else {
                        let fixed_byte = items[1].parse::<u8>().unwrap();

                        let fixed_rand = FixedByteRandom { byte: fixed_byte };
                        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
                        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

                        let neighbor_public_key = identity.get_public_key();

                        let message = NetworkerToChanneler::RemoveNeighbor {
                            neighbor_public_key,
                        };

                        if channeler_sender.try_send(message).is_err() {
                            println!("Failed to send [del] command.");
                        }
                    }
                }
                "send" => {
                    if items.len() < 4 {
                        println!("Too few args.");
                        print_usage();
                    } else {
                        let fixed_byte = items[1].parse::<u8>().unwrap();

                        let fixed_rand = FixedByteRandom { byte: fixed_byte };
                        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
                        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

                        let neighbor_public_key = identity.get_public_key();

                        let token = items[2].parse::<u32>().unwrap();

                        let content = Vec::from(items[3..].join(" ").as_bytes());

                        let message = NetworkerToChanneler::SendChannelMessage {
                            token,
                            neighbor_public_key,
                            content,
                        };

                        if channeler_sender.try_send(message).is_err() {
                            println!("Failed to send [send] command.");
                        }
                    }
                }

                _ => {
                    println!("unsupported command!");
                }
            }

            Ok(())
        });

    handle.spawn(sm.then(|_| Ok(())));
    handle.spawn(timer_module.map_err(|_| ()));
    handle.spawn(mock_networker_receiver_part);
    handle.spawn(mock_networker_sender_part);

    // Hook the stdin
    ::std::thread::spawn(|| read_stdin(stdin_tx));

    core.run(channeler).unwrap();
}

fn print_usage() {
    let usage = r#"
====================================================================
Currently supported command:

1. add [fixed byte: u8] <addr>
       add neighbor, generate using FixedByteRandom
2. del [fixed byte: u8]
       del neighbor, generate using FixedByteRandom
3. send [fixed byte: u8]  [token: u32]  [content]
       send message to neighbor via token channel with content
====================================================================
    "#;

    println!("{}", usage);
}


fn read_stdin(mut tx: mpsc::Sender<String>) {
    let mut stdin = io::stdin();
    loop {
        let mut line = String::new();

        let n = stdin.read_line(&mut line).unwrap();
        line.truncate(n);

        tx = match tx.send(line).wait() {
            Ok(tx) => tx,
            Err(_) => {
                println!("[stdio reader]: send failed");
                break;
            },
        };
    }
}
