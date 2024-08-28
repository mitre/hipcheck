#![allow(unused_variables)]

mod hipcheck;
mod hipcheck_transport;

use crate::hipcheck_transport::*;
use anyhow::{anyhow, Result};
use clap::Parser;
use hipcheck::plugin_server::{Plugin, PluginServer};
use hipcheck::{
	Configuration, ConfigurationResult, ConfigurationStatus, Empty, PolicyExpression,
	Query as PluginQuery, Schema,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

static SHA256_KEY_SCHEMA: &str = include_str!("query_schema_sha256.json");
static SHA256_OUTPUT_SCHEMA: &str = include_str!("query_schema_sha256.json");

fn sha256(content: Vec<u8>) -> Vec<u8> {
	let mut hasher = Sha256::new();
	hasher.update(content);
	hasher.finalize().to_vec()
}

pub async fn handle_sha256(channel: HcTransport, id: usize, key: Vec<u8>) -> Result<()> {
	println!("SHA256-{id}: Key: {key:02x?}");
	let res = sha256(key);
	println!("SHA256-{id}: Hash: {res:02x?}");
	let output = serde_json::to_value(res)?;
	let resp = Query {
		id,
		request: false,
		publisher: "".to_owned(),
		plugin: "".to_owned(),
		query: "".to_owned(),
		key: json!(null),
		output,
	};
	channel.send(resp).await?;
	Ok(())
}
struct Sha256Runner {
	channel: HcTransport,
}
impl Sha256Runner {
	pub fn new(channel: HcTransport) -> Self {
		Sha256Runner { channel }
	}
	async fn handle_query(channel: HcTransport, id: usize, name: String, key: Value) -> Result<()> {
		if name == "sha256" {
			let Value::Array(val_vec) = &key else {
				return Err(anyhow!("get_rand argument must be a number"));
			};
			let byte_vec = val_vec
				.iter()
				.map(|x| {
					let Value::Number(val_byte) = x else {
						return Err(anyhow!("expected all integers"));
					};
					let Some(byte) = val_byte.as_u64() else {
						return Err(anyhow!(
							"sha256 input array must contain only unsigned integers"
						));
					};
					Ok(byte as u8)
				})
				.collect::<Result<Vec<u8>>>()?;
			handle_sha256(channel, id, byte_vec).await?;
			Ok(())
		} else {
			Err(anyhow!("unrecognized query '{}'", name))
		}
	}
	pub async fn run(self) -> Result<()> {
		loop {
			eprintln!("SHA256: Looping");
			let Some(msg) = self.channel.recv_new().await? else {
				eprintln!("Channel closed by remote");
				break;
			};
			if msg.request {
				let child_channel = self.channel.clone();
				tokio::spawn(async move {
					if let Err(e) =
						Sha256Runner::handle_query(child_channel, msg.id, msg.query, msg.key).await
					{
						eprintln!("handle_query failed: {e}");
					};
				});
			} else {
				return Err(anyhow!("Did not expect a response-type message here"));
			}
		}
		Ok(())
	}
}

#[derive(Debug)]
struct RandDataPlugin {
	pub schema: Schema,
}
impl RandDataPlugin {
	pub fn new() -> Self {
		let schema = Schema {
			query_name: "sha256".to_owned(),
			key_schema: SHA256_KEY_SCHEMA.to_owned(),
			output_schema: SHA256_OUTPUT_SCHEMA.to_owned(),
		};
		RandDataPlugin { schema }
	}
}

#[tonic::async_trait]
impl Plugin for RandDataPlugin {
	type GetQuerySchemasStream =
		Pin<Box<dyn Stream<Item = Result<Schema, Status>> + Send + 'static>>;
	type InitiateQueryProtocolStream = ReceiverStream<Result<PluginQuery, Status>>;
	async fn get_query_schemas(
		&self,
		_request: Request<Empty>,
	) -> Result<Response<Self::GetQuerySchemasStream>, Status> {
		Ok(Response::new(Box::pin(tokio_stream::iter(vec![Ok(self
			.schema
			.clone())]))))
	}
	async fn set_configuration(
		&self,
		request: Request<Configuration>,
	) -> Result<Response<ConfigurationResult>, Status> {
		Ok(Response::new(ConfigurationResult {
			status: ConfigurationStatus::ErrorNone as i32,
			message: "".to_owned(),
		}))
	}
	async fn get_default_policy_expression(
		&self,
		request: Request<Empty>,
	) -> Result<Response<PolicyExpression>, Status> {
		Ok(Response::new(PolicyExpression {
			policy_expression: "".to_owned(),
		}))
	}
	async fn initiate_query_protocol(
		&self,
		request: Request<Streaming<PluginQuery>>,
	) -> Result<Response<Self::InitiateQueryProtocolStream>, Status> {
		let rx = request.into_inner();
		let (tx, out_rx) = mpsc::channel::<Result<PluginQuery, Status>>(4);
		tokio::spawn(async move {
			let channel = HcTransport::new(rx, tx);
			if let Err(e) = Sha256Runner::new(channel).run().await {
				eprintln!("sha256 plugin ended in error: {e}");
			}
		});
		Ok(Response::new(ReceiverStream::new(out_rx)))
	}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::try_parse().map_err(Box::new)?;
	let addr = format!("127.0.0.1:{}", args.port);
	let plugin = RandDataPlugin::new();
	let svc = PluginServer::new(plugin);
	Server::builder()
		.add_service(svc)
		.serve(addr.parse().unwrap())
		.await?;
	Ok(())
}
