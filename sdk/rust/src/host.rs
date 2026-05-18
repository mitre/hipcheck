// SPDX-License-Identifier: Apache-2.0

use std::net::{Ipv4Addr, SocketAddr};

#[derive(Debug, Clone)]
pub enum Host {
	// 127.0.0.1
	Loopback,
	// 0.0.0.0
	Any,
	// Any other IP address.
	Other(Ipv4Addr),
}

impl Host {
	pub fn to_socket_addr(&self, port: u16) -> SocketAddr {
		match self {
			Host::Loopback => SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), port),
			Host::Any => SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), port),
			Host::Other(ip) => SocketAddr::new((*ip).into(), port),
		}
	}
}
