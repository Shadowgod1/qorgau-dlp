//! Native Win32 TCP & UDP connection monitor using `GetExtendedTcpTable` + `GetExtendedUdpTable`.
//! Captures both TCP and UDP (fixes legacy bug where UDP was dropped), with 300s dedup TTL.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::collectors::win_util::get_process_name_and_path;
use crate::transport::AgentTransport;

const DEDUP_TTL: Duration = Duration::from_secs(300);

#[derive(Debug)]
struct NetConn {
    local_ip: String,
    local_port: u16,
    remote_ip: String,
    remote_port: u16,
    protocol: &'static str,
    status: &'static str,
    pid: u32,
}

pub async fn run(transport: Arc<AgentTransport>, mut enabled: tokio::sync::watch::Receiver<bool>) {
    let mut seen: HashMap<String, Instant> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(15));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            continue;
        }

        let now = Instant::now();
        seen.retain(|_, ts| now.duration_since(*ts) < DEDUP_TTL);

        let conns = enumerate_connections();
        for c in conns {
            if c.remote_ip == "127.0.0.1" || c.remote_ip == "0.0.0.0" {
                continue;
            }
            let key = format!("{}:{}:{}:{}", c.protocol, c.pid, c.remote_ip, c.remote_port);
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key, now);

            let (proc_name, exe_path) = get_process_name_and_path(c.pid);
            transport.enqueue(
                "network",
                json!({
                    "local_ip": c.local_ip,
                    "local_port": c.local_port,
                    "remote_ip": c.remote_ip,
                    "remote_port": c.remote_port,
                    "protocol": c.protocol,
                    "status": c.status,
                    "process_name": proc_name,
                    "exe_path": exe_path,
                    "bytes_sent": 0,
                    "bytes_recv": 0,
                }),
            );
        }
    }
}

#[cfg(windows)]
fn enumerate_connections() -> Vec<NetConn> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCPTABLE_OWNER_PID, MIB_UDPTABLE_OWNER_PID,
        TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    let mut out = Vec::new();

    // 1. IPv4 TCP table
    unsafe {
        let mut size = 0u32;
        let _ = GetExtendedTcpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            if GetExtendedTcpTable(
                Some(buf.as_mut_ptr() as *mut _),
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            ) == 0
            {
                let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
                let rows = std::slice::from_raw_parts(
                    table.table.as_ptr(),
                    table.dwNumEntries as usize,
                );
                for r in rows {
                    let status = match r.dwState {
                        5 => "ESTABLISHED",
                        3 => "SYN_SENT",
                        8 => "CLOSE_WAIT",
                        _ => continue,
                    };
                    let lip = Ipv4Addr::from(u32::from_be(r.dwLocalAddr)).to_string();
                    let rip = Ipv4Addr::from(u32::from_be(r.dwRemoteAddr)).to_string();
                    let lport = u16::from_be(r.dwLocalPort as u16);
                    let rport = u16::from_be(r.dwRemotePort as u16);
                    out.push(NetConn {
                        local_ip: lip,
                        local_port: lport,
                        remote_ip: rip,
                        remote_port: rport,
                        protocol: "tcp",
                        status,
                        pid: r.dwOwningPid,
                    });
                }
            }
        }
    }

    // 2. IPv4 UDP table (captures bound UDP sockets with owning process)
    unsafe {
        let mut size = 0u32;
        let _ = GetExtendedUdpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            UDP_TABLE_OWNER_PID,
            0,
        );
        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            if GetExtendedUdpTable(
                Some(buf.as_mut_ptr() as *mut _),
                &mut size,
                false,
                AF_INET.0 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            ) == 0
            {
                let table = &*(buf.as_ptr() as *const MIB_UDPTABLE_OWNER_PID);
                let rows = std::slice::from_raw_parts(
                    table.table.as_ptr(),
                    table.dwNumEntries as usize,
                );
                for r in rows {
                    let lip = Ipv4Addr::from(u32::from_be(r.dwLocalAddr)).to_string();
                    let lport = u16::from_be(r.dwLocalPort as u16);
                    if lport == 0 {
                        continue;
                    }
                    out.push(NetConn {
                        local_ip: lip.clone(),
                        local_port: lport,
                        remote_ip: lip,
                        remote_port: lport,
                        protocol: "udp",
                        status: "BOUND",
                        pid: r.dwOwningPid,
                    });
                }
            }
        }
    }

    out
}

#[cfg(not(windows))]
fn enumerate_connections() -> Vec<NetConn> {
    Vec::new()
}
