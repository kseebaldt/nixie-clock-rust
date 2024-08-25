use std::sync::{Arc, Mutex};

use anyhow::Result;
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};

use drivers::config::{Config, ConfigStorage};
use esp_idf_svc::http::server::EspHttpServer;

use log::info;

const STACK_SIZE: usize = 10240;
const MAX_LEN: usize = 256;
static INDEX_HTML: &str = include_str!("../../webapp/dist/index.html");

pub fn create_server(
    config_storage: Arc<Mutex<ConfigStorage>>,
) -> Result<EspHttpServer<'static>, anyhow::Error> {
    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: STACK_SIZE,
        ..Default::default()
    };
    let mut server = EspHttpServer::new(&server_configuration)?;
    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?
            .write_all(INDEX_HTML.as_bytes())
            .map(|_| ())
    })?;
    let storage2 = config_storage.clone();
    server.fn_handler("/config", Method::Get, move |req| {
        let mut s = storage2.lock().unwrap();
        let config = s.load().unwrap();
        let j = serde_json::to_string(&config).unwrap();

        req.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?
            .write_all(j.as_bytes())
            .map(|_| ())
    })?;
    let storage3 = config_storage.clone();
    server.fn_handler::<anyhow::Error, _>("/config", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        let mut s = storage3.lock().unwrap();

        if len > MAX_LEN {
            req.into_status_response(413)?
                .write_all("Request too big".as_bytes())?;
            return Ok(());
        }

        let mut buf = vec![0; len];
        req.read_exact(&mut buf)?;

        if let Ok(config) = serde_json::from_slice::<Config>(&buf) {
            info!("Config: {:?}", config);

            match s.save(&config) {
                Ok(_) => {
                    req.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?
                        .write_all("{{\"status\":\"ok\"}}".as_bytes())?;
                }
                Err(_) => {
                    req.into_status_response(500)?
                        .write_all("JSON error".as_bytes())?;
                }
            };
        } else {
            req.into_status_response(500)?
                .write_all("JSON error".as_bytes())?;
        }

        Ok(())
    })?;
    Ok(server)
}
