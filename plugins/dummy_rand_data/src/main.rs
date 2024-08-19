#![allow(unused_variables)]

mod hipcheck;
mod hipcheck_transport;

use crate::hipcheck_transport::*;
use clap::Parser;
use hipcheck::plugin_server::{Plugin, PluginServer};
use hipcheck::{
	Configuration, ConfigurationResult, ConfigurationStatus, Empty, PolicyExpression,
	Query as PluginQuery, Schema,
};
use rand::Rng;
use serde_json::{json, Value};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

static GET_RAND_KEY_SCHEMA: &str = include_str!("query_schema_get_rand.json");
static GET_RAND_OUTPUT_SCHEMA: &str = include_str!("query_schema_get_rand.json");

fn get_rand(num_bytes: usize) -> Vec<u8> {
	let mut vec = vec![0u8; num_bytes];
	let mut rng = rand::thread_rng();
	rng.fill(vec.as_mut_slice());
	vec
}

struct RandDataRunner {
	channel: HcTransport,
}
impl RandDataRunner {
	pub fn new(channel: HcTransport) -> Self {
		RandDataRunner { channel }
	}
	pub fn handle_query(id: usize, name: String, key: Value) -> Result<Query, String> {
		if name == "rand_data" {
			let Value::Number(num_size) = &key else {
				return Err("get_rand argument must be a number".to_owned());
			};
			let Some(size) = num_size.as_u64() else {
				return Err("get_rand argument must be an unsigned integer".to_owned());
			};
			let res = get_rand(size as usize);
			let output = serde_json::to_value(res).map_err(|e| e.to_string())?;
			Ok(Query {
				id,
				request: false,
				publisher: "".to_owned(),
				plugin: "".to_owned(),
				query: "".to_owned(),
				key: json!(null),
				output,
			})
		} else {
			Err(format!("unrecognized query '{name}'"))
		}
	}
	pub async fn run(mut self) -> Result<(), String> {
		loop {
			eprintln!("Looping");
			let Some(msg) = self.channel.recv().await? else {
				eprintln!("Channel closed by remote");
				break;
			};
			if msg.request {
				let rsp = RandDataRunner::handle_query(msg.id, msg.query, msg.key)?;
				eprintln!("Sending response: {rsp:?}");
				self.channel.send(rsp).await?;
			} else {
				return Err("Did not expect a response-type message here".to_owned());
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
			query_name: "rand_data".to_owned(),
			key_schema: GET_RAND_KEY_SCHEMA.to_owned(),
			output_schema: GET_RAND_OUTPUT_SCHEMA.to_owned(),
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
			if let Err(e) = RandDataRunner::new(channel).run().await {
				eprintln!("rand_data plugin ended in error: {e}");
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
