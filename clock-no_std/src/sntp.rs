//! SNTP (Simple Network Time Protocol) client for ESP32
//! 
//! This module provides functionality to synchronize time with NTP servers
//! using embassy-net's UDP socket capabilities.

use embassy_net::{Stack, udp::UdpSocket};
use embassy_time::{Duration, Timer};
use esp_println::println;
use chrono::{DateTime, Utc};
use chrono_tz::{Tz, UTC};
use smoltcp::storage::PacketMetadata;
use embassy_net::udp::UdpMetadata;
use embassy_futures::select::{select, Either};

/// SNTP client for time synchronization
pub struct SntpClient<'a> {
    stack: &'a Stack<'a>,
}

impl<'a> SntpClient<'a> {
    pub fn new(stack: &'a Stack<'a>) -> Self {
        Self { stack }
    }

    /// Synchronize time with an NTP server using raw UDP
    pub async fn sync_time(&self, server: &str) -> Result<DateTime<Utc>, SntpError> {
        println!("SNTP: Starting time sync with server: {}", server);

        // Create UDP socket with proper metadata buffers
        let mut rx_meta = [PacketMetadata::<UdpMetadata>::EMPTY; 16];
        let mut rx_buffer = [0; 1024];
        let mut tx_meta = [PacketMetadata::<UdpMetadata>::EMPTY; 16];
        let mut tx_buffer = [0; 1024];
        let mut socket = UdpSocket::new(*self.stack, &mut rx_meta, &mut rx_buffer, &mut tx_meta, &mut tx_buffer);

        // Resolve server address
        let server_addr = match self.stack.dns_query(server, embassy_net::dns::DnsQueryType::A).await {
            Ok(addrs) => {
                if addrs.is_empty() {
                    return Err(SntpError::DnsResolution);
                }
                addrs[0]
            }
            Err(_) => return Err(SntpError::DnsResolution),
        };

        println!("SNTP: Resolved {} to {}", server, server_addr);

        // Bind to local port
        socket.bind(0).map_err(|_| SntpError::SocketBind)?;

        // Simple NTP request packet (48 bytes)
        let mut request = [0u8; 48];
        request[0] = 0x1B; // LI=0, VN=3, Mode=3 (client)

        // Send request to NTP server (port 123)
        socket
            .send_to(&request, (server_addr, 123))
            .await
            .map_err(|_| SntpError::SendRequest)?;

        println!("SNTP: Request sent to {}:123", server_addr);

        // Wait for response with timeout
        let mut response = [0u8; 48];
        let timeout_duration = Duration::from_secs(5);
        
        let (response_len, _remote_addr) = match select(
            socket.recv_from(&mut response),
            Timer::after(timeout_duration),
        ).await {
            Either::First(result) => result.map_err(|_| SntpError::ReceiveResponse)?,
            Either::Second(_) => return Err(SntpError::Timeout),
        };

        if response_len < 48 {
            return Err(SntpError::InvalidResponse);
        }

        println!("SNTP: Received response ({} bytes)", response_len);

        // Parse NTP timestamp from bytes 40-47 (transmit timestamp)
        let ntp_timestamp = u64::from_be_bytes([
            response[40], response[41], response[42], response[43],
            response[44], response[45], response[46], response[47],
        ]);
        
        // Convert NTP timestamp to Unix timestamp
        // NTP epoch: Jan 1, 1900; Unix epoch: Jan 1, 1970
        // Difference: 70 years = 2,208,988,800 seconds
        const NTP_UNIX_OFFSET: u64 = 2_208_988_800;
        
        if ntp_timestamp < NTP_UNIX_OFFSET {
            return Err(SntpError::InvalidTimestamp);
        }
        
        let unix_timestamp = ntp_timestamp - NTP_UNIX_OFFSET;
        
        // Convert to DateTime<Utc>
        let datetime = DateTime::from_timestamp(unix_timestamp as i64, 0)
            .ok_or(SntpError::InvalidTimestamp)?;

        println!("SNTP: Time synchronized: {}", datetime);
        
        Ok(datetime)
    }

    /// Sync with default NTP servers
    pub async fn sync_with_default_servers(&self) -> Result<DateTime<Utc>, SntpError> {
        let servers = ["pool.ntp.org", "time.nist.gov", "time.google.com"];
        
        for server in servers.iter() {
            println!("SNTP: Trying server: {}", server);
            match self.sync_time(server).await {
                Ok(time) => return Ok(time),
                Err(e) => {
                    println!("SNTP: Failed to sync with {}: {:?}", server, e);
                    Timer::after(Duration::from_secs(1)).await;
                }
            }
        }
        
        Err(SntpError::AllServersFailed)
    }
}

#[derive(Debug)]
pub enum SntpError {
    DnsResolution,
    SocketBind,
    SendRequest,
    ReceiveResponse,
    Timeout,
    InvalidResponse,
    InvalidTimestamp,
    AllServersFailed,
}

/// Global time state with timezone support
pub struct TimeManager {
    last_sync: Option<DateTime<Utc>>,
    timezone: heapless::String<32>,
    parsed_timezone: Tz,
}

impl TimeManager {
    pub fn new() -> Self {
        Self {
            last_sync: None,
            timezone: heapless::String::try_from("UTC").unwrap(),
            parsed_timezone: UTC,
        }
    }

    pub fn set_timezone(&mut self, tz: &str) -> Result<(), ()> {
        // Parse timezone string
        let parsed_tz = match tz.parse::<Tz>() {
            Ok(tz) => tz,
            Err(_) => {
                println!("TimeManager: Invalid timezone '{}', keeping current", tz);
                return Err(());
            }
        };
        
        self.timezone = heapless::String::try_from(tz).map_err(|_| ())?;
        self.parsed_timezone = parsed_tz;
        println!("TimeManager: Timezone set to {} (includes DST support)", tz);
        Ok(())
    }

    pub fn update_time(&mut self, time: DateTime<Utc>) {
        self.last_sync = Some(time);
        println!("TimeManager: Time updated to {}", time);
    }

    pub fn get_current_time(&self) -> Option<DateTime<Utc>> {
        // TODO: Add elapsed time calculation from last sync using embassy_time
        // For now, return the last synced time
        self.last_sync
    }
    
    pub fn get_local_time(&self) -> Option<chrono::DateTime<Tz>> {
        self.last_sync.map(|utc_time| {
            utc_time.with_timezone(&self.parsed_timezone)
        })
    }

    pub fn get_timezone(&self) -> &str {
        &self.timezone
    }

    pub fn needs_sync(&self) -> bool {
        match self.last_sync {
            None => true,
            Some(_last) => {
                // For now, always return false after first sync
                // TODO: Implement proper time tracking with embassy_time
                false
            }
        }
    }
}