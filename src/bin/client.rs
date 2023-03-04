extern crate libc;

use std::io::{BufRead, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::{io, mem, thread};
use std::os::fd::{OwnedFd};

const MULTICAST_PORT: u16 = 9000;

fn read_multiple_lines_from_stdin() -> String {
    println!("Enter message (empty line to send):");
    let mut lines = io::stdin().lock().lines();
    let mut message = String::new();
    while let Some(result) = lines.next() {
        let line = result.unwrap();

        if line.len() == 0 {
            break;
        }

        if message.len() > 0 {
            message.push_str("\n");
        }
        message.push_str(&line);
    }
    println!("Message read from stdin");
    message
}

fn udp_reader(udp_socket: &UdpSocket) {
    let mut buf = [0; 1024];
    loop {
        let result = udp_socket.recv_from(&mut buf);

        if let Err(e) = result {
            println!("Error reading udp message: {}", e);
            continue;
        }
        let (bytes_read, _) = result.unwrap();

        let message = String::from_utf8_lossy(&buf[..bytes_read]);
        println!("{}", message);
    }
}

fn create_multicast_socket() -> Result<UdpSocket, io::Error> {
    unsafe {
        let socket = libc::socket(
            libc::AF_INET,
            libc::SOCK_DGRAM,
            libc::IPPROTO_UDP,
        );
        let optval: libc::c_int = 1;
        let ret = libc::setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &optval as *const _ as *const libc::c_void,
            mem::size_of_val(&optval) as libc::socklen_t,
        );
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }
        let ret = libc::setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &optval as *const _ as *const libc::c_void,
            mem::size_of_val(&optval) as libc::socklen_t,
        );
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        // bind socket
        let mut addr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: MULTICAST_PORT.to_be(),
            sin_addr: libc::in_addr {
                s_addr: libc::INADDR_ANY,
            },
            sin_zero: [0; 8],
        };
        let ret = libc::bind(
            socket,
            &mut addr as *mut _ as *mut libc::sockaddr,
            mem::size_of_val(&addr) as libc::socklen_t,
        );

        if ret != 0 {
            return Err(io::Error::last_os_error());
        }
        let owned_fd: OwnedFd = std::os::unix::io::FromRawFd::from_raw_fd(socket);
        Ok(UdpSocket::from(owned_fd))
    }
}

fn main() -> io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8081")?;

    let mut stream_clone = stream.try_clone()?;

    let udp_socket = UdpSocket::bind("127.0.0.1:0")?;

    let multicast_ip = Ipv4Addr::new(239, 255, 255, 250);

    let multicast_udp_socket = create_multicast_socket()?;


    let udp_socket_clone = udp_socket.try_clone()?;
    let multicast_udp_socket_clone = multicast_udp_socket.try_clone()?;

    multicast_udp_socket.join_multicast_v4(&multicast_ip, &Ipv4Addr::new(0, 0, 0, 0))?;
    multicast_udp_socket.set_multicast_ttl_v4(1)?;

    udp_socket.connect("127.0.0.1:8081").unwrap_or_else(|err| {
        println!("Error connecting to server: {}", err);
        return;
    });

    println!("Client started. Enter username:");

    // read username from stdin
    let mut username = String::new();
    io::stdin().read_line(&mut username).unwrap_or_else(|err| {
        panic!("Failed to read username: {}", err);
    });

    // write username and hello message to server

    stream.write(format!("{}: {}", &username, "Hello from client").as_bytes()).unwrap_or_else(|err| {
        panic!("Failed to write to server: {}", err);
    });

    // spawn thread to read messages from server

    thread::spawn(move || {
        let mut buf = [0; 1024];
        loop {
            let bytes_read = stream_clone.read(&mut buf);

            if bytes_read.is_err() {
                std::println!("Error reading message: {}", bytes_read.err().unwrap());
                continue;
            }

            let bytes_read = bytes_read.unwrap();

            if bytes_read == 0 {
                std::println!("Server closed connection");
                return;
            }

            let message = String::from_utf8_lossy(&buf[..bytes_read]).to_string();

            println!("{}", message);
        }
    });

    thread::spawn(move || {
        udp_reader(&udp_socket_clone);
    });

    thread::spawn(move || {
        udp_reader(&multicast_udp_socket_clone);
    });

    loop {
        let mut command = String::new();

        if let Err(e) = std::io::stdin().read_line(&mut command) {
            println!("Error reading command: {}", e);
            continue;
        }

        match command.trim() {
            "s" => {
                // send message to server
                let message = read_multiple_lines_from_stdin();
                let bytes_written = stream.write(format!("{}: {}", &username, &message).as_bytes());
                if bytes_written.is_err() {
                    println!("Error writing message: {}", bytes_written.err().unwrap());
                    continue;
                }

                println!("Message sent: {}", message);
            }
            "q" => {
                // quit
                break;
            }
            "u" => {
                //read message and send it through udp
                let msg = read_multiple_lines_from_stdin();
                let bytes_written = udp_socket.send_to(msg.as_bytes(), "127.0.0.1:8081");
                if bytes_written.is_err() {
                    println!("Error writing message: {}", bytes_written.err().unwrap());
                    continue;
                }
            }
            "m" => {
                // multicast message
                let msg = read_multiple_lines_from_stdin();
                let bytes_written = multicast_udp_socket.send_to(msg.as_bytes(), SocketAddr::new(multicast_ip.into(), MULTICAST_PORT));

                if bytes_written.is_err() {
                    println!("Error writing message: {}", bytes_written.err().unwrap());
                    continue;
                }
            }
            _ => {
                println!("Unknown command");
            }
        }

        // write message to server
    }
    Ok(())
}