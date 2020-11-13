use std::{
    io::BufRead,
    io::{BufWriter, Write},
};

use json::JsonValue;
use log::info;

pub struct Message {
    pub headers: Vec<String>,
    pub content: JsonValue,
}

impl Message {
    pub fn get_id(&self) -> Option<usize> {
        self.content["id"].as_usize()
    }

    pub fn patch_id(&mut self, id: Option<usize>) {
        if let Some(id) = id {
            self.content["id"] = json::JsonValue::Number(id.into());
        } else {
            self.content.remove("id");
        }
    }

    pub fn read_from(buf_stream: &mut dyn BufRead) -> Message {
        // info!("-- Reading headers ..");
        let headers = {
            let mut headers = Vec::new();
            let mut buf = String::new();
            loop {
                buf.clear();
                buf_stream
                    .read_line(&mut buf)
                    .expect("Error reading headers");
                // info!("!! Read -- {}", &buf);
                headers.push(buf.clone());
                if buf == "\r\n" {
                    break;
                }
            }
            headers
        };
        // info!("-- Headers read {}", headers.len());

        // info!("-- Search content length ..");
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
        // info!("-- Found content length {}", content_len);

        let data = {
            let mut buf = vec![0; content_len];
            // info!("-- Reading data { }", (&mut buf[..]).len());
            buf_stream.read_exact(&mut buf[..]).expect("Read error");
            // info!("-- Data read {}", String::from_utf8_lossy(&buf));
            buf
        };
        let json = json::parse(&String::from_utf8_lossy(&data)).unwrap();

        Message {
            headers,
            content: json,
        }
    }

    pub fn write_to<W: Write>(&self, buf_stream: &mut BufWriter<W>) {
        for header in &self.headers {
            buf_stream
                .write(header.as_bytes())
                .expect("Writing headers");
        }
        self.content.write(buf_stream).expect("Writing content");
    }
}
