use crate::message::Message;
use log::info;
use std::{
    io::Write,
    io::{BufReader, BufWriter},
    net::TcpStream,
};

pub fn client() {
    let stream = TcpStream::connect("localhost:7777").unwrap();
    info!("Connected relay local {:?}", stream.local_addr());

    let mut tcpin = BufWriter::new(stream.try_clone().unwrap());
    let mut tcpout = BufReader::new(stream);

    let thread1 = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let stdin = stdin.lock();
        let mut stdin = BufReader::new(stdin);

        loop {
            let req = Message::read_from(&mut stdin);
            info!("Relay - Incoming request: {} bytes", &req.headers[0]);
            info!("Relay - Contents: {}", req.content.pretty(2));
            req.write_to(&mut tcpin);
            tcpin.flush().unwrap();
        }
    });

    let thread2 = std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let stdout = stdout.lock();
        let mut stdout = BufWriter::new(stdout);

        loop {
            let resp = Message::read_from(&mut tcpout);
            info!("Relay - Response: {} bytes", &resp.headers[0]);
            info!("Relay - Contents: {}", resp.content.pretty(2));
            resp.write_to(&mut stdout);
            stdout.flush().unwrap();
        }
    });

    thread1.join().unwrap();
    thread2.join().unwrap();
}
