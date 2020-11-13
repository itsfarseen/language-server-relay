use std::{
    io::Write,
    io::{BufReader, BufWriter},
    net::TcpStream,
    sync::Arc,
};

use dashmap::DashMap;
use log::info;

use crate::{message::Message, server::ClientHandle};

pub fn handle_client(client_handle: ClientHandle, stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = BufWriter::new(stream);

    let ClientHandle {
        request_id_map,
        serverin,
        serverout,
    } = client_handle;

    let req_id_patch_map = Arc::new(DashMap::new());
    let req_id_patch_map_ = req_id_patch_map.clone();
    std::thread::spawn(move || {
        let req_id_patch_map = req_id_patch_map_;
        loop {
            let mut req = Message::read_from(&mut reader);
            info!("handle_client: message on tcp input stream");
            let new_req_id = request_id_map.new_request_id();
            let old_req_id = req.get_id();
            req_id_patch_map.insert(new_req_id, old_req_id);
            req.patch_id(Some(new_req_id));
            serverin.send(req).unwrap();
        }
    });

    std::thread::spawn(move || loop {
        let mut resp = serverout.recv().unwrap();
        info!("handle_client: message on client serverout");
        let new_req_id = resp.get_id().unwrap();
        let old_req_id = {
            let (_key, val) = req_id_patch_map.remove(&new_req_id).unwrap();
            val
        };
        resp.patch_id(old_req_id);
        resp.write_to(&mut writer);
        writer.flush().unwrap();
    });
}
