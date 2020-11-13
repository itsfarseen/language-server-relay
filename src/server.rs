use crate::{client_handler::handle_client, message::Message};
use dashmap::DashMap;
use log::info;
use std::sync::Arc;
use std::{io::BufReader, io::BufWriter, sync::atomic::AtomicUsize, sync::Mutex};
use std::{
    io::Write,
    sync::mpsc::{Receiver, Sender},
};
use std::{net::TcpListener, process::Command, sync::atomic::Ordering};
use std::{process::Stdio, sync::mpsc, thread};

pub fn start() -> Result<(), std::io::Error> {
    let lserver = "/home/farzeen/.local/bin/rust-analyzer";
    let listener = TcpListener::bind("127.0.0.1:7777")?;
    let lang_server = run_lang_server(lserver);

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(stream) = stream {
                let client_handle = lang_server.new_client();
                handle_client(client_handle, stream);
            } else {
                continue;
            };
        }
    });
    Ok(())
}

#[derive(Clone)]
struct LangServer {
    last_client_id: Arc<AtomicUsize>,
    request_id_map_prototype: RequestIdMap,
    clients: Arc<DashMap<usize, Mutex<ClientHandleInternal>>>,
    common_serverin: Sender<Message>,
}

impl LangServer {
    fn new_client(&self) -> ClientHandle {
        let serverin = self.common_serverin.clone();
        let (serverout_internal, serverout) = mpsc::channel();
        let client_id = self.last_client_id.fetch_add(1, Ordering::SeqCst);

        let mut request_id_map = self.request_id_map_prototype.clone();
        request_id_map.client_id = client_id;

        let client = ClientHandle {
            request_id_map,
            serverin,
            serverout,
        };
        let client_internal = ClientHandleInternal {
            serverout: serverout_internal,
        };

        self.clients.insert(client_id, Mutex::new(client_internal));
        client
    }
}

#[derive(Clone)]
pub struct RequestIdMap {
    last_request_id: Arc<AtomicUsize>,
    client_id: usize,
    request_id_client_id_map: Arc<DashMap<usize, usize>>,
}

impl RequestIdMap {
    pub fn new_request_id(&self) -> usize {
        let req_id = self.last_request_id.fetch_add(1, Ordering::SeqCst);
        self.request_id_client_id_map.insert(req_id, self.client_id);
        req_id
    }

    fn get_client_id(&self, request_id: usize) -> usize {
        *self.request_id_client_id_map.get(&request_id).unwrap()
    }
}

pub struct ClientHandle {
    pub request_id_map: RequestIdMap,
    pub serverin: Sender<Message>,
    pub serverout: Receiver<Message>,
}

struct ClientHandleInternal {
    serverout: Sender<Message>,
}

fn run_lang_server(exec_path: &str) -> LangServer {
    let mut child = Command::new(exec_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect(&format!("Failed to launch {}", exec_path));
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let mut stdin_buf = BufWriter::new(stdin);
    let mut stdout_buf = BufReader::new(stdout);

    let (serverin, serverin_internal) = mpsc::channel();

    let request_id_map_prototype = RequestIdMap {
        last_request_id: Arc::new(AtomicUsize::new(0)),
        client_id: 0,
        request_id_client_id_map: Arc::new(DashMap::new()),
    };

    let lang_server = LangServer {
        last_client_id: Arc::new(AtomicUsize::new(0)),
        common_serverin: serverin,
        clients: Arc::new(DashMap::new()),
        request_id_map_prototype,
    };

    std::thread::spawn(move || loop {
        let req = serverin_internal.recv().unwrap();
        info!("run_lang_server: message on serverin_internal");
        req.write_to(&mut stdin_buf);
        stdin_buf.flush().unwrap();
    });

    let lang_server_ = lang_server.clone();
    std::thread::spawn(move || {
        let lang_server = lang_server_;
        loop {
            let resp = Message::read_from(&mut stdout_buf);
            info!("run_lang_server: message on lang server stdout");
            let req_id = resp.get_id().unwrap();
            let client_id = match lang_server
                .request_id_map_prototype
                .request_id_client_id_map
                .get(&req_id)
            {
                None => {
                    info!("run_lang_server: skipping (client_id not found in request_id_map)");
                    continue;
                }
                Some(client_id) => *client_id,
            };
            let client_serverout = match lang_server.clients.get(&client_id) {
                None => {
                    info!("run_lang_server: skipping (client_handle_internal not found for client_id)");
                    continue;
                }
                Some(client) => {
                    info!("run_lang_server: Locking");
                    let client = client.lock().unwrap();
                    client.serverout.clone()
                }
            };
            info!("run_lang_server: relaying to client_serverout");
            client_serverout.send(resp).unwrap();
        }
    });

    lang_server
}
