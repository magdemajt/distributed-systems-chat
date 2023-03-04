use std::io::prelude::*;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{mpsc};
use std::thread;

#[derive(Debug)]
struct Client {
    name: String,
    stream: TcpStream,
}

impl PartialEq for Client {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Client {
    fn get_mut_stream(&mut self) -> &mut TcpStream {
        &mut self.stream
    }
}

impl Eq for Client {}

struct ClientMessage {
    client: Client,
    message: String,
}

struct UdpMessage {
    socket_addr: SocketAddr,
    message: String,
}


fn message_sender_handler(rx: mpsc::Receiver<ClientMessage>) {
    let mut clients: Vec<Client> = Vec::new();
    loop {
        let message = rx.recv();
        if message.is_err() {
            std::println!("Error receiving message: {}", message.err().unwrap());
            continue;
        }
        let message = message.unwrap();
        if !clients.contains(&message.client) {
            let cloned_socket = message.client.stream.try_clone();
            if cloned_socket.is_err() {
                std::println!("Error cloning socket: {}", cloned_socket.err().unwrap());
                continue;
            }
            let cloned_client = Client {
                name: message.client.name.clone(),
                stream: cloned_socket.unwrap(),
            };
            clients.push(cloned_client);
        }
        clients.iter_mut().filter(|client| client != &&message.client).for_each(|client| {
            let stream = client.get_mut_stream();
            let message = format!("{}: {}", message.client.name, message.message);
            let result = stream.write(message.as_bytes());
            if result.is_err() {
                std::println!("Error sending message to client: {}", result.err().unwrap());
            }
        });
    }
}

fn connection_handler(tx: mpsc::Sender<ClientMessage>, mut stream: TcpStream) {
    loop {
        let stream_clone = stream.try_clone();

        if stream_clone.is_err() {
            std::println!("Error cloning stream: {}", stream_clone.err().unwrap());
            continue;
        }

        let mut buffer = [0 as u8; 1024];

        let size = stream.read(&mut buffer);

        if size.is_err() {
            std::println!("Error reading username: {}", size.err().unwrap());
            continue;
        }
        let size = size.unwrap();

        let msg_with_username = String::from_utf8_lossy(&buffer[..size]).to_string();

        if size == 0 {
            std::println!("Client disconnected");
            break;
        }

        // get username from string pattern {}:{} where {} is username and {} is message
        let username = msg_with_username.split(":").next();
        if username.is_none() {
            std::println!("Error getting username");
            continue;
        }
        let username = username.unwrap().trim().to_string();
        let msg = msg_with_username.split(":").nth(1);

        let msg = msg.unwrap_or_else(|| {
            std::println!("Error getting message");
            ""
        }).trim().to_string();

        let client = Client {
            name: username,
            stream: stream_clone.unwrap(),
        };

        std::println!("{}: {}", &client.name, &msg);

        let client_message = ClientMessage {
            client,
            message: msg,
        };

        tx.send(client_message).unwrap_or_else(|err| {
            std::println!("Error sending message: {}", err);
        });
    }
}

fn udp_handler(udp_socket: &UdpSocket, tx: mpsc::Sender<UdpMessage>) {
    loop {
        let mut buf = [0 as u8; 1024];
        let res = udp_socket.recv_from(&mut buf);

        if let Err(e) = res {
            std::println!("Error receiving udp message: {}", e);
            continue;
        }

        let (size, src) = res.unwrap();

        let message = String::from_utf8_lossy(&buf[..size]).to_string();

        println!("Received udp message: {}", &message);

        let cloned_socket = udp_socket.try_clone();

        if let Err(e) = cloned_socket {
            std::println!("Error cloning udp socket: {}", e);
            continue;
        }

        let udp_message = UdpMessage {
            socket_addr: src,
            message,
        };

        tx.send(udp_message).unwrap_or_else(|err| {
            std::println!("Error sending udp message: {}", err);
        });
    }
}

fn udp_sender(udp_socket: &UdpSocket, rx: mpsc::Receiver<UdpMessage>) {
    let mut sockets = Vec::new();

    loop {
        let message = rx.recv();

        if let Err(e) = message {
            std::println!("Error receiving udp message: {}", e);
            continue;
        }
        let message = message.unwrap();

        println!("Sending udp message: {}", &message.message);

        if !sockets.contains(&message.socket_addr) {
            sockets.push(message.socket_addr.clone());
        }

        sockets.iter().filter(|socket| socket != &&message.socket_addr).for_each(|socket| {
            println!("Sending udp message to: {}", socket);
            let result = udp_socket.send_to(message.message.as_bytes(), socket);
            if result.is_err() {
                std::println!("Error sending udp message: {}", result.err().unwrap());
            }
        });
    }
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:8081").unwrap_or_else(|err| {
        panic!("Failed to bind to port 8081: {}", err);
    });

    let udp_socket = UdpSocket::bind("127.0.0.1:8081").unwrap_or_else(|err| {
        panic!("Failed to create udp socket on port 8081: {}", err);
    });

    let (tx, rx) = mpsc::channel::<ClientMessage>();

    let (utx, urx) = mpsc::channel::<UdpMessage>();

    let udp_clone = udp_socket.try_clone().unwrap_or_else(|err| {
        panic!("Failed to clone udp socket: {}", err);
    });

    thread::spawn(move || {
        udp_handler(&udp_socket, utx);
    });

    thread::spawn(move || {
        udp_sender(&udp_clone, urx);
    });

    thread::spawn(move || {
        message_sender_handler(rx);
    });

    println!("Server started on port 8081 (tcp and udp)");

    for stream in listener.incoming() {
        let tx_clone = tx.clone();
        match stream {
            Err(e) => println!("Failed to connect: {}", e),
            Ok(stream) => {
                thread::spawn(move || {
                    connection_handler(tx_clone, stream);
                });
            }
        };
    }
}
