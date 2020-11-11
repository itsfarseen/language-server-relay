use log::info;
use simplelog;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    io::{BufReader, BufWriter, Read},
    net::TcpStream,
};

use main_daemon::Message;

fn main() {
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        OpenOptions::new()
            .create(true)
            .append(true)
            .open("/home/farzeen/relay.log")
            .unwrap(),
    )
    .unwrap();
    info!("Starting daemon...");
    let status = main_daemon::start();
    info!("Dameon started: {}", status.is_ok());
    let stdin = std::io::stdin();
    let stdin = stdin.lock();

    let stdout = std::io::stdout();
    let stdout = stdout.lock();

    info!("Starting relay ..");
    relay(BufReader::new(stdin), BufWriter::new(stdout));
}

mod main_daemon {
    use log::info;
    use std::io::BufReader;
    use std::net::{TcpListener, TcpStream};
    use std::process::Command;
    use std::process::Stdio;
    use std::sync::mpsc::{Receiver, Sender};
    use std::{io::BufWriter, sync::Mutex};
    use std::{
        io::{BufRead, Write},
        sync::Arc,
    };

    pub struct Message {
        pub headers: Vec<String>,
        pub content: Vec<u8>,
    }

    impl Message {
        pub fn read_from(buf_stream: &mut dyn BufRead) -> Message {
            info!("-- Reading headers ..");
            let headers = {
                let mut headers = Vec::new();
                let mut buf = String::new();
                loop {
                    buf.clear();
                    buf_stream
                        .read_line(&mut buf)
                        .expect("Error reading headers");
                    info!("!! Read -- {}", &buf);
                    headers.push(buf.clone());
                    if buf == "\r\n" {
                        break;
                    }
                }
                headers
            };
            info!("-- Headers read {}", headers.len());

            info!("-- Search content length ..");
            let content_len = {
                let mut content_len = None;
                for header in &headers {
                    if header.starts_with("Content-Length:") {
                        let content_len_str = header["Content-Length:".len()..].trim();
                        let content_len_usize: usize =
                            content_len_str.parse().expect("Invalid integer");
                        content_len = Some(content_len_usize);
                    }
                }
                content_len.expect("Content-Length header not found")
            };
            info!("-- Found content length {}", content_len);

            let data = {
                let mut buf = vec![0; content_len];

                info!("-- Reading data { }", (&mut buf[..]).len());
                buf_stream.read_exact(&mut buf[..]).expect("Read error");
                info!("-- Data read {}", String::from_utf8_lossy(&buf));
                buf
            };
            Message {
                headers,
                content: data,
            }
        }
        pub fn write_to<W: Write>(&self, buf_stream: &mut BufWriter<W>) {
            for header in &self.headers {
                buf_stream
                    .write(header.as_bytes())
                    .expect("Writing headers");
            }
            buf_stream.write(&self.content).expect("Writing content");
        }
    }

    pub fn start() -> Result<(), std::io::Error> {
        let lserver = "/home/farzeen/.local/bin/rust-analyzer";
        let listener = TcpListener::bind("127.0.0.1:7777")?;
        let (sender, receiver) = run_lang_server(lserver);
        let sender_receiver = Arc::new(Mutex::new((sender, receiver)));

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let s_r = sender_receiver.clone();
                if let Ok(stream) = stream {
                    std::thread::spawn(move || {
                        handle_client(s_r, stream);
                    });
                } else {
                    continue;
                };
            }
        });
        Ok(())
    }

    fn run_lang_server(exec_path: &str) -> (Sender<Message>, Receiver<Message>) {
        let mut child = Command::new(exec_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect(&format!("Failed to launch {}", exec_path));
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (serverin, serverin_rx) = std::sync::mpsc::channel();
        let (serverout_tx, serverout) = std::sync::mpsc::channel();

        let mut stdin_buf = BufWriter::new(stdin);
        let mut stdout_buf = BufReader::new(stdout);

        std::thread::spawn(move || loop {
            let req: Message = serverin_rx.recv().unwrap();
            info!("Main daemon - Incoming request: {} bytes", &req.headers[0]);
            info!(
                "Main daemon - Contents: {}",
                String::from_utf8_lossy(&req.content)
            );
            req.write_to(&mut stdin_buf);
            stdin_buf.flush().unwrap();
            let resp = Message::read_from(&mut stdout_buf);
            info!("Main daemon - Response: {} bytes", &resp.headers[0]);
            info!(
                "Main daemon - Contents: {}",
                String::from_utf8_lossy(&resp.content)
            );
            serverout_tx.send(resp).unwrap();
        });
        (serverin, serverout)
    }

    fn handle_client(
        sender_receiver: Arc<Mutex<(Sender<Message>, Receiver<Message>)>>,
        stream: TcpStream,
    ) {
        let mut stream = Some(stream);

        loop {
            let mut buf_reader = BufReader::new(stream.take().unwrap());
            let req = Message::read_from(&mut buf_reader);
            info!("TcpHandler - Incoming request: {} bytes", &req.headers[0]);
            info!(
                "TcpHandler - Contents: {}",
                String::from_utf8_lossy(&req.content)
            );
            let _ = stream.replace(buf_reader.into_inner());

            let lock = sender_receiver.lock().unwrap();
            let sender = &lock.0;
            let receiver = &lock.1;
            sender.send(req).unwrap();
            let resp = receiver.recv().unwrap();
            info!("TcpHandler - Response: {} bytes", &resp.headers[0]);
            info!(
                "TcpHandler - Contents: {}",
                String::from_utf8_lossy(&resp.content)
            );
            drop(lock);

            let mut buf_writer = BufWriter::new(stream.take().unwrap());
            resp.write_to(&mut buf_writer);
            let _ = stream.replace(buf_writer.into_inner().unwrap());
        }
    }
}

fn relay(mut stdin: BufReader<impl Read>, mut stdout: BufWriter<impl Write>) {
    let stream = TcpStream::connect("localhost:7777").unwrap();
    info!("Connected relay local {:?}", stream.local_addr());
    let mut stream = Some(stream);
    loop {
        let req = Message::read_from(&mut stdin);
        info!("Relay - Incoming request: {} bytes", &req.headers[0]);
        info!(
            "Relay - Contents: {}",
            String::from_utf8_lossy(&req.content)
        );

        let mut buf_writer = BufWriter::new(stream.take().unwrap());
        req.write_to(&mut buf_writer);
        let _ = stream.replace(buf_writer.into_inner().unwrap());

        let mut buf_reader = BufReader::new(stream.take().unwrap());
        let resp = Message::read_from(&mut buf_reader);
        info!("Relay - Response: {} bytes", &resp.headers[0]);
        info!(
            "Relay - Contents: {}",
            String::from_utf8_lossy(&resp.content)
        );
        let _ = stream.replace(buf_reader.into_inner());

        resp.write_to(&mut stdout);
        stdout.flush().unwrap();
    }
}

// TODO -
// Make relay full duplex. Take data from r-a back to client even if
// client doesn't send a request.
