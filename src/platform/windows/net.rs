use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP_STATE, MIB_TCPROW_OWNER_PID,
    MIB_TCPTABLE_OWNER_PID, MIB_UDPROW_OWNER_PID, MIB_UDPTABLE_OWNER_PID,
    TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows::Win32::Networking::WinSock::AF_INET;

use crate::domain::{ProbeError, Protocol, SocketRow, SocketState};
use crate::platform::NetProbe;

pub struct WindowsNetProbe;

impl NetProbe for WindowsNetProbe {
    fn list_sockets(&self) -> Result<Vec<SocketRow>, ProbeError> {
        let mut rows = Vec::new();
        rows.extend(tcp4_table()?);
        rows.extend(udp4_table()?);
        Ok(rows)
    }
}

fn tcp4_table() -> Result<Vec<SocketRow>, ProbeError> {
    let buf = query_table(|size, ptr| unsafe {
        GetExtendedTcpTable(
            ptr,
            size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    })?;
    if buf.len() < 4 {
        return Ok(Vec::new());
    }
    let table = unsafe { &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
    let count = table.dwNumEntries as usize;
    let first = table.table.as_ptr();
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let row = unsafe { &*first.add(i) };
        out.push(tcp4_row(row));
    }
    Ok(out)
}

fn udp4_table() -> Result<Vec<SocketRow>, ProbeError> {
    let buf = query_table(|size, ptr| unsafe {
        GetExtendedUdpTable(
            ptr,
            size,
            false,
            AF_INET.0 as u32,
            UDP_TABLE_OWNER_PID,
            0,
        )
    })?;
    if buf.len() < 4 {
        return Ok(Vec::new());
    }
    let table = unsafe { &*(buf.as_ptr() as *const MIB_UDPTABLE_OWNER_PID) };
    let count = table.dwNumEntries as usize;
    let first = table.table.as_ptr();
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let row = unsafe { &*first.add(i) };
        out.push(udp4_row(row));
    }
    Ok(out)
}

fn query_table(
    mut get: impl FnMut(*mut u32, Option<*mut core::ffi::c_void>) -> u32,
) -> Result<Vec<u8>, ProbeError> {
    let mut size: u32 = 0;
    let _ = get(&mut size, None);
    if size == 0 {
        return Ok(Vec::new());
    }
    let mut buf = vec![0u8; size as usize];
    let mut status = get(&mut size, Some(buf.as_mut_ptr().cast()));
    if status == 122 {
        buf.resize(size as usize, 0);
        status = get(&mut size, Some(buf.as_mut_ptr().cast()));
    }
    if status != 0 {
        return Err(ProbeError::msg(format!(
            "IP helper table query failed: {status}"
        )));
    }
    Ok(buf)
}

fn tcp4_row(row: &MIB_TCPROW_OWNER_PID) -> SocketRow {
    let local = SocketAddr::new(
        IpAddr::V4(u32_to_ipv4(row.dwLocalAddr)),
        u16_from_be_port(row.dwLocalPort),
    );
    let remote = SocketAddr::new(
        IpAddr::V4(u32_to_ipv4(row.dwRemoteAddr)),
        u16_from_be_port(row.dwRemotePort),
    );
    SocketRow {
        protocol: Protocol::Tcp,
        local,
        remote: Some(remote),
        state: map_tcp_state(row.dwState),
        pid: row.dwOwningPid,
    }
}

fn udp4_row(row: &MIB_UDPROW_OWNER_PID) -> SocketRow {
    let local = SocketAddr::new(
        IpAddr::V4(u32_to_ipv4(row.dwLocalAddr)),
        u16_from_be_port(row.dwLocalPort),
    );
    SocketRow {
        protocol: Protocol::Udp,
        local,
        remote: None,
        state: SocketState::Other,
        pid: row.dwOwningPid,
    }
}

fn u32_to_ipv4(addr: u32) -> Ipv4Addr {
    Ipv4Addr::from(addr.to_ne_bytes())
}

fn u16_from_be_port(port: u32) -> u16 {
    u16::from_be((port & 0xFFFF) as u16)
}

fn map_tcp_state(state: u32) -> SocketState {
    match MIB_TCP_STATE(state as i32) {
        s if s.0 == 2 => SocketState::Listen,
        s if s.0 == 5 => SocketState::Established,
        s if s.0 == 8 => SocketState::CloseWait,
        s if s.0 == 11 => SocketState::TimeWait,
        s if s.0 == 3 => SocketState::SynSent,
        s if s.0 == 4 => SocketState::SynRecv,
        s if s.0 == 6 => SocketState::FinWait1,
        s if s.0 == 7 => SocketState::FinWait2,
        s if s.0 == 9 => SocketState::Closing,
        s if s.0 == 10 => SocketState::LastAck,
        s if s.0 == 1 => SocketState::Closed,
        _ => SocketState::Other,
    }
}
